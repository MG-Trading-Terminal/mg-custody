//! Embedded wallet persistence.

use chrono::{DateTime, Utc};
use custody_common::embedded_wallets::{EmbeddedWallet, EmbeddedWalletType, WalletStatus, EmbeddedAccount};
use custody_common::Chain;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    pub async fn insert_embedded_wallet(&self, w: &EmbeddedWallet) -> anyhow::Result<()> {
        let wtype = serde_json::to_string(&w.wallet_type)?;
        let status = serde_json::to_string(&w.status)?;
        sqlx::query(
            "INSERT INTO embedded_wallets (id, user_id, wallet_type, status, created_at, last_active_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(w.id).bind(&w.user_id)
        .bind(wtype.trim_matches('"')).bind(status.trim_matches('"'))
        .bind(w.created_at).bind(w.last_active_at)
        .execute(&self.pool).await?;

        for acc in &w.accounts {
            sqlx::query(
                "INSERT INTO embedded_accounts (wallet_id, chain, address, derivation_path, is_deployed)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (wallet_id, chain) DO NOTHING",
            )
            .bind(w.id).bind(acc.chain.as_str())
            .bind(&acc.address).bind(&acc.derivation_path).bind(acc.is_deployed)
            .execute(&self.pool).await?;
        }
        Ok(())
    }

    pub async fn find_embedded_wallet(&self, id: Uuid) -> anyhow::Result<Option<EmbeddedWallet>> {
        let row: Option<(Uuid, Uuid, String, String, DateTime<Utc>, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT id, user_id, wallet_type, status, created_at, last_active_at
             FROM embedded_wallets WHERE id = $1",
        ).bind(id).fetch_optional(&self.pool).await?;

        match row {
            Some((id, user_id, wtype, status, created_at, last_active)) => {
                let accounts = self.list_embedded_accounts(id).await?;
                Ok(Some(EmbeddedWallet {
                    id, user_id,
                    wallet_type: match wtype.as_str() { "mpc_split" => EmbeddedWalletType::MpcSplit, "social_recovery" => EmbeddedWalletType::SocialRecovery, _ => EmbeddedWalletType::NonCustodial },
                    status: match status.as_str() { "frozen" => WalletStatus::Frozen, "deactivated" => WalletStatus::Deactivated, _ => WalletStatus::Active },
                    accounts, created_at, last_active_at: last_active,
                }))
            }
            None => Ok(None),
        }
    }

    pub async fn list_embedded_wallets_by_user(&self, user_id: Uuid) -> anyhow::Result<Vec<EmbeddedWallet>> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM embedded_wallets WHERE user_id = $1 ORDER BY created_at",
        ).bind(user_id).fetch_all(&self.pool).await?;

        let mut wallets = Vec::new();
        for (id,) in rows {
            if let Some(w) = self.find_embedded_wallet(id).await? {
                wallets.push(w);
            }
        }
        Ok(wallets)
    }

    async fn list_embedded_accounts(&self, wallet_id: Uuid) -> anyhow::Result<Vec<EmbeddedAccount>> {
        let rows: Vec<(String, String, String, bool)> = sqlx::query_as(
            "SELECT chain, address, derivation_path, is_deployed
             FROM embedded_accounts WHERE wallet_id = $1",
        ).bind(wallet_id).fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(|(chain, address, path, deployed)| EmbeddedAccount {
            chain: Chain::parse_str(&chain), address, derivation_path: path, is_deployed: deployed,
        }).collect())
    }
}
