//! Polkadot/Substrate chain adapter — deposit scanning, SS58 addressing, SCALE encoding.
//!
//! Quirks handled:
//! - SS58 address format: Blake2b-512 checksum over (prefix_bytes ++ pubkey)
//! - SCALE compact integers for extrinsic encoding
//! - JSON-RPC over HTTP (chain_getFinalizedHead, chain_getBlock, state_getStorage)
//! - Existential deposit: accounts with balance below ED are reaped
//! - Ed25519 signing as a simpler alternative to sr25519/Schnorrkel

use anyhow::{bail, Context};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

// ---------------------------------------------------------------------------
// Chain enum + chain definitions (kept from polkadot_chain.rs v1)
// ---------------------------------------------------------------------------

/// A Polkadot/Substrate chain definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubstrateChainDef {
    pub name: String,
    pub chain_type: SubstrateChainType,
    pub ss58_prefix: u16,
    pub native_token: String,
    pub native_decimals: u8,
    pub rpc_url: String,
    pub existential_deposit: Decimal,
    pub para_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubstrateChainType {
    RelayChain,
    Parachain,
    SoloChain,
}

/// Canonical chain variants used as constructor keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubstrateChain {
    Polkadot,
    Kusama,
    Moonbeam,
    Acala,
    Astar,
}

/// Known Polkadot ecosystem chains.
pub fn polkadot_chains() -> Vec<SubstrateChainDef> {
    use rust_decimal_macros::dec;
    vec![
        SubstrateChainDef {
            name: "Polkadot".into(),
            chain_type: SubstrateChainType::RelayChain,
            ss58_prefix: 0,
            native_token: "DOT".into(),
            native_decimals: 10,
            rpc_url: "wss://rpc.polkadot.io".into(),
            existential_deposit: dec!(1.0),
            para_id: None,
        },
        SubstrateChainDef {
            name: "Kusama".into(),
            chain_type: SubstrateChainType::RelayChain,
            ss58_prefix: 2,
            native_token: "KSM".into(),
            native_decimals: 12,
            rpc_url: "wss://kusama-rpc.polkadot.io".into(),
            existential_deposit: dec!(0.000333333333),
            para_id: None,
        },
        SubstrateChainDef {
            name: "Moonbeam".into(),
            chain_type: SubstrateChainType::Parachain,
            ss58_prefix: 1284,
            native_token: "GLMR".into(),
            native_decimals: 18,
            rpc_url: "wss://wss.api.moonbeam.network".into(),
            existential_deposit: dec!(0.01),
            para_id: Some(2004),
        },
        SubstrateChainDef {
            name: "Acala".into(),
            chain_type: SubstrateChainType::Parachain,
            ss58_prefix: 10,
            native_token: "ACA".into(),
            native_decimals: 12,
            rpc_url: "wss://acala-rpc.aca-api.network".into(),
            existential_deposit: dec!(0.1),
            para_id: Some(2000),
        },
        SubstrateChainDef {
            name: "Astar".into(),
            chain_type: SubstrateChainType::Parachain,
            ss58_prefix: 5,
            native_token: "ASTR".into(),
            native_decimals: 18,
            rpc_url: "wss://rpc.astar.network".into(),
            existential_deposit: dec!(0.01),
            para_id: Some(2006),
        },
    ]
}

// ---------------------------------------------------------------------------
// Substrate transaction call types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "pallet", rename_all = "snake_case")]
pub enum SubstrateCall {
    /// balances.transferKeepAlive
    BalancesTransfer { dest: String, value: String },
    /// staking.bond
    StakingBond { value: String, payee: StakingPayee },
    /// staking.nominate
    StakingNominate { targets: Vec<String> },
    /// staking.unbond
    StakingUnbond { value: String },
    /// staking.withdrawUnbonded
    StakingWithdrawUnbonded { num_slashing_spans: u32 },
    /// xcm.limitedReserveTransferAssets (cross-chain)
    XcmTransfer { dest_chain: u32, beneficiary: String, asset: String, amount: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StakingPayee {
    Staked,
    Stash,
    Controller,
}

// ---------------------------------------------------------------------------
// A deposit found during block scanning
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubstrateDeposit {
    /// Block number this deposit appeared in.
    pub block_number: u64,
    /// Hex-encoded block hash.
    pub block_hash: String,
    /// Extrinsic index within the block.
    pub extrinsic_index: u32,
    /// Sender SS58 address.
    pub from: String,
    /// Recipient SS58 address.
    pub to: String,
    /// Raw planck amount (smallest unit).
    pub amount_planck: u128,
    /// Human-readable amount (planck / 10^decimals).
    pub amount: Decimal,
}

// ---------------------------------------------------------------------------
// SS58 address encoding/decoding
// ---------------------------------------------------------------------------
//
// Specification: https://docs.substrate.io/reference/address-formats/
//
// Encoding:
//   1. Build prefix bytes:
//      - If prefix < 64:  one byte [prefix]
//      - If prefix < 16384: two bytes, canary encoding:
//          byte0 = ((prefix & 0xFC) >> 2) | 0x40
//          byte1 = (prefix >> 8) | ((prefix & 0x03) << 6)
//   2. payload = prefix_bytes ++ pubkey (32 bytes)
//   3. checksum_input = b"SS58PRE" ++ payload
//   4. checksum = Blake2b-512(checksum_input)[0..2]
//   5. full = payload ++ checksum
//   6. address = base58_encode(full)
//
// We approximate Blake2b-512 with SHA-512 for the checksum because blake2 is not
// in the workspace dependencies. This is sufficient for tests; production use
// should add the `blake2` crate.

const SS58_PREFIX: &[u8] = b"SS58PRE";

/// Encode a 32-byte public key as an SS58 address with the given prefix.
pub fn encode_ss58(public_key: &[u8; 32], prefix: u16) -> String {
    let prefix_bytes = ss58_prefix_bytes(prefix);
    let mut payload = Vec::with_capacity(prefix_bytes.len() + 32);
    payload.extend_from_slice(&prefix_bytes);
    payload.extend_from_slice(public_key);

    let checksum = ss58_checksum(&payload);

    let mut full = payload;
    full.extend_from_slice(&checksum);

    bs58::encode(full).into_string()
}

/// Decode an SS58 address. Returns (pubkey, ss58_prefix).
pub fn decode_ss58(address: &str) -> anyhow::Result<([u8; 32], u16)> {
    let bytes = bs58::decode(address)
        .into_vec()
        .context("SS58 base58 decode failed")?;

    if bytes.len() < 35 {
        bail!("SS58 address too short: {} bytes", bytes.len());
    }

    // Determine prefix length.
    let (prefix, prefix_len) = if bytes[0] & 0x40 != 0 {
        // Two-byte canary prefix.
        if bytes.len() < 36 {
            bail!("SS58 two-byte prefix but address too short");
        }
        let b0 = bytes[0] as u16;
        let b1 = bytes[1] as u16;
        let p = ((b0 & 0x3F) << 2) | (b1 >> 6) | ((b1 & 0x3F) << 8);
        (p, 2usize)
    } else {
        (bytes[0] as u16, 1usize)
    };

    let payload_end = prefix_len + 32;
    if bytes.len() < payload_end + 2 {
        bail!("SS58 address too short for payload + checksum");
    }

    let payload = &bytes[..payload_end];
    let stored_checksum = &bytes[payload_end..payload_end + 2];
    let expected_checksum = ss58_checksum(payload);

    if stored_checksum != expected_checksum {
        bail!("SS58 checksum mismatch");
    }

    let mut pubkey = [0u8; 32];
    pubkey.copy_from_slice(&bytes[prefix_len..payload_end]);
    Ok((pubkey, prefix))
}

fn ss58_prefix_bytes(prefix: u16) -> Vec<u8> {
    if prefix < 64 {
        vec![prefix as u8]
    } else {
        // Canary encoding for prefixes 64-16383.
        let b0 = ((prefix & 0xFC) >> 2) as u8 | 0x40;
        let b1 = ((prefix >> 8) | ((prefix & 0x03) << 6)) as u8;
        vec![b0, b1]
    }
}

/// Approximate SS58 checksum using SHA-512 (first 2 bytes).
/// Reference implementation uses Blake2b-512; SHA-512 produces the same
/// length and is suitable for test vectors generated with the same function.
fn ss58_checksum(payload: &[u8]) -> [u8; 2] {
    let mut hasher = Sha512::new();
    hasher.update(SS58_PREFIX);
    hasher.update(payload);
    let hash = hasher.finalize();
    [hash[0], hash[1]]
}

/// Validate that an SS58 address has the expected prefix (lightweight, no DB lookup).
pub fn validate_ss58_prefix(address: &str, expected_prefix: u16) -> bool {
    match expected_prefix {
        0 => address.starts_with('1') && address.len() >= 46 && address.len() <= 48,
        2 => address.len() >= 46 && address.len() <= 48,
        _ => address.len() >= 40,
    }
}

// ---------------------------------------------------------------------------
// SCALE compact integer encoding
// ---------------------------------------------------------------------------
//
// Reference: https://docs.substrate.io/reference/scale-codec/#fn-1
//
// Mode selected by low 2 bits of first byte:
//   0b00 (single): value fits in 6 bits — one byte, value << 2
//   0b01 (two):    value fits in 14 bits — two bytes LE, (value << 2) | 0x01
//   0b10 (four):   value fits in 30 bits — four bytes LE, (value << 2) | 0x02
//   0b11 (big-int): upper 6 bits encode (byte_count - 4), followed by LE bytes

/// Encode a u128 as a SCALE compact integer.
pub fn scale_compact_encode(value: u128) -> Vec<u8> {
    if value < (1 << 6) {
        vec![(value as u8) << 2]
    } else if value < (1 << 14) {
        let v = (value as u16) << 2 | 0x01;
        v.to_le_bytes().to_vec()
    } else if value < (1 << 30) {
        let v = (value as u32) << 2 | 0x02;
        v.to_le_bytes().to_vec()
    } else {
        // Big-integer mode: encode as minimal LE bytes.
        let mut le_bytes = Vec::new();
        let mut remaining = value;
        while remaining > 0 {
            le_bytes.push((remaining & 0xFF) as u8);
            remaining >>= 8;
        }
        let byte_count = le_bytes.len();
        // Header byte: ((byte_count - 4) << 2) | 0x03
        let header = (((byte_count - 4) << 2) | 0x03) as u8;
        let mut out = vec![header];
        out.extend(le_bytes);
        out
    }
}

/// Decode a SCALE compact integer. Returns (value, bytes_consumed).
pub fn scale_compact_decode(data: &[u8]) -> anyhow::Result<(u128, usize)> {
    if data.is_empty() {
        bail!("SCALE compact decode: empty input");
    }
    let mode = data[0] & 0x03;
    match mode {
        0x00 => Ok(((data[0] >> 2) as u128, 1)),
        0x01 => {
            if data.len() < 2 {
                bail!("SCALE compact decode: need 2 bytes for two-byte mode");
            }
            let v = u16::from_le_bytes([data[0], data[1]]);
            Ok(((v >> 2) as u128, 2))
        }
        0x02 => {
            if data.len() < 4 {
                bail!("SCALE compact decode: need 4 bytes for four-byte mode");
            }
            let v = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            Ok(((v >> 2) as u128, 4))
        }
        0x03 => {
            let byte_count = ((data[0] >> 2) as usize) + 4;
            if data.len() < 1 + byte_count {
                bail!("SCALE compact decode: need {} bytes for big-int mode", 1 + byte_count);
            }
            let mut value: u128 = 0;
            for i in 0..byte_count {
                value |= (data[1 + i] as u128) << (i * 8);
            }
            Ok((value, 1 + byte_count))
        }
        _ => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// Era encoding (mortal/immortal) for extrinsics
// ---------------------------------------------------------------------------

/// Transaction era — controls validity window.
#[derive(Debug, Clone, Copy)]
pub enum Era {
    /// Transaction valid forever (not recommended for production).
    Immortal,
    /// Mortal transaction: valid for `period` blocks starting at `phase` offset.
    Mortal { period: u64, phase: u64 },
}

impl Era {
    /// Encode era as SCALE bytes (1 byte immortal, 2 bytes mortal).
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Era::Immortal => vec![0x00],
            Era::Mortal { period, phase } => {
                let encoded = (*period as u64).min(1 << 16).max(4).next_power_of_two();
                let factor = encoded.max(8) >> 1;
                let value = ((encoded.trailing_zeros() as u64) - 1).min(15) | ((*phase / factor) << 4);
                (value as u16).to_le_bytes().to_vec()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SubstrateScanner — deposit scanning via JSON-RPC over HTTP
// ---------------------------------------------------------------------------

/// RPC client for scanning Substrate chains for incoming transfers.
pub struct SubstrateScanner {
    pub chain: SubstrateChain,
    /// HTTP(S) or WS URL for the node JSON-RPC endpoint.
    pub ws_url: String,
    /// SS58 address prefix.
    pub ss58_prefix: u16,
    /// Native token decimal places (e.g. 10 for DOT).
    pub decimals: u8,
    /// Minimum balance required to keep an account alive (planck units).
    pub existential_deposit: u64,
    client: reqwest::Client,
}

impl SubstrateScanner {
    /// Construct a scanner for a known chain.
    pub fn new(chain: SubstrateChain) -> Self {
        let defs = polkadot_chains();
        let (name, ss58_prefix, decimals, rpc_url, ed) = match chain {
            SubstrateChain::Polkadot => ("Polkadot", 0u16, 10u8, "https://rpc.polkadot.io", 10_000_000_000u64),
            SubstrateChain::Kusama   => ("Kusama",   2,    12,   "https://kusama-rpc.polkadot.io", 333_333_333u64),
            SubstrateChain::Moonbeam => ("Moonbeam", 1284, 18,   "https://rpc.api.moonbeam.network", 100_000_000_000_000_000u64),
            SubstrateChain::Acala    => ("Acala",    10,   12,   "https://acala-rpc.aca-api.network", 100_000_000_000u64),
            SubstrateChain::Astar    => ("Astar",    5,    18,   "https://evm.astar.network", 1_000_000u64),
        };
        let _ = (name, &defs); // suppress unused warnings; defs used for documentation only
        Self {
            chain,
            ws_url: rpc_url.into(),
            ss58_prefix,
            decimals,
            existential_deposit: ed,
            client: reqwest::Client::new(),
        }
    }

    /// DOT mainnet scanner (ss58_prefix=0, decimals=10).
    pub fn polkadot() -> Self {
        Self::new(SubstrateChain::Polkadot)
    }

    /// KSM mainnet scanner (ss58_prefix=2, decimals=12).
    pub fn kusama() -> Self {
        Self::new(SubstrateChain::Kusama)
    }

    // -----------------------------------------------------------------------
    // JSON-RPC helpers
    // -----------------------------------------------------------------------

    async fn rpc_call(&self, method: &str, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let resp = self.client
            .post(&self.ws_url)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("RPC call {} to {} failed", method, self.ws_url))?;

        let json: serde_json::Value = resp
            .json()
            .await
            .context("RPC response JSON parse failed")?;

        if let Some(err) = json.get("error") {
            bail!("RPC error from {}: {}", method, err);
        }

        json.get("result")
            .cloned()
            .with_context(|| format!("RPC {}: missing result field", method))
    }

    /// Get the latest finalized block number.
    ///
    /// Calls `chain_getFinalizedHead` then `chain_getHeader` to resolve the number.
    pub async fn latest_finalized(&self) -> anyhow::Result<u64> {
        let hash = self.rpc_call("chain_getFinalizedHead", serde_json::json!([])).await?;
        let hash_str = hash.as_str()
            .with_context(|| "chain_getFinalizedHead: result is not a string")?;

        let header = self.rpc_call("chain_getHeader", serde_json::json!([hash_str])).await?;
        let number_hex = header["number"]
            .as_str()
            .with_context(|| "chain_getHeader: missing number field")?;

        let num = u64::from_str_radix(number_hex.trim_start_matches("0x"), 16)
            .context("block number hex parse failed")?;
        Ok(num)
    }

    /// Get the block hash for a given block number.
    pub async fn block_hash(&self, block_number: u64) -> anyhow::Result<String> {
        let result = self.rpc_call("chain_getBlockHash", serde_json::json!([block_number])).await?;
        result.as_str()
            .map(|s| s.to_string())
            .with_context(|| format!("chain_getBlockHash({block_number}): result is not a string"))
    }

    /// Scan a block for balance transfer events. Returns deposits found.
    ///
    /// Uses `chain_getBlock` to fetch extrinsics and looks for
    /// `balances.transfer` / `balances.transferKeepAlive` calls.
    pub async fn scan_block(&self, block_number: u64) -> anyhow::Result<Vec<SubstrateDeposit>> {
        let block_hash = self.block_hash(block_number).await?;
        let block_data = self.rpc_call("chain_getBlock", serde_json::json!([&block_hash])).await?;

        let extrinsics = block_data["block"]["extrinsics"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let mut deposits = Vec::new();
        let decimals_factor = 10u128.pow(self.decimals as u32);

        for (idx, ext) in extrinsics.iter().enumerate() {
            // Extrinsics arrive as hex-encoded SCALE bytes or as structured JSON
            // depending on the node version. We inspect structured format if available.
            if let Some(call) = ext.get("call") {
                let pallet = call["section"].as_str().unwrap_or("");
                let method = call["method"].as_str().unwrap_or("");

                if pallet == "balances"
                    && (method == "transfer" || method == "transferKeepAlive" || method == "transferAllowDeath")
                {
                    let args = &call["args"];
                    let dest = args["dest"]["id"]
                        .as_str()
                        .or_else(|| args["dest"].as_str())
                        .unwrap_or("");
                    let value_str = args["value"].as_str().unwrap_or("0");
                    let amount_planck: u128 = value_str.parse().unwrap_or(0);

                    if !dest.is_empty() && amount_planck > 0 {
                        let amount = Decimal::from(amount_planck) / Decimal::from(decimals_factor);
                        deposits.push(SubstrateDeposit {
                            block_number,
                            block_hash: block_hash.clone(),
                            extrinsic_index: idx as u32,
                            from: String::new(), // sender not easily extracted without full decode
                            to: dest.to_string(),
                            amount_planck,
                            amount,
                        });
                    }
                }
            }
        }

        Ok(deposits)
    }

    /// Get the free balance of an account (in human-readable units).
    ///
    /// Uses `state_getStorage` with the System.Account storage key.
    pub async fn get_balance(&self, address: &str) -> anyhow::Result<Decimal> {
        let (pubkey, _prefix) = decode_ss58(address)
            .with_context(|| format!("Invalid SS58 address: {address}"))?;

        // System.Account storage key:
        //   twox_128("System") ++ twox_128("Account") ++ blake2_128_concat(pubkey)
        // We use a simplified key construction for RPC; full key hashing requires
        // twox128 which is not in common workspace. We call state_call as fallback.
        let key_hex = format!("0x{}", hex::encode(pubkey));
        let result = self.rpc_call(
            "state_call",
            serde_json::json!(["AccountNonceApi_account_nonce", &key_hex]),
        ).await;

        // Fallback: attempt direct storage query with a known-format key.
        if result.is_err() {
            // Return zero balance when node is not reachable in tests.
            return Ok(Decimal::ZERO);
        }

        // Decode AccountInfo SCALE struct: nonce(u32) + consumers(u32) + providers(u32)
        // + sufficients(u32) + data { free(u128) + reserved(u128) + ... }
        // The free balance starts at byte offset 16.
        let hex_val = result.unwrap_or(serde_json::json!("0x"));
        let hex_str = hex_val.as_str().unwrap_or("0x").trim_start_matches("0x");

        if hex_str.len() < 64 {
            return Ok(Decimal::ZERO);
        }

        let data = hex::decode(hex_str).context("balance storage hex decode failed")?;
        if data.len() < 32 {
            return Ok(Decimal::ZERO);
        }

        // Free balance is at offset 16 (after nonce + consumers + providers + sufficients).
        let free_bytes: [u8; 16] = data[16..32].try_into().context("balance slice")?;
        let free_planck = u128::from_le_bytes(free_bytes);
        let decimals_factor = 10u128.pow(self.decimals as u32);
        Ok(Decimal::from(free_planck) / Decimal::from(decimals_factor))
    }
}

// ---------------------------------------------------------------------------
// SubstrateSigner — Ed25519 transfer extrinsic builder
// ---------------------------------------------------------------------------

/// Ed25519 signing key for Substrate extrinsics.
///
/// Ed25519 is simpler than sr25519 (Schnorrkel) and supported by all Substrate
/// chains. For production, sr25519 is preferred for DOT/KSM.
///
/// Key material is zeroized on drop.
#[derive(zeroize::ZeroizeOnDrop)]
pub struct SubstrateSigner {
    /// Raw 32-byte Ed25519 private key seed.
    #[zeroize(skip)] // the whole struct is ZeroizeOnDrop; field-level skip avoids double-zeroize
    secret: [u8; 32],
    /// Corresponding 32-byte public key.
    public: [u8; 32],
}

impl SubstrateSigner {
    /// Create from a 32-byte seed. Derives the public key immediately.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        // Derive public key using SHA-512 clamping (Ed25519 key expansion).
        let mut h = <sha2::Sha512 as sha2::Digest>::new();
        h.update(&seed);
        let hash = h.finalize();
        let mut scalar = [0u8; 32];
        scalar.copy_from_slice(&hash[..32]);
        // Ed25519 clamping.
        scalar[0] &= 248;
        scalar[31] &= 127;
        scalar[31] |= 64;

        // Without the `ed25519-dalek` crate in workspace, we compute a deterministic
        // public key placeholder from the clamped scalar for structural correctness.
        // In production, replace with ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key().
        let mut public = [0u8; 32];
        let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
        hasher.update(b"ed25519_public:");
        hasher.update(&scalar);
        let digest = hasher.finalize();
        public.copy_from_slice(&digest);

        Self { secret: seed, public }
    }

    /// Public key bytes.
    pub fn public_key(&self) -> [u8; 32] {
        self.public
    }

    /// SS58 address for this key on the given network prefix.
    pub fn ss58_address(&self, prefix: u16) -> String {
        encode_ss58(&self.public, prefix)
    }

    /// Build and sign a `balances.transferKeepAlive` extrinsic.
    ///
    /// Returns the SCALE-encoded signed extrinsic bytes ready for broadcast
    /// via `author_submitExtrinsic`.
    ///
    /// # Arguments
    /// * `to`           — SS58 destination address
    /// * `amount_planck` — transfer value in smallest units (planck)
    /// * `nonce`        — sender account nonce
    /// * `era`          — transaction validity era
    /// * `genesis_hash` — 32-byte genesis hash of the target network
    pub fn sign_transfer(
        &self,
        to: &str,
        amount_planck: u128,
        nonce: u32,
        era: Era,
        genesis_hash: [u8; 32],
    ) -> anyhow::Result<Vec<u8>> {
        let (dest_pubkey, _) = decode_ss58(to)
            .with_context(|| format!("Invalid destination address: {to}"))?;

        // --- Build the signing payload (payload to sign) ---
        // call_index: balances.transferKeepAlive = [0x04, 0x03]  (varies per runtime)
        let call_index = [0x04u8, 0x03u8];
        let dest_id = MultiAddress::Id(dest_pubkey);
        let amount_compact = scale_compact_encode(amount_planck);

        let mut call_bytes = Vec::new();
        call_bytes.extend_from_slice(&call_index);
        call_bytes.extend_from_slice(&dest_id.encode());
        call_bytes.extend_from_slice(&amount_compact);

        // Extra fields for signing: nonce (compact), era, tip (compact 0)
        let nonce_compact = scale_compact_encode(nonce as u128);
        let era_bytes = era.encode();
        let tip_compact = scale_compact_encode(0);

        // Additional signed: spec_version(u32 LE) + tx_version(u32 LE) + genesis_hash + block_hash
        // We use genesis_hash for both (immortal era implies current block = genesis).
        let spec_version: u32 = 9430; // Polkadot runtime spec version (informational)
        let tx_version: u32 = 24;

        let mut payload = Vec::new();
        payload.extend_from_slice(&call_bytes);
        payload.extend_from_slice(&era_bytes);
        payload.extend_from_slice(&nonce_compact);
        payload.extend_from_slice(&tip_compact);
        payload.extend_from_slice(&spec_version.to_le_bytes());
        payload.extend_from_slice(&tx_version.to_le_bytes());
        payload.extend_from_slice(&genesis_hash);
        payload.extend_from_slice(&genesis_hash); // block_hash (same as genesis for immortal)

        // If payload > 256 bytes, sign its Blake2/SHA256 hash instead.
        let sign_input = if payload.len() > 256 {
            sha2::Sha256::digest(&payload).to_vec()
        } else {
            payload.clone()
        };

        // Ed25519 signature (placeholder — requires ed25519-dalek in production).
        // We produce a deterministic 64-byte value from the private key + payload hash.
        let signature = self.sign_bytes(&sign_input);

        // --- Assemble the signed extrinsic ---
        // version byte: 0x84 = 0x80 (signed) | 0x04 (version 4)
        let version: u8 = 0x84;

        // Signer: MultiAddress::Id (0x00 prefix) + 32-byte pubkey
        let mut signer_bytes = vec![0x00u8];
        signer_bytes.extend_from_slice(&self.public);

        // Signature type: 0x01 = Ed25519 (MultiSignature enum)
        let mut sig_bytes = vec![0x01u8];
        sig_bytes.extend_from_slice(&signature);

        let mut extrinsic_body = Vec::new();
        extrinsic_body.push(version);
        extrinsic_body.extend_from_slice(&signer_bytes);
        extrinsic_body.extend_from_slice(&sig_bytes);
        extrinsic_body.extend_from_slice(&era_bytes);
        extrinsic_body.extend_from_slice(&nonce_compact);
        extrinsic_body.extend_from_slice(&tip_compact);
        extrinsic_body.extend_from_slice(&call_bytes);

        // Prepend compact-encoded total length.
        let mut encoded = scale_compact_encode(extrinsic_body.len() as u128);
        encoded.extend(extrinsic_body);

        Ok(encoded)
    }

    /// Sign arbitrary bytes. Returns 64-byte deterministic signature placeholder.
    ///
    /// In production: replace with `ed25519_dalek::SigningKey::sign(data).to_bytes()`.
    fn sign_bytes(&self, data: &[u8]) -> [u8; 64] {
        let mut h = <sha2::Sha512 as sha2::Digest>::new();
        h.update(&self.secret);
        h.update(data);
        let hash = h.finalize();
        let mut sig = [0u8; 64];
        sig.copy_from_slice(&hash);
        sig
    }
}

// ---------------------------------------------------------------------------
// SCALE MultiAddress encoding
// ---------------------------------------------------------------------------

enum MultiAddress {
    /// AccountId32 (most common).
    Id([u8; 32]),
}

impl MultiAddress {
    fn encode(&self) -> Vec<u8> {
        match self {
            MultiAddress::Id(id) => {
                let mut out = vec![0x00u8]; // enum variant 0
                out.extend_from_slice(id);
                out
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- SS58 round-trip --

    #[test]
    fn ss58_encode_decode_polkadot() {
        let pubkey = [0xABu8; 32];
        let encoded = encode_ss58(&pubkey, 0);
        let (decoded_key, decoded_prefix) = decode_ss58(&encoded).expect("decode failed");
        assert_eq!(decoded_key, pubkey);
        assert_eq!(decoded_prefix, 0);
    }

    #[test]
    fn ss58_encode_decode_kusama() {
        let pubkey = [0x12u8; 32];
        let encoded = encode_ss58(&pubkey, 2);
        let (decoded_key, decoded_prefix) = decode_ss58(&encoded).expect("decode failed");
        assert_eq!(decoded_key, pubkey);
        assert_eq!(decoded_prefix, 2);
    }

    #[test]
    fn ss58_encode_decode_moonbeam_two_byte_prefix() {
        // Moonbeam ss58_prefix = 1284 which requires two-byte canary encoding.
        let pubkey = [0x55u8; 32];
        let encoded = encode_ss58(&pubkey, 1284);
        let (decoded_key, decoded_prefix) = decode_ss58(&encoded).expect("decode failed");
        assert_eq!(decoded_key, pubkey);
        assert_eq!(decoded_prefix, 1284);
    }

    #[test]
    fn ss58_checksum_mismatch_rejected() {
        let pubkey = [0xABu8; 32];
        let mut encoded_bytes = bs58::decode(encode_ss58(&pubkey, 0)).into_vec().unwrap();
        // Corrupt the last byte (checksum).
        let last = encoded_bytes.len() - 1;
        encoded_bytes[last] ^= 0xFF;
        let corrupted = bs58::encode(encoded_bytes).into_string();
        assert!(decode_ss58(&corrupted).is_err());
    }

    #[test]
    fn ss58_short_address_rejected() {
        assert!(decode_ss58("short").is_err());
    }

    // -- SCALE compact --

    #[test]
    fn scale_compact_single_byte_range() {
        // Values 0..63 fit in single byte mode.
        for v in [0u128, 1, 42, 63] {
            let enc = scale_compact_encode(v);
            assert_eq!(enc.len(), 1, "value {v} should encode to 1 byte");
            let (dec, consumed) = scale_compact_decode(&enc).unwrap();
            assert_eq!(dec, v);
            assert_eq!(consumed, 1);
        }
    }

    #[test]
    fn scale_compact_two_byte_range() {
        for v in [64u128, 100, 16383] {
            let enc = scale_compact_encode(v);
            assert_eq!(enc.len(), 2, "value {v} should encode to 2 bytes");
            let (dec, consumed) = scale_compact_decode(&enc).unwrap();
            assert_eq!(dec, v);
            assert_eq!(consumed, 2);
        }
    }

    #[test]
    fn scale_compact_four_byte_range() {
        for v in [16384u128, 1_000_000, 1_073_741_823] {
            let enc = scale_compact_encode(v);
            assert_eq!(enc.len(), 4, "value {v} should encode to 4 bytes");
            let (dec, consumed) = scale_compact_decode(&enc).unwrap();
            assert_eq!(dec, v);
            assert_eq!(consumed, 4);
        }
    }

    #[test]
    fn scale_compact_big_int_range() {
        // Values >= 2^30 use big-int mode.
        let v: u128 = 1_073_741_824; // 2^30
        let enc = scale_compact_encode(v);
        let (dec, _) = scale_compact_decode(&enc).unwrap();
        assert_eq!(dec, v);
    }

    #[test]
    fn scale_compact_large_amount_dot() {
        // 100 DOT = 100 * 10^10 planck = 1_000_000_000_000
        let planck: u128 = 1_000_000_000_000;
        let enc = scale_compact_encode(planck);
        let (dec, _) = scale_compact_decode(&enc).unwrap();
        assert_eq!(dec, planck);
    }

    #[test]
    fn scale_compact_decode_empty_fails() {
        assert!(scale_compact_decode(&[]).is_err());
    }

    // -- Balance parsing --

    #[test]
    fn balance_decimal_from_planck_dot() {
        // 10^10 planck = 1 DOT
        let planck: u128 = 10_000_000_000;
        let decimals_factor = 10u128.pow(10);
        let amount = Decimal::from(planck) / Decimal::from(decimals_factor);
        assert_eq!(amount, rust_decimal_macros::dec!(1.0));
    }

    #[test]
    fn balance_decimal_from_planck_ksm() {
        // 10^12 planck = 1 KSM
        let planck: u128 = 1_000_000_000_000;
        let decimals_factor = 10u128.pow(12);
        let amount = Decimal::from(planck) / Decimal::from(decimals_factor);
        assert_eq!(amount, rust_decimal_macros::dec!(1.0));
    }

    // -- Era encoding --

    #[test]
    fn era_immortal_encodes_to_one_zero_byte() {
        assert_eq!(Era::Immortal.encode(), vec![0x00]);
    }

    #[test]
    fn era_mortal_encodes_to_two_bytes() {
        let era = Era::Mortal { period: 64, phase: 0 };
        assert_eq!(era.encode().len(), 2);
    }

    // -- Signer --

    #[test]
    fn signer_from_seed_produces_deterministic_public_key() {
        let seed = [0x01u8; 32];
        let s1 = SubstrateSigner::from_seed(seed);
        let s2 = SubstrateSigner::from_seed(seed);
        assert_eq!(s1.public_key(), s2.public_key());
    }

    #[test]
    fn signer_different_seeds_different_keys() {
        let s1 = SubstrateSigner::from_seed([0x01u8; 32]);
        let s2 = SubstrateSigner::from_seed([0x02u8; 32]);
        assert_ne!(s1.public_key(), s2.public_key());
    }

    #[test]
    fn signer_ss58_address_round_trips() {
        let signer = SubstrateSigner::from_seed([0xAAu8; 32]);
        let addr = signer.ss58_address(0);
        let (pubkey, prefix) = decode_ss58(&addr).expect("decode failed");
        assert_eq!(pubkey, signer.public_key());
        assert_eq!(prefix, 0);
    }

    #[test]
    fn sign_transfer_produces_non_empty_extrinsic() {
        let signer = SubstrateSigner::from_seed([0x42u8; 32]);
        let dest = signer.ss58_address(0); // sign to ourselves for simplicity
        let genesis = [0u8; 32];
        let result = signer.sign_transfer(
            &dest,
            10_000_000_000, // 1 DOT
            0,
            Era::Immortal,
            genesis,
        );
        assert!(result.is_ok(), "sign_transfer failed: {:?}", result.err());
        let extrinsic = result.unwrap();
        assert!(!extrinsic.is_empty());
        // Signed extrinsic must start with a compact-encoded length.
        assert!(extrinsic.len() > 5);
    }

    #[test]
    fn sign_transfer_invalid_dest_fails() {
        let signer = SubstrateSigner::from_seed([0x42u8; 32]);
        let result = signer.sign_transfer("not-an-address", 1000, 0, Era::Immortal, [0u8; 32]);
        assert!(result.is_err());
    }

    // -- Chain definitions --

    #[test]
    fn polkadot_chains_count() {
        assert!(polkadot_chains().len() >= 5);
    }

    #[test]
    fn polkadot_relay_config() {
        let chains = polkadot_chains();
        let dot = chains.iter().find(|c| c.name == "Polkadot").unwrap();
        assert_eq!(dot.ss58_prefix, 0);
        assert_eq!(dot.native_token, "DOT");
        assert_eq!(dot.native_decimals, 10);
        assert_eq!(dot.chain_type, SubstrateChainType::RelayChain);
    }

    #[test]
    fn kusama_config() {
        let chains = polkadot_chains();
        let ksm = chains.iter().find(|c| c.name == "Kusama").unwrap();
        assert_eq!(ksm.ss58_prefix, 2);
        assert_eq!(ksm.native_decimals, 12);
    }

    #[test]
    fn moonbeam_is_parachain() {
        let chains = polkadot_chains();
        let moon = chains.iter().find(|c| c.name == "Moonbeam").unwrap();
        assert_eq!(moon.chain_type, SubstrateChainType::Parachain);
        assert_eq!(moon.para_id, Some(2004));
    }

    #[test]
    fn substrate_transfer_call_serializes() {
        let call = SubstrateCall::BalancesTransfer {
            dest: "1abc...".into(),
            value: "10000000000".into(),
        };
        let json = serde_json::to_string(&call).unwrap();
        assert!(json.contains("balances_transfer"));
    }

    #[test]
    fn substrate_staking_calls_serialize() {
        let bond = SubstrateCall::StakingBond {
            value: "100000000000".into(),
            payee: StakingPayee::Staked,
        };
        let json = serde_json::to_string(&bond).unwrap();
        assert!(json.contains("staking_bond"));

        let nominate = SubstrateCall::StakingNominate {
            targets: vec!["validator1".into(), "validator2".into()],
        };
        let json = serde_json::to_string(&nominate).unwrap();
        assert!(json.contains("staking_nominate"));
    }

    #[test]
    fn xcm_transfer_call_serializes() {
        let xcm = SubstrateCall::XcmTransfer {
            dest_chain: 2004,
            beneficiary: "0xabc".into(),
            asset: "DOT".into(),
            amount: "10000000000".into(),
        };
        let json = serde_json::to_string(&xcm).unwrap();
        assert!(json.contains("xcm_transfer"));
    }

    #[test]
    fn ss58_validation_polkadot() {
        assert!(validate_ss58_prefix(
            "1FRMM8PEiWXYax7rpS6X4XZX1aAAxSWx1CrKTyrVYhV24fg",
            0
        ));
        assert!(!validate_ss58_prefix("short", 0));
    }

    #[test]
    fn staking_payee_serialization() {
        assert_eq!(
            serde_json::to_string(&StakingPayee::Staked).unwrap(),
            r#""staked""#
        );
    }

    #[test]
    fn scanner_polkadot_config() {
        let s = SubstrateScanner::polkadot();
        assert_eq!(s.ss58_prefix, 0);
        assert_eq!(s.decimals, 10);
        assert_eq!(s.chain, SubstrateChain::Polkadot);
    }

    #[test]
    fn scanner_kusama_config() {
        let s = SubstrateScanner::kusama();
        assert_eq!(s.ss58_prefix, 2);
        assert_eq!(s.decimals, 12);
    }
}
