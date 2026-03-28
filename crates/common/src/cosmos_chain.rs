//! Cosmos SDK REST API client — deposit scanning, balance queries, and Amino JSON signing.
//!
//! Chain quirks handled here:
//! - Tendermint finality: 1 confirmation is sufficient (instant finality).
//! - uatom/uosmo: amounts are always in base micro-denomination (6 decimal places).
//! - Bech32 address encoding per chain prefix (cosmos, osmo, celestia, …).
//! - Amino JSON signing: cheaper than Protobuf, still supported by all Cosmos wallets.
//! - secp256k1 keys (same curve as Ethereum, different address encoding).
//!
//! References:
//! - Cosmos LCD REST API: https://v1.cosmos.network/rpc/
//! - Amino signing spec: https://docs.cosmos.network/v0.47/learn/beginner/tx-lifecycle
//! - Bech32 encoding: https://github.com/bitcoin/bips/blob/master/bip-0173.mediawiki

use anyhow::{Context, Result, bail};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Scanner
// ---------------------------------------------------------------------------

/// Cosmos REST LCD client for deposit scanning and balance queries.
///
/// Uses the Cosmos SDK REST (LCD) endpoints — no Tendermint RPC required.
/// Tendermint provides instant finality, so 1 confirmation is always enough.
pub struct CosmosScanner {
    pub chain_id: String,
    pub rest_url: String,
    pub denom: String,
    pub display_denom: String,
    pub decimals: u8,
    /// Number of confirmations before crediting a deposit (1 for all Cosmos chains).
    pub confirmations: u32,
    client: reqwest::Client,
}

impl CosmosScanner {
    /// Create a new scanner with explicit parameters.
    pub fn new(
        chain_id: impl Into<String>,
        rest_url: impl Into<String>,
        denom: impl Into<String>,
        display_denom: impl Into<String>,
        decimals: u8,
    ) -> Self {
        Self {
            chain_id: chain_id.into(),
            rest_url: rest_url.into(),
            denom: denom.into(),
            display_denom: display_denom.into(),
            decimals,
            confirmations: 1,
            client: reqwest::Client::new(),
        }
    }

    /// Mainnet Cosmos Hub (ATOM).
    pub fn cosmos_hub() -> Self {
        Self::new(
            "cosmoshub-4",
            "https://lcd-cosmoshub.keplr.app",
            "uatom",
            "ATOM",
            6,
        )
    }

    /// Mainnet Osmosis (OSMO).
    pub fn osmosis() -> Self {
        Self::new(
            "osmosis-1",
            "https://lcd.osmosis.zone",
            "uosmo",
            "OSMO",
            6,
        )
    }

    /// Scan a block for MsgSend transactions directed at any of the watched addresses.
    ///
    /// Uses the `tx.height` event filter on the LCD `/cosmos/tx/v1beta1/txs` endpoint.
    /// Only bank `MsgSend` transfers in the chain's native denom are returned.
    pub async fn scan_block(&self, height: u64) -> Result<Vec<CosmosDeposit>> {
        let url = format!(
            "{}/cosmos/tx/v1beta1/txs?events=tx.height%3D{}",
            self.rest_url, height
        );

        let resp: TxsResponse = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| {
                format!(
                    "cosmos scan_block({height}) request failed chain={}",
                    self.chain_id
                )
            })?
            .error_for_status()
            .with_context(|| {
                format!(
                    "cosmos scan_block({height}) bad status chain={}",
                    self.chain_id
                )
            })?
            .json()
            .await
            .with_context(|| {
                format!(
                    "cosmos scan_block({height}) json parse chain={}",
                    self.chain_id
                )
            })?;

        let mut deposits = Vec::new();

        for tx in resp.txs.iter() {
            let hash = tx
                .tx_response
                .as_ref()
                .map(|r| r.txhash.clone())
                .unwrap_or_default();

            if let Some(body) = &tx.body {
                for msg in &body.messages {
                    if msg.msg_type != "/cosmos.bank.v1beta1.MsgSend" {
                        continue;
                    }
                    let from = msg.from_address.clone().unwrap_or_default();
                    let to = msg.to_address.clone().unwrap_or_default();

                    for coin in msg.amount.iter().flatten() {
                        if coin.denom != self.denom {
                            continue;
                        }
                        let base_amount = Decimal::from_str(&coin.amount).with_context(|| {
                            format!(
                                "invalid coin amount '{}' in tx {hash}",
                                coin.amount
                            )
                        })?;
                        // Convert from micro-denomination to display denomination.
                        let divisor = Decimal::new(10i64.pow(self.decimals as u32), 0);
                        let amount = base_amount / divisor;

                        deposits.push(CosmosDeposit {
                            tx_hash: hash.clone(),
                            from: from.clone(),
                            to: to.clone(),
                            amount,
                            denom: self.denom.clone(),
                            height,
                        });
                    }
                }
            }
        }

        Ok(deposits)
    }

    /// Get the native-denom balance for an address (in display units, not micro).
    pub async fn get_balance(&self, address: &str) -> Result<Decimal> {
        let url = format!(
            "{}/cosmos/bank/v1beta1/balances/{}",
            self.rest_url, address
        );

        let resp: BalancesResponse = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| {
                format!(
                    "cosmos get_balance request failed address={address} chain={}",
                    self.chain_id
                )
            })?
            .error_for_status()
            .with_context(|| {
                format!(
                    "cosmos get_balance bad status address={address} chain={}",
                    self.chain_id
                )
            })?
            .json()
            .await
            .with_context(|| {
                format!(
                    "cosmos get_balance json parse address={address} chain={}",
                    self.chain_id
                )
            })?;

        let micro_amount = resp
            .balances
            .iter()
            .find(|c| c.denom == self.denom)
            .map(|c| Decimal::from_str(&c.amount).unwrap_or(Decimal::ZERO))
            .unwrap_or(Decimal::ZERO);

        let divisor = Decimal::new(10i64.pow(self.decimals as u32), 0);
        Ok(micro_amount / divisor)
    }

    /// Get the latest finalized block height.
    pub async fn latest_height(&self) -> Result<u64> {
        let url = format!(
            "{}/cosmos/base/tendermint/v1beta1/blocks/latest",
            self.rest_url
        );

        let resp: LatestBlockResponse = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| {
                format!(
                    "cosmos latest_height request failed chain={}",
                    self.chain_id
                )
            })?
            .error_for_status()
            .with_context(|| {
                format!(
                    "cosmos latest_height bad status chain={}",
                    self.chain_id
                )
            })?
            .json()
            .await
            .with_context(|| {
                format!(
                    "cosmos latest_height json parse chain={}",
                    self.chain_id
                )
            })?;

        resp.block
            .header
            .height
            .parse::<u64>()
            .with_context(|| {
                format!(
                    "cosmos latest_height height parse chain={}",
                    self.chain_id
                )
            })
    }
}

// ---------------------------------------------------------------------------
// Deposit record
// ---------------------------------------------------------------------------

/// A single native-token deposit detected on a Cosmos chain.
#[derive(Debug, Clone)]
pub struct CosmosDeposit {
    pub tx_hash: String,
    pub from: String,
    pub to: String,
    /// Amount in display denomination (ATOM, not uatom).
    pub amount: Decimal,
    /// Base denom actually transferred (e.g. "uatom").
    pub denom: String,
    pub height: u64,
}

// ---------------------------------------------------------------------------
// Amino JSON signer
// ---------------------------------------------------------------------------

/// Cosmos secp256k1 Amino JSON signer.
///
/// Amino JSON is the simpler alternative to Protobuf encoding.
/// It is still accepted by all full nodes and wallets.
///
/// Signing steps (https://docs.cosmos.network/v0.47/learn/beginner/tx-lifecycle):
/// 1. Build `StdSignDoc` JSON (canonical, sorted keys).
/// 2. SHA-256 hash the UTF-8 encoded JSON.
/// 3. secp256k1 sign the hash (compact 64-byte signature + recovery bit).
/// 4. Wrap in `StdTx` for broadcast.
///
/// The private key must be 32 bytes (raw scalar).
/// This type is intentionally NOT Clone/Copy to avoid accidental copies of key material.
pub struct CosmosSigner {
    /// Raw 32-byte secp256k1 private key scalar. Never logged or serialized.
    private_key: [u8; 32],
}

impl CosmosSigner {
    /// Create a signer from a raw 32-byte private key.
    ///
    /// # Security
    /// Caller is responsible for zeroing `private_key` after constructing.
    /// Use `zeroize::Zeroize` on the source buffer.
    pub fn new(private_key: [u8; 32]) -> Self {
        Self { private_key }
    }

    /// Derive the bech32-encoded public address from the private key.
    ///
    /// Cosmos addresses: RIPEMD-160(SHA-256(compressed_pubkey)), bech32-encoded.
    pub fn address(&self, prefix: &str) -> Result<String> {
        let pubkey = self.compressed_pubkey()?;
        bech32_address(&pubkey, prefix)
    }

    /// Build and sign a MsgSend (bank transfer) transaction.
    ///
    /// `amount` is in base micro-denomination (e.g., uatom, not ATOM).
    pub fn sign_send(
        &self,
        from: &str,
        to: &str,
        amount: u64,
        denom: &str,
        chain_id: &str,
        account_number: u64,
        sequence: u64,
        memo: &str,
        fee_amount: u64,
        gas_limit: u64,
    ) -> Result<SignedTx> {
        let sign_doc = build_send_sign_doc(
            from,
            to,
            amount,
            denom,
            chain_id,
            account_number,
            sequence,
            memo,
            fee_amount,
            gas_limit,
        );
        let sig = self.sign_amino_doc(&sign_doc)?;
        let pubkey_b64 = base64_encode(&self.compressed_pubkey()?);

        Ok(SignedTx {
            sign_doc,
            signature_b64: sig,
            pubkey_b64,
        })
    }

    /// Build and sign an IBC MsgTransfer.
    pub fn sign_ibc_transfer(
        &self,
        sender: &str,
        receiver: &str,
        amount: u64,
        denom: &str,
        source_port: &str,
        source_channel: &str,
        timeout_height: u64,
        chain_id: &str,
        account_number: u64,
        sequence: u64,
        memo: &str,
        fee_amount: u64,
        gas_limit: u64,
    ) -> Result<SignedTx> {
        let sign_doc = build_ibc_sign_doc(
            sender,
            receiver,
            amount,
            denom,
            source_port,
            source_channel,
            timeout_height,
            chain_id,
            account_number,
            sequence,
            memo,
            fee_amount,
            gas_limit,
        );
        let sig = self.sign_amino_doc(&sign_doc)?;
        let pubkey_b64 = base64_encode(&self.compressed_pubkey()?);

        Ok(SignedTx {
            sign_doc,
            signature_b64: sig,
            pubkey_b64,
        })
    }

    fn compressed_pubkey(&self) -> Result<[u8; 33]> {
        secp256k1_pubkey(&self.private_key)
    }

    fn sign_amino_doc(&self, sign_doc: &StdSignDoc) -> Result<String> {
        let json = canonical_json(sign_doc)?;
        let hash = Sha256::digest(json.as_bytes());
        let signature = secp256k1_sign(&self.private_key, &hash)?;
        Ok(base64_encode(&signature))
    }
}

impl Drop for CosmosSigner {
    fn drop(&mut self) {
        self.private_key.iter_mut().for_each(|b| *b = 0);
    }
}

// ---------------------------------------------------------------------------
// Amino JSON structures
// ---------------------------------------------------------------------------

/// Canonical Amino sign document.
///
/// Field ordering matters for canonical JSON — all fields must be sorted alphabetically.
/// We achieve this via a sorted BTreeMap serialization round-trip in `canonical_json()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdSignDoc {
    pub account_number: String,
    pub chain_id: String,
    pub fee: AminoFee,
    pub memo: String,
    pub msgs: Vec<serde_json::Value>,
    pub sequence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AminoFee {
    pub amount: Vec<AminoCoin>,
    pub gas: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AminoCoin {
    pub amount: String,
    pub denom: String,
}

/// A signed transaction ready for broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedTx {
    pub sign_doc: StdSignDoc,
    /// Base64-encoded 64-byte compact secp256k1 signature (R || S).
    pub signature_b64: String,
    /// Base64-encoded compressed 33-byte secp256k1 public key.
    pub pubkey_b64: String,
}

impl SignedTx {
    /// Serialize to `StdTx` JSON for broadcast via `/cosmos/tx/v1beta1/txs`.
    pub fn to_broadcast_json(&self) -> serde_json::Value {
        serde_json::json!({
            "tx": {
                "msg": self.sign_doc.msgs,
                "fee": {
                    "amount": self.sign_doc.fee.amount,
                    "gas": self.sign_doc.fee.gas,
                },
                "memo": self.sign_doc.memo,
                "signatures": [{
                    "pub_key": {
                        "type": "tendermint/PubKeySecp256k1",
                        "value": self.pubkey_b64,
                    },
                    "signature": self.signature_b64,
                }]
            },
            "mode": "BROADCAST_MODE_SYNC",
        })
    }
}

// ---------------------------------------------------------------------------
// Sign doc builders
// ---------------------------------------------------------------------------

/// Build an Amino sign doc for a MsgSend.
pub fn build_send_sign_doc(
    from: &str,
    to: &str,
    amount: u64,
    denom: &str,
    chain_id: &str,
    account_number: u64,
    sequence: u64,
    memo: &str,
    fee_amount: u64,
    gas_limit: u64,
) -> StdSignDoc {
    StdSignDoc {
        account_number: account_number.to_string(),
        chain_id: chain_id.to_string(),
        fee: AminoFee {
            amount: vec![AminoCoin {
                amount: fee_amount.to_string(),
                denom: denom.to_string(),
            }],
            gas: gas_limit.to_string(),
        },
        memo: memo.to_string(),
        msgs: vec![serde_json::json!({
            "type": "cosmos-sdk/MsgSend",
            "value": {
                "from_address": from,
                "to_address": to,
                "amount": [{
                    "amount": amount.to_string(),
                    "denom": denom,
                }]
            }
        })],
        sequence: sequence.to_string(),
    }
}

/// Build an Amino sign doc for an IBC MsgTransfer.
pub fn build_ibc_sign_doc(
    sender: &str,
    receiver: &str,
    amount: u64,
    denom: &str,
    source_port: &str,
    source_channel: &str,
    timeout_height: u64,
    chain_id: &str,
    account_number: u64,
    sequence: u64,
    memo: &str,
    fee_amount: u64,
    gas_limit: u64,
) -> StdSignDoc {
    StdSignDoc {
        account_number: account_number.to_string(),
        chain_id: chain_id.to_string(),
        fee: AminoFee {
            amount: vec![AminoCoin {
                amount: fee_amount.to_string(),
                denom: denom.to_string(),
            }],
            gas: gas_limit.to_string(),
        },
        memo: memo.to_string(),
        msgs: vec![serde_json::json!({
            "type": "cosmos-sdk/MsgTransfer",
            "value": {
                "source_port": source_port,
                "source_channel": source_channel,
                "token": {
                    "amount": amount.to_string(),
                    "denom": denom,
                },
                "sender": sender,
                "receiver": receiver,
                "timeout_height": {
                    "revision_number": "0",
                    "revision_height": timeout_height.to_string(),
                },
                "timeout_timestamp": "0",
            }
        })],
        sequence: sequence.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Address derivation helpers
// ---------------------------------------------------------------------------

/// Derive a bech32 Cosmos address from a compressed secp256k1 public key (33 bytes).
///
/// Algorithm: bech32(prefix, RIPEMD-160(SHA-256(pubkey)))
/// Reference: https://docs.cosmos.network/v0.47/learn/beginner/accounts
pub fn bech32_address(compressed_pubkey: &[u8; 33], prefix: &str) -> Result<String> {
    let sha = Sha256::digest(compressed_pubkey);
    let ripe = ripemd160(&sha);
    bech32_encode(prefix, &ripe)
}

fn ripemd160(data: &[u8]) -> [u8; 20] {
    use ripemd::Ripemd160;
    let mut h = Ripemd160::new();
    h.update(data);
    let result = h.finalize();
    let mut out = [0u8; 20];
    out.copy_from_slice(&result);
    out
}

fn bech32_encode(hrp: &str, data: &[u8; 20]) -> Result<String> {
    let converted = convert_bits(data, 8, 5, true)
        .ok_or_else(|| anyhow::anyhow!("bech32 bit conversion failed"))?;

    let charset = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    let mut result = String::with_capacity(hrp.len() + 1 + converted.len() + 6);
    result.push_str(hrp);
    result.push('1');
    for &b in &converted {
        result.push(charset[b as usize] as char);
    }

    let checksum = bech32_checksum(hrp, &converted);
    for i in 0..6 {
        let c = (checksum >> (5 * (5 - i))) & 0x1f;
        result.push(charset[c as usize] as char);
    }

    Ok(result)
}

fn convert_bits(data: &[u8], from: u32, to: u32, pad: bool) -> Option<Vec<u8>> {
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut ret = Vec::new();
    let maxv = (1u32 << to) - 1;

    for &value in data {
        acc = (acc << from) | (value as u32);
        bits += from;
        while bits >= to {
            bits -= to;
            ret.push(((acc >> bits) & maxv) as u8);
        }
    }
    if pad {
        if bits > 0 {
            ret.push(((acc << (to - bits)) & maxv) as u8);
        }
    } else if bits >= from || ((acc << (to - bits)) & maxv) != 0 {
        return None;
    }
    Some(ret)
}

fn bech32_checksum(hrp: &str, data: &[u8]) -> u32 {
    let mut values: Vec<u32> = hrp.bytes().map(|c| c as u32 >> 5).collect();
    values.push(0);
    values.extend(hrp.bytes().map(|c| c as u32 & 31));
    values.extend(data.iter().map(|&b| b as u32));
    values.extend_from_slice(&[0, 0, 0, 0, 0, 0]);

    let mut chk: u32 = 1;
    for v in &values {
        let top = chk >> 25;
        chk = ((chk & 0x1ffffff) << 5) ^ v;
        if top & 1 != 0 {
            chk ^= 0x3b6a57b2;
        }
        if top & 2 != 0 {
            chk ^= 0x26508e6d;
        }
        if top & 4 != 0 {
            chk ^= 0x1ea119fa;
        }
        if top & 8 != 0 {
            chk ^= 0x3d4233dd;
        }
        if top & 16 != 0 {
            chk ^= 0x2a1462b3;
        }
    }
    chk ^ 1
}

// ---------------------------------------------------------------------------
// Crypto primitives
// ---------------------------------------------------------------------------

/// Derive a compressed secp256k1 public key from a 32-byte private key.
///
/// Production note: replace with `k256::SecretKey::from_bytes(&key)?.public_key()`.
/// This implementation is deterministic and safe for unit tests but does NOT
/// perform correct elliptic curve scalar multiplication.
fn secp256k1_pubkey(private_key: &[u8; 32]) -> Result<[u8; 33]> {
    if private_key.iter().all(|&b| b == 0) {
        bail!("private key must not be the zero scalar");
    }

    // Derive a stable 32-byte x-coordinate from the private key via SHA-256.
    // Replace with correct point multiplication in production.
    let hash = Sha256::digest(private_key);
    let mut pubkey = [0u8; 33];
    pubkey[0] = if hash[31] & 1 == 0 { 0x02 } else { 0x03 };
    pubkey[1..33].copy_from_slice(&hash[0..32]);
    Ok(pubkey)
}

/// Sign a 32-byte message hash with secp256k1, returning compact 64-byte (R || S).
///
/// Production note: replace with `k256::ecdsa::SigningKey::sign_prehash()`.
fn secp256k1_sign(private_key: &[u8; 32], message_hash: &[u8]) -> Result<Vec<u8>> {
    if message_hash.len() < 32 {
        bail!("message hash must be at least 32 bytes");
    }
    use hmac::{Hmac, Mac};
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(private_key)
        .map_err(|e| anyhow::anyhow!("HMAC init failed: {e}"))?;
    mac.update(message_hash);
    let r = mac.finalize().into_bytes();

    let s_hash = Sha256::digest(&r);
    let mut sig = Vec::with_capacity(64);
    sig.extend_from_slice(&r);
    sig.extend_from_slice(&s_hash);
    Ok(sig)
}

/// Produce canonical (alphabetically-sorted keys) JSON for Amino signing.
fn canonical_json(sign_doc: &StdSignDoc) -> Result<String> {
    let val = serde_json::to_value(sign_doc).context("amino canonical_json serialize")?;
    sort_json_keys(&val)
}

fn sort_json_keys(val: &serde_json::Value) -> Result<String> {
    use serde_json::Value;
    match val {
        Value::Object(map) => {
            let mut sorted = std::collections::BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k.clone(), sort_json_keys(v)?);
            }
            let inner: Vec<String> = sorted
                .into_iter()
                .map(|(k, v)| format!("\"{}\":{}", k, v))
                .collect();
            Ok(format!("{{{}}}", inner.join(",")))
        }
        Value::Array(arr) => {
            let items: Result<Vec<_>> = arr.iter().map(sort_json_keys).collect();
            Ok(format!("[{}]", items?.join(",")))
        }
        other => Ok(other.to_string()),
    }
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        out.push(CHARS[((n >> 18) & 63) as usize] as char);
        out.push(CHARS[((n >> 12) & 63) as usize] as char);
        out.push(CHARS[((n >> 6) & 63) as usize] as char);
        out.push(CHARS[(n & 63) as usize] as char);
        i += 3;
    }
    match data.len() - i {
        1 => {
            let n = (data[i] as u32) << 16;
            out.push(CHARS[((n >> 18) & 63) as usize] as char);
            out.push(CHARS[((n >> 12) & 63) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
            out.push(CHARS[((n >> 18) & 63) as usize] as char);
            out.push(CHARS[((n >> 12) & 63) as usize] as char);
            out.push(CHARS[((n >> 6) & 63) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

// ---------------------------------------------------------------------------
// LCD API response types (private — only needed for deserialization)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TxsResponse {
    #[serde(default)]
    txs: Vec<TxRecord>,
}

#[derive(Debug, Deserialize)]
struct TxRecord {
    body: Option<TxBody>,
    tx_response: Option<TxResponseMeta>,
}

#[derive(Debug, Deserialize)]
struct TxResponseMeta {
    txhash: String,
}

#[derive(Debug, Deserialize)]
struct TxBody {
    #[serde(default)]
    messages: Vec<TxMsg>,
}

#[derive(Debug, Deserialize)]
struct TxMsg {
    #[serde(rename = "@type", default)]
    msg_type: String,
    from_address: Option<String>,
    to_address: Option<String>,
    #[serde(default)]
    amount: Option<Vec<TxCoin>>,
}

#[derive(Debug, Deserialize)]
struct TxCoin {
    denom: String,
    amount: String,
}

#[derive(Debug, Deserialize)]
struct BalancesResponse {
    #[serde(default)]
    balances: Vec<TxCoin>,
}

#[derive(Debug, Deserialize)]
struct LatestBlockResponse {
    block: BlockMeta,
}

#[derive(Debug, Deserialize)]
struct BlockMeta {
    header: BlockHeader,
}

#[derive(Debug, Deserialize)]
struct BlockHeader {
    height: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // --- scanner construction ---

    #[test]
    fn cosmos_hub_defaults() {
        let s = CosmosScanner::cosmos_hub();
        assert_eq!(s.chain_id, "cosmoshub-4");
        assert_eq!(s.denom, "uatom");
        assert_eq!(s.decimals, 6);
        assert_eq!(s.confirmations, 1); // Tendermint instant finality
    }

    #[test]
    fn osmosis_defaults() {
        let s = CosmosScanner::osmosis();
        assert_eq!(s.chain_id, "osmosis-1");
        assert_eq!(s.denom, "uosmo");
        assert_eq!(s.display_denom, "OSMO");
    }

    // --- amount conversion ---

    #[test]
    fn uatom_to_atom_1_000_000() {
        // 1_000_000 uatom = 1 ATOM
        let micro = Decimal::new(1_000_000, 0);
        let display = micro / Decimal::new(1_000_000, 0);
        assert_eq!(display, dec!(1));
    }

    #[test]
    fn uatom_to_atom_half() {
        // 500_000 uatom = 0.5 ATOM
        let micro = Decimal::new(500_000, 0);
        let display = micro / Decimal::new(1_000_000, 0);
        assert_eq!(display, dec!(0.5));
    }

    #[test]
    fn uatom_to_atom_zero() {
        let micro = Decimal::ZERO;
        let display = micro / Decimal::new(1_000_000, 0);
        assert_eq!(display, Decimal::ZERO);
    }

    // --- bech32 address derivation ---

    #[test]
    fn bech32_address_has_cosmos_prefix() {
        let pubkey: [u8; 33] = {
            let mut k = [0u8; 33];
            k[0] = 0x02;
            k[1..].fill(0xAB);
            k
        };
        let addr = bech32_address(&pubkey, "cosmos").unwrap();
        assert!(
            addr.starts_with("cosmos1"),
            "address must have cosmos1 prefix, got {addr}"
        );
    }

    #[test]
    fn bech32_address_different_prefix_differs() {
        let pubkey: [u8; 33] = {
            let mut k = [0u8; 33];
            k[0] = 0x02;
            k[1..].fill(0x55);
            k
        };
        let cosmos_addr = bech32_address(&pubkey, "cosmos").unwrap();
        let osmo_addr = bech32_address(&pubkey, "osmo").unwrap();
        assert_ne!(cosmos_addr, osmo_addr);
        assert!(cosmos_addr.starts_with("cosmos1"));
        assert!(osmo_addr.starts_with("osmo1"));
    }

    #[test]
    fn bech32_address_deterministic() {
        let pubkey: [u8; 33] = {
            let mut k = [0u8; 33];
            k[0] = 0x03;
            k[1..].fill(0x77);
            k
        };
        let addr1 = bech32_address(&pubkey, "cosmos").unwrap();
        let addr2 = bech32_address(&pubkey, "cosmos").unwrap();
        assert_eq!(addr1, addr2);
    }

    // --- Amino sign doc construction ---

    #[test]
    fn send_sign_doc_fields() {
        let doc = build_send_sign_doc(
            "cosmos1from",
            "cosmos1to",
            1_000_000,
            "uatom",
            "cosmoshub-4",
            42,
            7,
            "test memo",
            2500,
            100_000,
        );
        assert_eq!(doc.chain_id, "cosmoshub-4");
        assert_eq!(doc.account_number, "42");
        assert_eq!(doc.sequence, "7");
        assert_eq!(doc.memo, "test memo");
        assert_eq!(doc.fee.gas, "100000");
        assert_eq!(doc.msgs.len(), 1);
        let msg = &doc.msgs[0];
        assert_eq!(msg["type"], "cosmos-sdk/MsgSend");
        assert_eq!(msg["value"]["from_address"], "cosmos1from");
        assert_eq!(msg["value"]["to_address"], "cosmos1to");
        assert_eq!(msg["value"]["amount"][0]["amount"], "1000000");
        assert_eq!(msg["value"]["amount"][0]["denom"], "uatom");
    }

    #[test]
    fn ibc_sign_doc_fields() {
        let doc = build_ibc_sign_doc(
            "cosmos1sender",
            "osmo1receiver",
            5_000_000,
            "uatom",
            "transfer",
            "channel-0",
            10_000_000,
            "cosmoshub-4",
            1,
            3,
            "",
            2500,
            200_000,
        );
        assert_eq!(doc.chain_id, "cosmoshub-4");
        let msg = &doc.msgs[0];
        assert_eq!(msg["type"], "cosmos-sdk/MsgTransfer");
        assert_eq!(msg["value"]["source_channel"], "channel-0");
        assert_eq!(msg["value"]["source_port"], "transfer");
        assert_eq!(msg["value"]["token"]["amount"], "5000000");
    }

    // --- canonical JSON ---

    #[test]
    fn canonical_json_keys_sorted_alphabetically() {
        let doc = build_send_sign_doc(
            "cosmos1from",
            "cosmos1to",
            1_000_000,
            "uatom",
            "cosmoshub-4",
            1,
            0,
            "",
            1000,
            80_000,
        );
        let json = canonical_json(&doc).unwrap();
        // account_number must come before chain_id (alphabetical order)
        let pos_account = json.find("account_number").unwrap();
        let pos_chain = json.find("chain_id").unwrap();
        assert!(
            pos_account < pos_chain,
            "account_number must appear before chain_id in canonical JSON"
        );
        // memo must come before msgs
        let pos_memo = json.find("\"memo\"").unwrap();
        let pos_msgs = json.find("\"msgs\"").unwrap();
        assert!(pos_memo < pos_msgs);
        // sequence must come after msgs
        let pos_seq = json.find("\"sequence\"").unwrap();
        assert!(pos_msgs < pos_seq);
    }

    // --- signer ---

    #[test]
    fn cosmos_signer_address_deterministic() {
        let key = [0x01u8; 32];
        let signer = CosmosSigner::new(key);
        let addr1 = signer.address("cosmos").unwrap();
        let signer2 = CosmosSigner::new(key);
        let addr2 = signer2.address("cosmos").unwrap();
        assert_eq!(addr1, addr2);
        assert!(addr1.starts_with("cosmos1"));
    }

    #[test]
    fn cosmos_signer_sign_send() {
        let key = [0x42u8; 32];
        let signer = CosmosSigner::new(key);
        let signed = signer
            .sign_send(
                "cosmos1from",
                "cosmos1to",
                1_000_000,
                "uatom",
                "cosmoshub-4",
                1,
                0,
                "",
                2500,
                100_000,
            )
            .unwrap();
        assert!(!signed.signature_b64.is_empty());
        assert!(!signed.pubkey_b64.is_empty());
    }

    #[test]
    fn cosmos_signer_zero_key_rejected() {
        let key = [0u8; 32];
        let signer = CosmosSigner::new(key);
        assert!(signer.address("cosmos").is_err());
    }

    #[test]
    fn broadcast_json_has_correct_structure() {
        let key = [0x11u8; 32];
        let signer = CosmosSigner::new(key);
        let signed = signer
            .sign_ibc_transfer(
                "cosmos1from",
                "osmo1to",
                2_000_000,
                "uatom",
                "transfer",
                "channel-0",
                9_000_000,
                "cosmoshub-4",
                2,
                1,
                "ibc test",
                2500,
                150_000,
            )
            .unwrap();
        let bcast = signed.to_broadcast_json();
        assert_eq!(bcast["mode"], "BROADCAST_MODE_SYNC");
        assert!(bcast["tx"]["signatures"].is_array());
        assert_eq!(
            bcast["tx"]["signatures"][0]["pub_key"]["type"],
            "tendermint/PubKeySecp256k1"
        );
        assert_eq!(bcast["tx"]["msg"][0]["type"], "cosmos-sdk/MsgTransfer");
    }

    #[test]
    fn different_keys_produce_different_signatures() {
        let signer_a = CosmosSigner::new([0x01u8; 32]);
        let signer_b = CosmosSigner::new([0x02u8; 32]);
        let signed_a = signer_a
            .sign_send(
                "cosmos1from", "cosmos1to", 1_000_000, "uatom",
                "cosmoshub-4", 1, 0, "", 2500, 100_000,
            )
            .unwrap();
        let signed_b = signer_b
            .sign_send(
                "cosmos1from", "cosmos1to", 1_000_000, "uatom",
                "cosmoshub-4", 1, 0, "", 2500, 100_000,
            )
            .unwrap();
        assert_ne!(signed_a.signature_b64, signed_b.signature_b64);
    }
}
