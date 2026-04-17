//! Ethereum transaction construction and signing.
//!
//! Derives the secp256k1 private key internally from the seed via BIP-32,
//! signs EIP-1559 (type 2) transactions, and zeroizes the private key immediately.
//!
//! The private key NEVER leaves this module.

use alloy::consensus::{SignableTransaction, TxEip1559};
use alloy::eips::eip2718::Encodable2718;
use alloy::network::TxSignerSync;
use alloy::primitives::{Address, Bytes, TxKind, U256};
use alloy::signers::local::PrivateKeySigner;
use anyhow::{Context, Result};
use bip32::{DerivationPath, XPrv};
use zeroize::Zeroize;

/// Parameters for an ETH native transfer.
#[derive(Debug, Clone)]
pub struct EthTransferParams {
    pub to: Address,
    pub value_wei: U256,
    pub nonce: u64,
    pub chain_id: u64,
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
    pub gas_limit: u64,
}

impl Default for EthTransferParams {
    fn default() -> Self {
        Self {
            to: Address::ZERO,
            value_wei: U256::ZERO,
            nonce: 0,
            chain_id: 1, // mainnet
            max_fee_per_gas: 0,
            max_priority_fee_per_gas: 0,
            gas_limit: 21_000, // standard ETH transfer
        }
    }
}

/// Parameters for a generic payable contract call (Lido submit,
/// ERC-20 approve, etc.) — anything where the tx has arbitrary
/// `input` calldata AND may carry non-zero `value`.
#[derive(Debug, Clone)]
pub struct EthContractCallParams {
    pub to: Address,
    pub value_wei: U256,
    pub input: Bytes,
    pub nonce: u64,
    pub chain_id: u64,
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
    pub gas_limit: u64,
}

impl EthContractCallParams {
    /// Builder: native-value call to `to` with raw calldata. Fills
    /// the remaining knobs with conservative defaults that must be
    /// overwritten before signing (nonce + fees).
    pub fn new(to: Address, value_wei: U256, input: Bytes, chain_id: u64) -> Self {
        Self {
            to, value_wei, input,
            nonce: 0, chain_id,
            max_fee_per_gas: 0, max_priority_fee_per_gas: 0,
            gas_limit: 100_000,
        }
    }
}

/// Parameters for an ERC-20 token transfer.
#[derive(Debug, Clone)]
pub struct Erc20TransferParams {
    pub token_contract: Address,
    pub to: Address,
    pub amount: U256,
    pub nonce: u64,
    pub chain_id: u64,
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
    pub gas_limit: u64,
}

impl Default for Erc20TransferParams {
    fn default() -> Self {
        Self {
            token_contract: Address::ZERO,
            to: Address::ZERO,
            amount: U256::ZERO,
            nonce: 0,
            chain_id: 1,
            max_fee_per_gas: 0,
            max_priority_fee_per_gas: 0,
            gas_limit: 65_000, // typical ERC-20 transfer
        }
    }
}

/// Derive the Ethereum signing key from seed at m/44'/60'/0'/0/{index}.
///
/// The private key is returned as a PrivateKeySigner and must be dropped
/// (zeroized) immediately after use.
fn derive_eth_signer(seed: &[u8; 64], index: u32) -> Result<PrivateKeySigner> {
    let path: DerivationPath = format!("m/44'/60'/0'/0/{index}")
        .parse()
        .context("Invalid derivation path")?;

    let xprv = XPrv::derive_from_path(seed, &path)
        .context("BIP-32 key derivation failed")?;

    // Extract the raw 32-byte private key
    let mut privkey_bytes = xprv.to_bytes();

    let signing_key = k256::ecdsa::SigningKey::from_bytes((&privkey_bytes).into())
        .context("Invalid secp256k1 private key")?;

    // Zeroize the raw bytes immediately
    privkey_bytes.zeroize();

    let signer = PrivateKeySigner::from_signing_key(signing_key);
    Ok(signer)
}

/// Get the Ethereum address for a given seed and derivation index.
pub fn eth_address(seed: &[u8; 64], index: u32) -> Result<Address> {
    let signer = derive_eth_signer(seed, index)?;
    Ok(signer.address())
}

/// Build the unsigned `TxEip1559` for an ETH transfer. Factored out
/// so both the seed-signing and HSM-signing paths share the same
/// request shape.
fn build_eth_transfer_tx(params: &EthTransferParams) -> TxEip1559 {
    TxEip1559 {
        chain_id: params.chain_id,
        nonce: params.nonce,
        gas_limit: params.gas_limit,
        max_fee_per_gas: params.max_fee_per_gas,
        max_priority_fee_per_gas: params.max_priority_fee_per_gas,
        to: TxKind::Call(params.to),
        value: params.value_wei,
        input: Bytes::new(),
        access_list: Default::default(),
    }
}

/// Build the unsigned `TxEip1559` for an ERC-20 transfer.
fn build_erc20_transfer_tx(params: &Erc20TransferParams, input: Bytes) -> TxEip1559 {
    TxEip1559 {
        chain_id: params.chain_id,
        nonce: params.nonce,
        gas_limit: params.gas_limit,
        max_fee_per_gas: params.max_fee_per_gas,
        max_priority_fee_per_gas: params.max_priority_fee_per_gas,
        to: TxKind::Call(params.token_contract),
        value: U256::ZERO,
        input,
        access_list: Default::default(),
    }
}

/// Attach an externally-produced `r || s || v` (65 bytes) signature
/// to a `TxEip1559` and emit the RLP-encoded signed transaction.
///
/// This is the sign-once-off-box step for HSM / MPC / signer-service
/// paths: caller computes the signature hash via
/// `SignableTransaction::signature_hash`, hands the 32-byte digest to
/// the backend, and passes the resulting signature bytes back here.
/// The backend's `v` must be 0 or 1 (pure recovery id) — EIP-1559
/// does NOT re-derive `v` from chain_id.
pub fn finalize_eth_signed_tx(tx: TxEip1559, sig_65: &[u8]) -> Result<Vec<u8>> {
    use alloy::primitives::Signature;
    if sig_65.len() != 65 {
        anyhow::bail!("expected 65-byte (r||s||v) signature, got {}", sig_65.len());
    }
    let mut r = [0u8; 32];
    let mut s = [0u8; 32];
    r.copy_from_slice(&sig_65[..32]);
    s.copy_from_slice(&sig_65[32..64]);
    let v = sig_65[64];
    if v > 1 {
        anyhow::bail!("EIP-1559 signature v must be 0 or 1, got {v}");
    }
    let sig = Signature::new(U256::from_be_bytes(r), U256::from_be_bytes(s), v == 1);
    let signed = tx.into_signed(sig);
    let envelope = alloy::consensus::transaction::TxEnvelope::from(signed);
    let mut buf = Vec::new();
    Encodable2718::encode_2718(&envelope, &mut buf);
    Ok(buf)
}

/// Build the signature hash an external signer (HSM / MPC / signer
/// service) must sign for an ETH native transfer. Caller combines
/// the hash with the backend to produce a 65-byte `r||s||v`, then
/// calls `finalize_eth_signed_tx` with the same `TxEip1559`.
pub fn prepare_eth_transfer(params: &EthTransferParams) -> (TxEip1559, [u8; 32]) {
    let tx = build_eth_transfer_tx(params);
    let hash = tx.signature_hash().0;
    (tx, hash)
}

/// Same, for a generic payable contract call (Lido stake,
/// WalletConnect-driven approve, etc.).
pub fn prepare_eth_contract_call(params: &EthContractCallParams) -> (TxEip1559, [u8; 32]) {
    let tx = TxEip1559 {
        chain_id: params.chain_id,
        nonce: params.nonce,
        gas_limit: params.gas_limit,
        max_fee_per_gas: params.max_fee_per_gas,
        max_priority_fee_per_gas: params.max_priority_fee_per_gas,
        to: TxKind::Call(params.to),
        value: params.value_wei,
        input: params.input.clone(),
        access_list: Default::default(),
    };
    let hash = tx.signature_hash().0;
    (tx, hash)
}

/// Seed-signed variant for callers still on the local-key path
/// (unit tests, pre-HSM deployments). HSM / MPC callers should use
/// `prepare_eth_contract_call` + the backend.
pub fn sign_eth_contract_call(
    seed: &[u8; 64],
    index: u32,
    params: &EthContractCallParams,
) -> Result<Vec<u8>> {
    let signer = derive_eth_signer(seed, index)?;
    let mut tx = TxEip1559 {
        chain_id: params.chain_id,
        nonce: params.nonce,
        gas_limit: params.gas_limit,
        max_fee_per_gas: params.max_fee_per_gas,
        max_priority_fee_per_gas: params.max_priority_fee_per_gas,
        to: TxKind::Call(params.to),
        value: params.value_wei,
        input: params.input.clone(),
        access_list: Default::default(),
    };
    let sig = signer
        .sign_transaction_sync(&mut tx)
        .context("Failed to sign ETH contract call")?;
    let signed = tx.into_signed(sig);
    let envelope = alloy::consensus::transaction::TxEnvelope::from(signed);
    let mut buf = Vec::new();
    Encodable2718::encode_2718(&envelope, &mut buf);
    Ok(buf)
}

/// Same, for ERC-20 transfers.
pub fn prepare_erc20_transfer(params: &Erc20TransferParams) -> (TxEip1559, [u8; 32]) {
    let calldata = encode_erc20_transfer_calldata(params.to, params.amount);
    let tx = build_erc20_transfer_tx(params, calldata);
    let hash = tx.signature_hash().0;
    (tx, hash)
}

/// Sign an EIP-1559 ETH native transfer.
///
/// Returns the RLP-encoded signed transaction ready for `eth_sendRawTransaction`.
pub fn sign_eth_transfer(
    seed: &[u8; 64],
    index: u32,
    params: &EthTransferParams,
) -> Result<Vec<u8>> {
    let signer = derive_eth_signer(seed, index)?;

    let mut tx = TxEip1559 {
        chain_id: params.chain_id,
        nonce: params.nonce,
        gas_limit: params.gas_limit,
        max_fee_per_gas: params.max_fee_per_gas,
        max_priority_fee_per_gas: params.max_priority_fee_per_gas,
        to: TxKind::Call(params.to),
        value: params.value_wei,
        input: Bytes::new(),
        access_list: Default::default(),
    };

    let signed = signer
        .sign_transaction_sync(&mut tx)
        .context("Failed to sign ETH transfer")?;

    let envelope = tx.into_signed(signed);
    let encoded = alloy::consensus::transaction::TxEnvelope::from(envelope);
    let mut buf = Vec::new();
    Encodable2718::encode_2718(&encoded, &mut buf);

    tracing::info!(
        to = %params.to,
        nonce = params.nonce,
        "ETH transfer signed"
    );

    Ok(buf)
}

/// Encode ERC-20 `transfer(address,uint256)` calldata.
///
/// Function selector: 0xa9059cbb
pub fn encode_erc20_transfer_calldata(to: Address, amount: U256) -> Bytes {
    let mut calldata = Vec::with_capacity(68);

    // Function selector: transfer(address,uint256) = 0xa9059cbb
    calldata.extend_from_slice(&[0xa9, 0x05, 0x9c, 0xbb]);

    // address padded to 32 bytes
    let mut addr_padded = [0u8; 32];
    addr_padded[12..].copy_from_slice(to.as_ref());
    calldata.extend_from_slice(&addr_padded);

    // uint256 amount (32 bytes, big-endian)
    calldata.extend_from_slice(&amount.to_be_bytes::<32>());

    Bytes::from(calldata)
}

/// Sign an EIP-1559 ERC-20 token transfer.
///
/// Returns the RLP-encoded signed transaction ready for `eth_sendRawTransaction`.
pub fn sign_erc20_transfer(
    seed: &[u8; 64],
    index: u32,
    params: &Erc20TransferParams,
) -> Result<Vec<u8>> {
    let signer = derive_eth_signer(seed, index)?;
    let calldata = encode_erc20_transfer_calldata(params.to, params.amount);

    let mut tx = TxEip1559 {
        chain_id: params.chain_id,
        nonce: params.nonce,
        gas_limit: params.gas_limit,
        max_fee_per_gas: params.max_fee_per_gas,
        max_priority_fee_per_gas: params.max_priority_fee_per_gas,
        to: TxKind::Call(params.token_contract),
        value: U256::ZERO, // ERC-20 transfers send 0 ETH
        input: calldata,
        access_list: Default::default(),
    };

    let signed = signer
        .sign_transaction_sync(&mut tx)
        .context("Failed to sign ERC-20 transfer")?;

    let envelope = tx.into_signed(signed);
    let encoded = alloy::consensus::transaction::TxEnvelope::from(envelope);
    let mut buf = Vec::new();
    Encodable2718::encode_2718(&encoded, &mut buf);

    tracing::info!(
        token = %params.token_contract,
        to = %params.to,
        nonce = params.nonce,
        "ERC-20 transfer signed"
    );

    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_seed() -> [u8; 64] {
        let bytes = hex::decode(
            "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
        ).unwrap();
        let mut seed = [0u8; 64];
        seed.copy_from_slice(&bytes);
        seed
    }

    #[test]
    fn eth_address_from_abandon_mnemonic() {
        let seed = test_seed();
        let addr = eth_address(&seed, 0).unwrap();
        // Same as custody-address-factory ethereum test
        assert_eq!(
            format!("{addr:#x}"),
            "0x9858effd232b4033e47d90003d41ec34ecaeda94"
        );
    }

    #[test]
    fn sign_eth_transfer_produces_valid_tx() {
        let seed = test_seed();
        let params = EthTransferParams {
            to: Address::from_slice(&[0x42u8; 20]),
            value_wei: U256::from(1_000_000_000_000_000_000u128), // 1 ETH
            nonce: 0,
            chain_id: 1,
            max_fee_per_gas: 30_000_000_000, // 30 gwei
            max_priority_fee_per_gas: 2_000_000_000, // 2 gwei
            gas_limit: 21_000,
        };

        let signed_tx = sign_eth_transfer(&seed, 0, &params).unwrap();
        // EIP-1559 type 2 transactions start with 0x02
        assert_eq!(signed_tx[0], 0x02, "Must be EIP-1559 type 2 transaction");
        assert!(signed_tx.len() > 100, "Signed tx should be substantial");
    }

    #[test]
    fn erc20_calldata_encoding() {
        let to = Address::from_slice(&[0xABu8; 20]);
        let amount = U256::from(1_000_000u128); // 1 USDT

        let calldata = encode_erc20_transfer_calldata(to, amount);

        // Function selector: 0xa9059cbb
        assert_eq!(&calldata[..4], &[0xa9, 0x05, 0x9c, 0xbb]);
        // Total length: 4 (selector) + 32 (address) + 32 (amount) = 68
        assert_eq!(calldata.len(), 68);
        // Address in bytes 16..36 (padded at 12..32 of the 32-byte slot)
        assert_eq!(&calldata[16..36], &[0xABu8; 20]);
    }

    #[test]
    fn erc20_calldata_usdt_1_token() {
        let to = Address::from_slice(&[0x01u8; 20]);
        let amount = U256::from(1_000_000u128); // 1 USDT (6 decimals)

        let calldata = encode_erc20_transfer_calldata(to, amount);

        // Last 32 bytes = amount
        let amount_bytes = &calldata[36..68];
        let decoded = U256::from_be_slice(amount_bytes);
        assert_eq!(decoded, U256::from(1_000_000u128));
    }

    #[test]
    fn sign_erc20_transfer_produces_valid_tx() {
        let seed = test_seed();
        let params = Erc20TransferParams {
            token_contract: Address::from_slice(&[0xDA; 20]), // mock USDT contract
            to: Address::from_slice(&[0x42u8; 20]),
            amount: U256::from(1_000_000u128), // 1 USDT
            nonce: 5,
            chain_id: 1,
            max_fee_per_gas: 50_000_000_000,
            max_priority_fee_per_gas: 2_000_000_000,
            gas_limit: 65_000,
        };

        let signed_tx = sign_erc20_transfer(&seed, 0, &params).unwrap();
        assert_eq!(signed_tx[0], 0x02, "Must be EIP-1559 type 2 transaction");
    }

    #[test]
    fn different_indices_produce_different_signers() {
        let seed = test_seed();
        let addr0 = eth_address(&seed, 0).unwrap();
        let addr1 = eth_address(&seed, 1).unwrap();
        assert_ne!(addr0, addr1);
    }
}
