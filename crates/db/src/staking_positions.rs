//! Staking position persistence.
//!
//! See migration 034. A position is created when a user calls
//! `POST /v1/staking/stake`; the handler inserts BOTH the position
//! row here AND a driving withdrawal row. The withdrawal pipeline
//! picks the withdrawal up, signs + broadcasts; a background task
//! advances the position status when the withdrawal confirms.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::CustodyDb;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StakingStatus {
    Pending,
    Confirmed,
    Unstaking,
    Unstaked,
    Failed,
}

impl StakingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Unstaking => "unstaking",
            Self::Unstaked => "unstaked",
            Self::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "confirmed" => Some(Self::Confirmed),
            "unstaking" => Some(Self::Unstaking),
            "unstaked" => Some(Self::Unstaked),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StakingPosition {
    pub id: Uuid,
    pub user_id: Uuid,
    pub chain: String,
    pub protocol: String,
    pub validator_address: String,
    pub asset: String,
    pub amount: Decimal,
    pub status: StakingStatus,
    pub withdrawal_id: Option<Uuid>,
    pub tx_hash: Option<String>,
    pub unstake_tx_hash: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub unstaked_at: Option<DateTime<Utc>>,
}

impl CustodyDb {
    pub async fn insert_staking_position(&self, p: &StakingPosition) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO staking_positions
                 (id, user_id, chain, protocol, validator_address, asset, amount,
                  status, withdrawal_id, tx_hash, unstake_tx_hash, error_message,
                  created_at, confirmed_at, unstaked_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(p.id)
        .bind(p.user_id)
        .bind(&p.chain)
        .bind(&p.protocol)
        .bind(&p.validator_address)
        .bind(&p.asset)
        .bind(p.amount)
        .bind(p.status.as_str())
        .bind(p.withdrawal_id)
        .bind(&p.tx_hash)
        .bind(&p.unstake_tx_hash)
        .bind(&p.error_message)
        .bind(p.created_at)
        .bind(p.confirmed_at)
        .bind(p.unstaked_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_staking_positions_for_user(
        &self,
        user_id: Uuid,
    ) -> anyhow::Result<Vec<StakingPosition>> {
        let rows: Vec<StakingPositionRow> = sqlx::query_as(
            "SELECT id, user_id, chain, protocol, validator_address, asset, amount,
                    status, withdrawal_id, tx_hash, unstake_tx_hash, error_message,
                    created_at, confirmed_at, unstaked_at
             FROM staking_positions
             WHERE user_id = $1
             ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_position).collect()
    }

    pub async fn find_staking_position_by_withdrawal(
        &self,
        withdrawal_id: Uuid,
    ) -> anyhow::Result<Option<StakingPosition>> {
        let row: Option<StakingPositionRow> = sqlx::query_as(
            "SELECT id, user_id, chain, protocol, validator_address, asset, amount,
                    status, withdrawal_id, tx_hash, unstake_tx_hash, error_message,
                    created_at, confirmed_at, unstaked_at
             FROM staking_positions
             WHERE withdrawal_id = $1",
        )
        .bind(withdrawal_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_position).transpose()
    }

    /// Advance a position's status + record tx_hash. Called by the
    /// staking reconciler task when the driving withdrawal confirms.
    pub async fn update_staking_position_status(
        &self,
        id: Uuid,
        status: StakingStatus,
        tx_hash: Option<&str>,
    ) -> anyhow::Result<()> {
        let confirmed_at = matches!(status, StakingStatus::Confirmed).then(Utc::now);
        let unstaked_at = matches!(status, StakingStatus::Unstaked).then(Utc::now);
        sqlx::query(
            "UPDATE staking_positions
             SET status = $2,
                 tx_hash = COALESCE($3, tx_hash),
                 confirmed_at = COALESCE($4, confirmed_at),
                 unstaked_at = COALESCE($5, unstaked_at)
             WHERE id = $1",
        )
        .bind(id)
        .bind(status.as_str())
        .bind(tx_hash)
        .bind(confirmed_at)
        .bind(unstaked_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct StakingPositionRow {
    id: Uuid,
    user_id: Uuid,
    chain: String,
    protocol: String,
    validator_address: String,
    asset: String,
    amount: Decimal,
    status: String,
    withdrawal_id: Option<Uuid>,
    tx_hash: Option<String>,
    unstake_tx_hash: Option<String>,
    error_message: Option<String>,
    created_at: DateTime<Utc>,
    confirmed_at: Option<DateTime<Utc>>,
    unstaked_at: Option<DateTime<Utc>>,
}

fn row_to_position(r: StakingPositionRow) -> anyhow::Result<StakingPosition> {
    let status = StakingStatus::parse(&r.status)
        .ok_or_else(|| anyhow::anyhow!("unknown staking_positions.status: {}", r.status))?;
    Ok(StakingPosition {
        id: r.id,
        user_id: r.user_id,
        chain: r.chain,
        protocol: r.protocol,
        validator_address: r.validator_address,
        asset: r.asset,
        amount: r.amount,
        status,
        withdrawal_id: r.withdrawal_id,
        tx_hash: r.tx_hash,
        unstake_tx_hash: r.unstake_tx_hash,
        error_message: r.error_message,
        created_at: r.created_at,
        confirmed_at: r.confirmed_at,
        unstaked_at: r.unstaked_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrip() {
        for s in [
            StakingStatus::Pending,
            StakingStatus::Confirmed,
            StakingStatus::Unstaking,
            StakingStatus::Unstaked,
            StakingStatus::Failed,
        ] {
            assert_eq!(StakingStatus::parse(s.as_str()), Some(s));
        }
    }

    #[test]
    fn unknown_status_rejected() {
        assert_eq!(StakingStatus::parse("queued"), None);
    }
}
