//! Address book persistence — saved destination addresses with labels.
//!
//! Migration 018 added `notes` and `last_used_at` columns.

use chrono::{DateTime, Utc};
use custody_common::webhook::AddressBookEntry;
use custody_common::Chain;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    pub async fn insert_address_book_entry(&self, e: &AddressBookEntry) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO address_book (id, label, address, chain, tag, whitelisted, notes, last_used_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (address, chain) DO NOTHING",
        )
        .bind(e.id)
        .bind(&e.label)
        .bind(&e.address)
        .bind(e.chain.as_str())
        .bind(&e.tag)
        .bind(e.whitelisted)
        .bind(&e.notes)
        .bind(e.last_used_at)
        .bind(e.created_at)
        .bind(e.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_address_book_entry(&self, id: Uuid) -> anyhow::Result<Option<AddressBookEntry>> {
        let row: Option<AddressBookRow> = sqlx::query_as(
            "SELECT id, label, address, chain, tag, whitelisted, notes, last_used_at, created_at, updated_at
             FROM address_book WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.into_entry()))
    }

    pub async fn find_address_book_by_address(&self, address: &str, chain: &str) -> anyhow::Result<Option<AddressBookEntry>> {
        let row: Option<AddressBookRow> = sqlx::query_as(
            "SELECT id, label, address, chain, tag, whitelisted, notes, last_used_at, created_at, updated_at
             FROM address_book WHERE address = $1 AND chain = $2",
        )
        .bind(address)
        .bind(chain)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.into_entry()))
    }

    pub async fn list_address_book(&self, chain_filter: Option<&str>) -> anyhow::Result<Vec<AddressBookEntry>> {
        let rows: Vec<AddressBookRow> = match chain_filter {
            Some(chain) => {
                sqlx::query_as(
                    "SELECT id, label, address, chain, tag, whitelisted, notes, last_used_at, created_at, updated_at
                     FROM address_book WHERE chain = $1 ORDER BY label",
                )
                .bind(chain)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    "SELECT id, label, address, chain, tag, whitelisted, notes, last_used_at, created_at, updated_at
                     FROM address_book ORDER BY label",
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(rows.into_iter().map(|r| r.into_entry()).collect())
    }

    pub async fn update_address_book_entry(
        &self,
        id: Uuid,
        label: Option<&str>,
        tag: Option<&str>,
        whitelisted: Option<bool>,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE address_book SET
                label = COALESCE($2, label),
                tag = COALESCE($3, tag),
                whitelisted = COALESCE($4, whitelisted),
                updated_at = NOW()
             WHERE id = $1",
        )
        .bind(id)
        .bind(label)
        .bind(tag)
        .bind(whitelisted)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Update the human-readable label and optional notes for an address book entry.
    ///
    /// Used by `PUT /v1/address-book/{id}/label`. Passing `None` for either
    /// field leaves the existing value unchanged (COALESCE semantics).
    pub async fn update_address_label(
        &self,
        id: Uuid,
        label: Option<&str>,
        notes: Option<&str>,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE address_book SET
                label = COALESCE($2, label),
                notes = COALESCE($3, notes),
                updated_at = NOW()
             WHERE id = $1",
        )
        .bind(id)
        .bind(label)
        .bind(notes)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Fuzzy search address book entries by label (case-insensitive substring)
    /// or exact address prefix.
    ///
    /// Returns up to 50 results ordered by label. The query uses `ILIKE` so
    /// it works without the GIN index added in migration 018 (the index just
    /// makes full-text searches faster for large tables).
    pub async fn search_addresses(&self, query: &str) -> anyhow::Result<Vec<AddressBookEntry>> {
        // Sanitise the query: escape ILIKE special characters before wrapping.
        let escaped = query.replace('%', r"\%").replace('_', r"\_");
        let pattern = format!("%{escaped}%");

        let rows: Vec<AddressBookRow> = sqlx::query_as(
            "SELECT id, label, address, chain, tag, whitelisted, notes, last_used_at, created_at, updated_at
             FROM address_book
             WHERE label ILIKE $1 OR address ILIKE $1
             ORDER BY label
             LIMIT 50",
        )
        .bind(&pattern)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_entry()).collect())
    }

    pub async fn delete_address_book_entry(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM address_book WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Check whether a destination address is whitelisted for a given chain.
    ///
    /// Whitelist is opt-in: if the `address_book` table has no rows with
    /// `whitelisted = true` for this chain, all addresses are permitted.
    /// If any whitelisted entries exist for the chain, only those addresses pass.
    ///
    /// Returns `true` if the address is allowed, `false` if it is blocked.
    pub async fn is_address_whitelisted(&self, address: &str, chain: &str) -> anyhow::Result<bool> {
        // Count how many whitelisted entries exist for this chain.
        let (whitelist_count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM address_book WHERE chain = $1 AND whitelisted = true",
        )
        .bind(chain)
        .fetch_one(&self.pool)
        .await?;

        // No whitelisted entries → whitelist is not configured → allow all.
        if whitelist_count == 0 {
            return Ok(true);
        }

        // Whitelist is active: check if this specific address is in it.
        let (present,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM address_book WHERE address = $1 AND chain = $2 AND whitelisted = true",
        )
        .bind(address)
        .bind(chain)
        .fetch_one(&self.pool)
        .await?;

        Ok(present > 0)
    }
}

#[derive(sqlx::FromRow)]
struct AddressBookRow {
    id: Uuid,
    label: String,
    address: String,
    chain: String,
    tag: Option<String>,
    whitelisted: bool,
    notes: Option<String>,
    last_used_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AddressBookRow {
    fn into_entry(self) -> AddressBookEntry {
        AddressBookEntry {
            id: self.id,
            label: self.label,
            address: self.address,
            chain: Chain::parse_str(&self.chain),
            tag: self.tag,
            whitelisted: self.whitelisted,
            notes: self.notes,
            last_used_at: self.last_used_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use custody_common::webhook::{AddressBookEntry, UpdateAddressLabelRequest};

    /// Verify the ILIKE escaping handles SQL wildcards in user input.
    #[test]
    fn search_query_escaping() {
        let raw = "Binance%Hot_Wallet";
        let escaped = raw.replace('%', r"\%").replace('_', r"\_");
        let pattern = format!("%{escaped}%");
        assert_eq!(pattern, r"%Binance\%Hot\_Wallet%");
    }

    /// `UpdateAddressLabelRequest` with both fields `None` must round-trip through JSON.
    #[test]
    fn update_label_request_serde_all_none() {
        let req = UpdateAddressLabelRequest { label: None, notes: None };
        let json = serde_json::to_string(&req).unwrap();
        let back: UpdateAddressLabelRequest = serde_json::from_str(&json).unwrap();
        assert!(back.label.is_none());
        assert!(back.notes.is_none());
    }

    /// Partial update: only label is set.
    #[test]
    fn update_label_request_serde_label_only() {
        let json = r#"{"label":"Binance"}"#;
        let req: UpdateAddressLabelRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.label.as_deref(), Some("Binance"));
        assert!(req.notes.is_none());
    }
}
