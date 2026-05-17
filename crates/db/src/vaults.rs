//! Vault, VaultAccount, and InternalTransfer persistence.
//!
//! Vaults are the central organizational primitive. Each vault contains
//! multiple VaultAccounts (one per asset+chain), each with a derived address.

use chrono::{DateTime, Utc};
use custody_common::vault::{
    InternalTransfer, InternalTransferStatus, Vault, VaultAccount,
};
use custody_common::{Asset, Chain};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::CustodyDb;

// ---------------------------------------------------------------------------
// Vault CRUD
// ---------------------------------------------------------------------------

impl CustodyDb {
    pub async fn insert_vault(&self, v: &Vault) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO vaults (id, name, owner_id, auto_fuel, hidden, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (owner_id, name) DO NOTHING",
        )
        .bind(v.id)
        .bind(&v.name)
        .bind(v.owner_id)
        .bind(v.auto_fuel)
        .bind(v.hidden)
        .bind(v.created_at)
        .bind(v.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_vault(&self, id: Uuid) -> anyhow::Result<Option<Vault>> {
        let row: Option<(Uuid, String, Uuid, bool, bool, DateTime<Utc>, DateTime<Utc>)> =
            sqlx::query_as(
                "SELECT id, name, owner_id, auto_fuel, hidden, created_at, updated_at
                 FROM vaults WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|(id, name, owner_id, auto_fuel, hidden, created_at, updated_at)| Vault {
            id,
            name,
            owner_id,
            auto_fuel,
            hidden,
            created_at,
            updated_at,
        }))
    }

    pub async fn list_vaults(&self, owner_id: Uuid, include_hidden: bool) -> anyhow::Result<Vec<Vault>> {
        let rows: Vec<(Uuid, String, Uuid, bool, bool, DateTime<Utc>, DateTime<Utc>)> = if include_hidden {
            sqlx::query_as(
                "SELECT id, name, owner_id, auto_fuel, hidden, created_at, updated_at
                 FROM vaults WHERE owner_id = $1 ORDER BY created_at",
            )
            .bind(owner_id)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as(
                "SELECT id, name, owner_id, auto_fuel, hidden, created_at, updated_at
                 FROM vaults WHERE owner_id = $1 AND hidden = false ORDER BY created_at",
            )
            .bind(owner_id)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(rows
            .into_iter()
            .map(|(id, name, owner_id, auto_fuel, hidden, created_at, updated_at)| Vault {
                id, name, owner_id, auto_fuel, hidden, created_at, updated_at,
            })
            .collect())
    }

    pub async fn update_vault(
        &self,
        id: Uuid,
        name: Option<&str>,
        auto_fuel: Option<bool>,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE vaults SET
                name = COALESCE($2, name),
                auto_fuel = COALESCE($3, auto_fuel),
                updated_at = NOW()
             WHERE id = $1",
        )
        .bind(id)
        .bind(name)
        .bind(auto_fuel)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn hide_vault(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE vaults SET hidden = true, updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    // -----------------------------------------------------------------------
    // Workspace-scoped vault queries
    //
    // The `vaults` table does not yet have a `workspace_id` column; workspace
    // membership is expressed through `owner_id` which is set to the user's ID
    // at creation time. These helpers add a second filter so that a user can
    // only access vaults they own — enforcing the per-workspace boundary at the
    // DB layer rather than relying solely on route-level checks.
    // -----------------------------------------------------------------------

    /// List vaults owned by `owner_id` — same as `list_vaults` but the name
    /// makes the workspace-scoped intent explicit at call sites.
    pub async fn list_vaults_for_workspace(
        &self,
        owner_id: Uuid,
        include_hidden: bool,
    ) -> anyhow::Result<Vec<Vault>> {
        self.list_vaults(owner_id, include_hidden).await
    }

    /// Find a vault by `vault_id` only if it is owned by `owner_id`.
    ///
    /// Returns `Ok(None)` — not an error — when the vault exists but belongs
    /// to a different workspace. Callers translate this to 404 so as not to
    /// reveal whether the vault exists at all (prevents enumeration).
    pub async fn find_vault_for_workspace(
        &self,
        vault_id: Uuid,
        owner_id: Uuid,
    ) -> anyhow::Result<Option<Vault>> {
        let row: Option<(Uuid, String, Uuid, bool, bool, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> =
            sqlx::query_as(
                "SELECT id, name, owner_id, auto_fuel, hidden, created_at, updated_at
                 FROM vaults WHERE id = $1 AND owner_id = $2",
            )
            .bind(vault_id)
            .bind(owner_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|(id, name, owner_id, auto_fuel, hidden, created_at, updated_at)| Vault {
            id, name, owner_id, auto_fuel, hidden, created_at, updated_at,
        }))
    }

    // -----------------------------------------------------------------------
    // VaultAccount CRUD
    // -----------------------------------------------------------------------

    pub async fn insert_vault_account(&self, va: &VaultAccount) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO vault_accounts (id, vault_id, asset, chain, address, derivation_path, balance, pending_balance, frozen, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (vault_id, asset, chain) DO NOTHING",
        )
        .bind(va.id)
        .bind(va.vault_id)
        .bind(va.asset.as_str())
        .bind(va.chain.as_str())
        .bind(&va.address)
        .bind(&va.derivation_path)
        .bind(va.balance)
        .bind(va.pending_balance)
        .bind(va.frozen)
        .bind(va.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_vault_account(
        &self,
        vault_id: Uuid,
        asset: &str,
        chain: &str,
    ) -> anyhow::Result<Option<VaultAccount>> {
        let row: Option<VaultAccountRow> = sqlx::query_as(
            "SELECT id, vault_id, asset, chain, address, derivation_path, balance, pending_balance, frozen, created_at
             FROM vault_accounts WHERE vault_id = $1 AND asset = $2 AND chain = $3",
        )
        .bind(vault_id)
        .bind(asset)
        .bind(chain)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| r.into_vault_account()).transpose()
    }

    pub async fn list_vault_accounts(&self, vault_id: Uuid) -> anyhow::Result<Vec<VaultAccount>> {
        let rows: Vec<VaultAccountRow> = sqlx::query_as(
            "SELECT id, vault_id, asset, chain, address, derivation_path, balance, pending_balance, frozen, created_at
             FROM vault_accounts WHERE vault_id = $1 ORDER BY created_at",
        )
        .bind(vault_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(|r| r.into_vault_account()).collect()
    }

    pub async fn find_vault_account_by_id(&self, id: Uuid) -> anyhow::Result<Option<VaultAccount>> {
        let row: Option<VaultAccountRow> = sqlx::query_as(
            "SELECT id, vault_id, asset, chain, address, derivation_path, balance, pending_balance, frozen, created_at
             FROM vault_accounts WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| r.into_vault_account()).transpose()
    }

    pub async fn find_vault_account_by_address(&self, address: &str) -> anyhow::Result<Option<VaultAccount>> {
        let row: Option<VaultAccountRow> = sqlx::query_as(
            "SELECT id, vault_id, asset, chain, address, derivation_path, balance, pending_balance, frozen, created_at
             FROM vault_accounts WHERE address = $1",
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| r.into_vault_account()).transpose()
    }

    pub async fn update_vault_account_balance(
        &self,
        id: Uuid,
        balance: Decimal,
        pending: Decimal,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE vault_accounts SET balance = $2, pending_balance = $3 WHERE id = $1",
        )
        .bind(id)
        .bind(balance)
        .bind(pending)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn freeze_vault_account(&self, id: Uuid, frozen: bool) -> anyhow::Result<()> {
        sqlx::query("UPDATE vault_accounts SET frozen = $2 WHERE id = $1")
            .bind(id)
            .bind(frozen)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Check if a vault has any non-zero balance accounts.
    pub async fn vault_has_balance(&self, vault_id: Uuid) -> anyhow::Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT COUNT(*) FROM vault_accounts WHERE vault_id = $1 AND (balance != 0 OR pending_balance != 0)",
        )
        .bind(vault_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.0 > 0).unwrap_or(false))
    }

    // -----------------------------------------------------------------------
    // Internal transfers
    // -----------------------------------------------------------------------

    pub async fn insert_internal_transfer(&self, t: &InternalTransfer) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO internal_transfers (id, source_vault_id, dest_vault_id, asset, chain, amount, status, idempotency_key, note, created_at, completed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(t.id)
        .bind(t.source_vault_id)
        .bind(t.dest_vault_id)
        .bind(t.asset.as_str())
        .bind(t.chain.as_str())
        .bind(t.amount)
        .bind(match t.status {
            InternalTransferStatus::Pending => "pending",
            InternalTransferStatus::Completed => "completed",
            InternalTransferStatus::Failed => "failed",
            InternalTransferStatus::Cancelled => "cancelled",
        })
        .bind(&t.idempotency_key)
        .bind(&t.note)
        .bind(t.created_at)
        .bind(t.completed_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_internal_transfer(&self, id: Uuid) -> anyhow::Result<Option<InternalTransfer>> {
        let row: Option<InternalTransferRow> = sqlx::query_as(
            "SELECT id, source_vault_id, dest_vault_id, asset, chain, amount, status, idempotency_key, note, created_at, completed_at
             FROM internal_transfers WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| r.into_transfer()).transpose()
    }

    pub async fn find_internal_transfer_by_key(&self, key: &str) -> anyhow::Result<Option<InternalTransfer>> {
        let row: Option<InternalTransferRow> = sqlx::query_as(
            "SELECT id, source_vault_id, dest_vault_id, asset, chain, amount, status, idempotency_key, note, created_at, completed_at
             FROM internal_transfers WHERE idempotency_key = $1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| r.into_transfer()).transpose()
    }

    pub async fn complete_internal_transfer(&self, id: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE internal_transfers SET status = 'completed', completed_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Internal row types (sqlx::FromRow)
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct VaultAccountRow {
    id: Uuid,
    vault_id: Uuid,
    asset: String,
    chain: String,
    address: String,
    derivation_path: String,
    balance: Decimal,
    pending_balance: Decimal,
    frozen: bool,
    created_at: DateTime<Utc>,
}

impl VaultAccountRow {
    fn into_vault_account(self) -> anyhow::Result<VaultAccount> {
        let chain = Chain::try_parse_str(&self.chain)
            .ok_or_else(|| anyhow::anyhow!("vault_accounts row {}: unknown chain '{}'", self.id, self.chain))?;
        Ok(VaultAccount {
            id: self.id,
            vault_id: self.vault_id,
            asset: Asset::new(&self.asset),
            chain,
            address: self.address,
            derivation_path: self.derivation_path,
            balance: self.balance,
            pending_balance: self.pending_balance,
            frozen: self.frozen,
            created_at: self.created_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct InternalTransferRow {
    id: Uuid,
    source_vault_id: Uuid,
    dest_vault_id: Uuid,
    asset: String,
    chain: String,
    amount: Decimal,
    status: String,
    idempotency_key: Option<String>,
    note: Option<String>,
    created_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

impl InternalTransferRow {
    fn into_transfer(self) -> anyhow::Result<InternalTransfer> {
        let status = match self.status.as_str() {
            "completed" => InternalTransferStatus::Completed,
            "failed" => InternalTransferStatus::Failed,
            "cancelled" => InternalTransferStatus::Cancelled,
            _ => InternalTransferStatus::Pending,
        };
        let chain = Chain::try_parse_str(&self.chain)
            .ok_or_else(|| anyhow::anyhow!("internal_transfers row {}: unknown chain '{}'", self.id, self.chain))?;
        Ok(InternalTransfer {
            id: self.id,
            source_vault_id: self.source_vault_id,
            dest_vault_id: self.dest_vault_id,
            asset: Asset::new(&self.asset),
            chain,
            amount: self.amount,
            status,
            idempotency_key: self.idempotency_key,
            note: self.note,
            created_at: self.created_at,
            completed_at: self.completed_at,
        })
    }
}
