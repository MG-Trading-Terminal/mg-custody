//! Custodied NFT persistence.
//!
//! The gateway's NFT routes used to short-circuit every DB call with
//! `TODO` comments — register/get/list all returned invented data.
//! This module backs them with real rows in `custodied_nfts`
//! (migration 025).
//!
//! Listing is always owner-scoped (JOIN on `vaults.owner_id`) so one
//! user can never see another's collection. An admin-wide listing
//! would be fine as a *separate* method, not as the default.

use chrono::{DateTime, Utc};
use custody_common::nft_custody::{CustodiedNft, NftStandard};
use custody_common::Chain;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    /// Insert a newly registered NFT. Returns the provided row verbatim
    /// — callers keep their authoritative copy — but fails with a
    /// duplicate-key error if `(chain, contract_address, token_id,
    /// owner_vault_id)` already exists, per the UNIQUE constraint.
    pub async fn insert_custodied_nft(&self, nft: &CustodiedNft) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO custodied_nfts
                 (id, chain, contract_address, token_id, standard,
                  owner_vault_id, metadata_uri, name, image_url,
                  quantity, acquired_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(nft.id)
        .bind(chain_to_str(nft.chain))
        .bind(&nft.contract_address)
        .bind(&nft.token_id)
        .bind(standard_to_str(nft.standard))
        .bind(nft.owner_vault_id)
        .bind(&nft.metadata_uri)
        .bind(&nft.name)
        .bind(&nft.image_url)
        .bind(nft.quantity as i64)
        .bind(nft.acquired_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Fetch a single NFT by its custody id.
    pub async fn find_custodied_nft(&self, id: Uuid) -> anyhow::Result<Option<CustodiedNft>> {
        let row: Option<NftRow> = sqlx::query_as(
            "SELECT id, chain, contract_address, token_id, standard,
                    owner_vault_id, metadata_uri, name, image_url,
                    quantity, acquired_at
             FROM custodied_nfts WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_nft).transpose()
    }

    /// List NFTs owned by any vault belonging to `user_id`.
    ///
    /// The JOIN-on-owner is what gates user isolation — the gateway
    /// handler no longer has to remember to filter manually.
    pub async fn list_custodied_nfts_for_user(
        &self,
        user_id: Uuid,
    ) -> anyhow::Result<Vec<CustodiedNft>> {
        let rows: Vec<NftRow> = sqlx::query_as(
            "SELECT n.id, n.chain, n.contract_address, n.token_id, n.standard,
                    n.owner_vault_id, n.metadata_uri, n.name, n.image_url,
                    n.quantity, n.acquired_at
             FROM custodied_nfts n
             JOIN vaults v ON v.id = n.owner_vault_id
             WHERE v.owner_id = $1
             ORDER BY n.acquired_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_nft).collect()
    }

    /// Remove a custody record. Returns `true` if a row was deleted.
    /// Used when the token has been transferred out of custody via an
    /// out-of-band path and the record needs to be retired.
    pub async fn delete_custodied_nft(&self, id: Uuid) -> anyhow::Result<bool> {
        let r = sqlx::query("DELETE FROM custodied_nfts WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// Row mapping
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct NftRow {
    id: Uuid,
    chain: String,
    contract_address: String,
    token_id: String,
    standard: String,
    owner_vault_id: Uuid,
    metadata_uri: Option<String>,
    name: Option<String>,
    image_url: Option<String>,
    quantity: i64,
    acquired_at: DateTime<Utc>,
}

fn row_to_nft(r: NftRow) -> anyhow::Result<CustodiedNft> {
    Ok(CustodiedNft {
        id: r.id,
        chain: str_to_chain(&r.chain)?,
        contract_address: r.contract_address,
        token_id: r.token_id,
        standard: str_to_standard(&r.standard)?,
        owner_vault_id: r.owner_vault_id,
        metadata_uri: r.metadata_uri,
        name: r.name,
        image_url: r.image_url,
        quantity: r.quantity.max(0) as u64,
        acquired_at: r.acquired_at,
    })
}

fn chain_to_str(c: Chain) -> &'static str {
    match c {
        Chain::Bitcoin => "bitcoin",
        Chain::Ethereum => "ethereum",
        Chain::Tron => "tron",
        Chain::Solana => "solana",
    }
}

fn str_to_chain(s: &str) -> anyhow::Result<Chain> {
    match s {
        "bitcoin" => Ok(Chain::Bitcoin),
        "ethereum" => Ok(Chain::Ethereum),
        "tron" => Ok(Chain::Tron),
        "solana" => Ok(Chain::Solana),
        other => Err(anyhow::anyhow!("unknown chain in custodied_nfts: {other}")),
    }
}

fn standard_to_str(s: NftStandard) -> &'static str {
    match s {
        NftStandard::Erc721 => "erc721",
        NftStandard::Erc1155 => "erc1155",
        NftStandard::Metaplex => "metaplex",
    }
}

fn str_to_standard(s: &str) -> anyhow::Result<NftStandard> {
    match s {
        "erc721" => Ok(NftStandard::Erc721),
        "erc1155" => Ok(NftStandard::Erc1155),
        "metaplex" => Ok(NftStandard::Metaplex),
        other => Err(anyhow::anyhow!("unknown NFT standard: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_roundtrip() {
        for c in [Chain::Bitcoin, Chain::Ethereum, Chain::Tron, Chain::Solana] {
            assert_eq!(str_to_chain(chain_to_str(c)).unwrap(), c);
        }
    }

    #[test]
    fn standard_roundtrip() {
        for s in [NftStandard::Erc721, NftStandard::Erc1155, NftStandard::Metaplex] {
            assert_eq!(str_to_standard(standard_to_str(s)).unwrap(), s);
        }
    }

    #[test]
    fn unknown_chain_rejected() {
        assert!(str_to_chain("near").is_err());
    }

    #[test]
    fn unknown_standard_rejected() {
        assert!(str_to_standard("erc404").is_err());
    }
}
