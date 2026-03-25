//! Tron transaction signing.
//!
//! Tron uses the same secp256k1 curve as Ethereum for signing,
//! but with different transaction format (protobuf-like JSON via HTTP API).
//!
//! Key derivation: m/44'/195'/0'/0/{index} (same as Phase 1 address-factory).

use anyhow::Result;

/// Parameters for a TRX native transfer.
#[derive(Debug, Clone)]
pub struct TrxTransferParams {
    pub from_address: String,
    pub to_address: String,
    pub amount_sun: u64,
}

/// Parameters for a TRC-20 token transfer.
#[derive(Debug, Clone)]
pub struct Trc20TransferParams {
    pub from_address: String,
    pub contract_address: String,
    pub to_address: String,
    pub amount: u64, // in token base units
}

/// Sign a TRX native transfer.
pub fn sign_trx_transfer(
    _seed: &[u8; 64],
    _index: u32,
    _params: &TrxTransferParams,
) -> Result<Vec<u8>> {
    // TODO: Implement using Tron's transaction format
    // 1. Build raw transaction via Tron HTTP API (createtransaction)
    // 2. Hash the raw transaction with SHA256
    // 3. Sign with secp256k1 key at m/44'/195'/0'/0/{index}
    // 4. Attach signature to the transaction
    anyhow::bail!("Tron signing not yet implemented")
}

/// Sign a TRC-20 transfer.
pub fn sign_trc20_transfer(
    _seed: &[u8; 64],
    _index: u32,
    _params: &Trc20TransferParams,
) -> Result<Vec<u8>> {
    // TODO: Implement using triggersmartcontract API
    // The TRC-20 transfer ABI is identical to ERC-20:
    // function transfer(address,uint256)
    // selector: a9059cbb
    anyhow::bail!("TRC-20 signing not yet implemented")
}

/// Encode TRC-20 transfer parameters.
///
/// Same ABI as ERC-20: transfer(address,uint256)
/// But Tron addresses need to be converted from Base58Check to hex (strip 0x41 prefix).
pub fn encode_trc20_transfer_params(to_address_hex: &str, amount: u64) -> String {
    // Function selector (same as ERC-20)
    let selector = "a9059cbb";

    // Pad address to 32 bytes (remove leading "41" Tron prefix if present)
    let addr_clean = to_address_hex.trim_start_matches("41");
    let addr_padded = format!("{:0>64}", addr_clean);

    // Amount as 32-byte hex
    let amount_hex = format!("{:064x}", amount);

    format!("{selector}{addr_padded}{amount_hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trc20_param_encoding() {
        let params = encode_trc20_transfer_params(
            "41abcdef1234567890abcdef1234567890abcdef12",
            1_000_000, // 1 USDT
        );

        // Starts with transfer selector
        assert!(params.starts_with("a9059cbb"));
        // Total length: 8 (selector) + 64 (address) + 64 (amount) = 136
        assert_eq!(params.len(), 136);
        // Amount in last 64 chars
        assert!(params.ends_with(&format!("{:064x}", 1_000_000)));
    }
}
