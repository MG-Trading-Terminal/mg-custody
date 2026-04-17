//! Idempotent address allocation.
//!
//! Manages address index assignment per chain, ensuring each (user_id, asset, chain)
//! tuple maps to exactly one deposit address. Uses the database as the source of truth
//! for index counters.

use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use custody_common::{Asset, Chain, DepositAddress};
use custody_db::CustodyDb;

use crate::bitcoin::{self, BitcoinNetwork};
use crate::hd::{self, SeedBytes};
use crate::{ethereum, slip0010, solana, tron};

/// Configuration for address generation.
#[derive(Debug, Clone)]
pub struct AddressFactoryConfig {
    pub bitcoin_network: BitcoinNetwork,
}

impl Default for AddressFactoryConfig {
    fn default() -> Self {
        Self {
            bitcoin_network: BitcoinNetwork::Mainnet,
        }
    }
}

/// Idempotent address allocator backed by the database.
///
/// For a given (user_id, asset, chain) tuple, always returns the same address.
/// New addresses are derived deterministically from the HD seed at an
/// atomically-assigned index.
pub struct AddressAllocator {
    db: CustodyDb,
    config: AddressFactoryConfig,
}

impl AddressAllocator {
    pub fn new(db: CustodyDb, config: AddressFactoryConfig) -> Self {
        Self { db, config }
    }

    /// Get or create a deposit address for the given user, asset, and chain.
    ///
    /// Idempotent: returns the same address for the same (user_id, asset, chain).
    /// The seed is passed by reference and never stored.
    pub async fn get_or_create(
        &self,
        seed: &SeedBytes,
        user_id: Uuid,
        asset: &Asset,
        chain: Chain,
    ) -> Result<DepositAddress> {
        // 1. Check for existing address (idempotency)
        if let Some(existing) = self.db.find_address(user_id, asset, chain).await? {
            tracing::debug!(
                user_id = %user_id,
                chain = %chain.as_str(),
                address = %existing.address,
                "Returning existing deposit address"
            );
            return Ok(existing);
        }

        // 2. Allocate next index atomically
        let index = self.db.next_address_index(chain).await?;

        // 3. Derive address
        let (address, derivation_path) = derive_address(seed, chain, index, &self.config)?;

        // 4. Build and persist
        let deposit_address = DepositAddress {
            id: Uuid::new_v4(),
            user_id,
            asset: asset.clone(),
            chain,
            address: address.clone(),
            derivation_path: derivation_path.clone(),
            created_at: Utc::now(),
        };

        self.db.insert_address(&deposit_address).await?;

        tracing::info!(
            user_id = %user_id,
            chain = %chain.as_str(),
            path = %derivation_path,
            address = %address,
            "New deposit address derived"
        );

        Ok(deposit_address)
    }
}

/// Return the hex-encoded public key at the given BIP-44/SLIP-0010
/// index for `chain`.
///
/// Format is hex (no `0x`), lowercased:
///   - secp256k1 (BTC/ETH/Tron): 66 hex chars = 33-byte compressed
///     public key. Callers that need the uncompressed form can
///     re-derive with `hd::derive_secp256k1_pubkey`.
///   - ed25519 (Solana): 64 hex chars = 32-byte raw public key,
///     which is also what Solana addresses base58-encode.
///
/// The previous `GET /v1/vaults/{id}/keys` routes returned an empty
/// string for `public_key` with a "would need to derive from seed"
/// comment — this helper is what that call site needed.
pub fn pubkey_hex_for_index(
    seed: &SeedBytes,
    chain: Chain,
    index: u32,
) -> Result<String> {
    Ok(match chain {
        Chain::Bitcoin | Chain::Ethereum | Chain::Tron => {
            let path = hd::derivation_path(chain, index)?;
            let pubkey = hd::derive_secp256k1_pubkey(seed, &path)?;
            hex::encode(pubkey.compressed)
        }
        Chain::Solana => {
            let slip_path = slip0010::solana_path(index);
            let keypair = slip0010::derive_ed25519_keypair(seed, &slip_path)?;
            hex::encode(keypair.public_key_bytes())
        }
    })
}

/// Parse a stored derivation path string back to its `(chain, index)`
/// tuple so `pubkey_hex_for_index` can be called for an account row
/// whose index wasn't persisted alongside the path.
///
/// Accepts the exact formats emitted by `derivation_path_string`:
/// `m/84'/0'/0'/0/{i}` (BTC), `m/44'/60'/0'/0/{i}` (ETH),
/// `m/44'/195'/0'/0/{i}` (Tron), `m/44'/501'/{i}'/0'` (Solana).
/// Returns `None` for anything else — we refuse to guess and let
/// the caller surface "unknown path format" to the operator.
pub fn chain_and_index_from_path(path: &str) -> Option<(Chain, u32)> {
    let rest = path.strip_prefix("m/").unwrap_or(path);
    let mut parts = rest.split('/');
    let p1 = parts.next()?;
    let p2 = parts.next()?;
    // Extract the numeric index at the right position based on the
    // fixed shape we emit — nothing fancier (path introspection for
    // exotic scripts is a separate story).
    match (p1, p2) {
        ("84'", "0'") => parts.nth(2).and_then(|s| s.parse().ok()).map(|i| (Chain::Bitcoin, i)),
        ("44'", "60'") => parts.nth(2).and_then(|s| s.parse().ok()).map(|i| (Chain::Ethereum, i)),
        ("44'", "195'") => parts.nth(2).and_then(|s| s.parse().ok()).map(|i| (Chain::Tron, i)),
        ("44'", "501'") => {
            // Solana path is `m/44'/501'/{i}'/0'` — the index is the
            // next component minus its hardened suffix.
            let third = parts.next()?;
            let stripped = third.strip_suffix('\'').unwrap_or(third);
            stripped.parse().ok().map(|i| (Chain::Solana, i))
        }
        _ => None,
    }
}

/// Derive an address for the given chain and index.
///
/// Returns (address_string, derivation_path_string).
/// Key material is zeroized when the key types are dropped.
pub fn derive_address(
    seed: &SeedBytes,
    chain: Chain,
    index: u32,
    config: &AddressFactoryConfig,
) -> Result<(String, String)> {
    let path_str = hd::derivation_path_string(chain, index)?;

    let address = match chain {
        Chain::Bitcoin => {
            let path = hd::derivation_path(chain, index)?;
            let pubkey = hd::derive_secp256k1_pubkey(seed, &path)?;
            bitcoin::encode_p2wpkh(&pubkey.compressed, config.bitcoin_network)?
        }
        Chain::Ethereum => {
            let path = hd::derivation_path(chain, index)?;
            let pubkey = hd::derive_secp256k1_pubkey(seed, &path)?;
            ethereum::encode_ethereum_address(&pubkey.uncompressed)?
        }
        Chain::Tron => {
            let path = hd::derivation_path(chain, index)?;
            let pubkey = hd::derive_secp256k1_pubkey(seed, &path)?;
            tron::encode_tron_address(&pubkey.uncompressed)?
        }
        Chain::Solana => {
            let slip_path = slip0010::solana_path(index);
            let keypair = slip0010::derive_ed25519_keypair(seed, &slip_path)?;
            solana::encode_solana_address(&keypair.public_key_bytes())?
        }
    };

    Ok((address, path_str))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_seed() -> SeedBytes {
        let bytes = hex::decode(
            "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
        ).unwrap();
        let mut seed = [0u8; 64];
        seed.copy_from_slice(&bytes);
        SeedBytes(seed)
    }

    fn default_config() -> AddressFactoryConfig {
        AddressFactoryConfig::default()
    }

    #[test]
    fn derive_bitcoin_address() {
        let seed = test_seed();
        let (addr, path) = derive_address(&seed, Chain::Bitcoin, 0, &default_config()).unwrap();
        assert_eq!(addr, "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu");
        assert_eq!(path, "m/84'/0'/0'/0/0");
    }

    #[test]
    fn derive_ethereum_address() {
        let seed = test_seed();
        let (addr, path) = derive_address(&seed, Chain::Ethereum, 0, &default_config()).unwrap();
        assert_eq!(addr, "0x9858EfFD232B4033E47d90003D41EC34EcaEda94");
        assert_eq!(path, "m/44'/60'/0'/0/0");
    }

    #[test]
    fn derive_tron_address() {
        let seed = test_seed();
        let (addr, path) = derive_address(&seed, Chain::Tron, 0, &default_config()).unwrap();
        assert!(addr.starts_with('T'), "Expected T prefix, got: {addr}");
        assert_eq!(path, "m/44'/195'/0'/0/0");
    }

    #[test]
    fn derive_solana_address() {
        let seed = test_seed();
        let (addr, path) = derive_address(&seed, Chain::Solana, 0, &default_config()).unwrap();
        assert!(addr.len() >= 32 && addr.len() <= 44);
        assert_eq!(path, "m/44'/501'/0'/0'");
    }

    #[test]
    fn derive_all_chains_deterministic() {
        let seed = test_seed();
        let config = default_config();

        for chain in [Chain::Bitcoin, Chain::Ethereum, Chain::Tron, Chain::Solana] {
            let (addr1, _) = derive_address(&seed, chain, 0, &config).unwrap();
            let (addr2, _) = derive_address(&seed, chain, 0, &config).unwrap();
            assert_eq!(addr1, addr2, "Address derivation must be deterministic for {chain:?}");
        }
    }

    #[test]
    fn derive_all_chains_unique_per_index() {
        let seed = test_seed();
        let config = default_config();

        for chain in [Chain::Bitcoin, Chain::Ethereum, Chain::Tron, Chain::Solana] {
            let (addr0, _) = derive_address(&seed, chain, 0, &config).unwrap();
            let (addr1, _) = derive_address(&seed, chain, 1, &config).unwrap();
            assert_ne!(addr0, addr1, "Different indices must produce different addresses for {chain:?}");
        }
    }
}
