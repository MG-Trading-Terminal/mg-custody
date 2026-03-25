//! Address book persistence — saved destination addresses with labels.

use chrono::{DateTime, Utc};
use custody_common::webhook::AddressBookEntry;
use custody_common::Chain;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    pub async fn insert_address_book_entry(&self, e: &AddressBookEntry) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO address_book (id, label, address, chain, tag, whitelisted, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (address, chain) DO NOTHING",
        )
        .bind(e.id)
        .bind(&e.label)
        .bind(&e.address)
        .bind(e.chain.as_str())
        .bind(&e.tag)
        .bind(e.whitelisted)
        .bind(e.created_at)
        .bind(e.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_address_book_entry(&self, id: Uuid) -> anyhow::Result<Option<AddressBookEntry>> {
        let row: Option<AddressBookRow> = sqlx::query_as(
            "SELECT id, label, address, chain, tag, whitelisted, created_at, updated_at
             FROM address_book WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.into_entry()))
    }

    pub async fn find_address_book_by_address(&self, address: &str, chain: &str) -> anyhow::Result<Option<AddressBookEntry>> {
        let row: Option<AddressBookRow> = sqlx::query_as(
            "SELECT id, label, address, chain, tag, whitelisted, created_at, updated_at
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
                    "SELECT id, label, address, chain, tag, whitelisted, created_at, updated_at
                     FROM address_book WHERE chain = $1 ORDER BY label",
                )
                .bind(chain)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    "SELECT id, label, address, chain, tag, whitelisted, created_at, updated_at
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

    pub async fn delete_address_book_entry(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM address_book WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
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
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
