//! Travel rule data persistence.

use chrono::{DateTime, Utc};
use custody_common::platform::{TravelRuleData, TravelRuleParty, TravelRuleStatus, TrustParties};
use custody_common::{Asset, Chain};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    pub async fn insert_travel_rule_data(&self, tr: &TravelRuleData) -> anyhow::Result<()> {
        let originator = serde_json::to_value(&tr.originator)?;
        let beneficiary = serde_json::to_value(&tr.beneficiary)?;
        let status = serde_json::to_string(&tr.status)?;

        sqlx::query(
            "INSERT INTO travel_rule_data (id, withdrawal_id, originator, beneficiary, amount, asset, chain, threshold_currency, threshold_amount, status, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT (withdrawal_id) DO NOTHING",
        )
        .bind(tr.id)
        .bind(tr.withdrawal_id)
        .bind(&originator)
        .bind(&beneficiary)
        .bind(tr.amount)
        .bind(tr.asset.as_str())
        .bind(tr.chain.as_str())
        .bind(&tr.threshold_currency)
        .bind(tr.threshold_amount)
        .bind(status.trim_matches('"'))
        .bind(tr.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Queue TRUST once per withdrawal. Only failures before sending can be requeued.
    pub async fn queue_trust_dispatch(&self, tr: &TravelRuleData, parties: &TrustParties) -> anyhow::Result<TravelRuleData> {
        sqlx::query(
            "INSERT INTO travel_rule_data (id, withdrawal_id, originator, beneficiary, amount,
                asset, chain, threshold_currency, threshold_amount, status, created_at, trust_parties)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'required', $10, $11)
             ON CONFLICT (withdrawal_id) DO UPDATE SET trust_parties = EXCLUDED.trust_parties,
                status = 'required', trust_tx_hash = NULL, trust_attempted_at = NULL
             WHERE (travel_rule_data.status = 'required' AND travel_rule_data.trust_parties IS NULL)
                OR (travel_rule_data.status = 'failed' AND travel_rule_data.trust_parties IS NOT NULL)",
        )
        .bind(tr.id)
        .bind(tr.withdrawal_id)
        .bind(serde_json::to_value(&tr.originator)?)
        .bind(serde_json::to_value(&tr.beneficiary)?)
        .bind(tr.amount)
        .bind(tr.asset.as_str())
        .bind(tr.chain.as_str())
        .bind(&tr.threshold_currency)
        .bind(tr.threshold_amount)
        .bind(tr.created_at)
        .bind(serde_json::to_value(parties)?)
        .execute(&self.pool)
        .await?;
        self.find_travel_rule_by_withdrawal(tr.withdrawal_id).await?
            .ok_or_else(|| anyhow::anyhow!("Missing Travel Rule record after queueing"))
    }

    /// Whether this withdrawal has active or submitted TRUST data.
    pub async fn has_trust_dispatch(&self, withdrawal_id: Uuid) -> anyhow::Result<bool> {
        Ok(self.trust_dispatch_status(withdrawal_id).await?.is_some_and(|status| status != "failed"))
    }

    /// Return the status if this withdrawal is assigned to TRUST.
    pub async fn trust_dispatch_status(&self, withdrawal_id: Uuid) -> anyhow::Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT status FROM travel_rule_data
             WHERE withdrawal_id = $1 AND trust_parties IS NOT NULL")
            .bind(withdrawal_id).fetch_optional(&self.pool).await?)
    }

    /// Claim one broadcast withdrawal for sending or a stale attempt for read-only reconciliation.
    pub async fn claim_trust_dispatch(&self, reconcile: bool) -> anyhow::Result<Option<TrustDispatch>> {
        Ok(sqlx::query_as(
            "WITH next AS (
                SELECT tr.id, w.tx_hash, w.destination_address FROM travel_rule_data tr
                JOIN withdrawals w ON w.id = tr.withdrawal_id
                WHERE tr.trust_parties IS NOT NULL AND (
                    (NOT $1 AND tr.status = 'required' AND w.tx_hash IS NOT NULL
                        AND w.status IN ('broadcast', 'confirming', 'finalized'))
                    OR ($1 AND tr.status = 'dispatching' AND tr.trust_tx_hash IS NOT NULL
                        AND tr.trust_attempted_at < NOW() - INTERVAL '5 minutes')
                )
                ORDER BY tr.created_at, tr.id LIMIT 1 FOR UPDATE OF tr SKIP LOCKED
             )
             UPDATE travel_rule_data tr SET status = 'dispatching', trust_attempted_at = NOW(),
                       trust_tx_hash = COALESCE(tr.trust_tx_hash, next.tx_hash)
             FROM next WHERE tr.id = next.id
             RETURNING tr.id, tr.withdrawal_id, tr.asset, tr.chain, tr.amount, tr.trust_parties,
                       tr.trust_tx_hash AS tx_hash, next.destination_address",
        ).bind(reconcile).fetch_optional(&self.pool).await?)
    }

    /// Persist the receipt or mark the dispatch for operator review.
    pub async fn finish_trust_dispatch(&self, id: Uuid, record_id: Option<&str>) -> anyhow::Result<()> {
        sqlx::query("UPDATE travel_rule_data SET status = $2, trust_record_id = $3
                     WHERE id = $1 AND status = 'dispatching'")
            .bind(id)
            .bind(if record_id.is_some() { "submitted" } else { "failed" })
            .bind(record_id)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn find_travel_rule_by_withdrawal(&self, withdrawal_id: Uuid) -> anyhow::Result<Option<TravelRuleData>> {
        let row: Option<TravelRuleRow> = sqlx::query_as(
            "SELECT id, withdrawal_id, originator, beneficiary, amount, asset, chain, threshold_currency, threshold_amount, status, trust_record_id, created_at
             FROM travel_rule_data WHERE withdrawal_id = $1",
        )
        .bind(withdrawal_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.and_then(|r| r.into_data().ok()))
    }

    pub async fn update_travel_rule_status(&self, id: Uuid, status: &str) -> anyhow::Result<bool> {
        let result = sqlx::query("UPDATE travel_rule_data SET status = $2 WHERE id = $1")
            .bind(id)
            .bind(status)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

/// A single claimed notification with the broadcast transaction identity.
#[derive(sqlx::FromRow)]
pub struct TrustDispatch {
    pub id: Uuid,
    pub withdrawal_id: Uuid,
    pub asset: String,
    pub chain: String,
    pub amount: Decimal,
    pub trust_parties: sqlx::types::Json<TrustParties>,
    pub tx_hash: String,
    pub destination_address: String,
}

#[derive(sqlx::FromRow)]
struct TravelRuleRow {
    id: Uuid,
    withdrawal_id: Uuid,
    originator: serde_json::Value,
    beneficiary: serde_json::Value,
    amount: Decimal,
    asset: String,
    chain: String,
    threshold_currency: String,
    threshold_amount: Decimal,
    status: String,
    trust_record_id: Option<String>,
    created_at: DateTime<Utc>,
}

impl TravelRuleRow {
    fn into_data(self) -> anyhow::Result<TravelRuleData> {
        let originator: TravelRuleParty = serde_json::from_value(self.originator)?;
        let beneficiary: TravelRuleParty = serde_json::from_value(self.beneficiary)?;
        let status = match self.status.as_str() {
            "dispatching" => TravelRuleStatus::Dispatching,
            "submitted" => TravelRuleStatus::Submitted,
            "verified" => TravelRuleStatus::Verified,
            "exempted" => TravelRuleStatus::Exempted,
            "failed" => TravelRuleStatus::Failed,
            _ => TravelRuleStatus::Required,
        };

        Ok(TravelRuleData {
            id: self.id,
            withdrawal_id: self.withdrawal_id,
            originator,
            beneficiary,
            amount: self.amount,
            asset: Asset::new(&self.asset),
            chain: Chain::try_parse_str(&self.chain)
                .ok_or_else(|| anyhow::anyhow!("travel_rule row {}: unknown chain '{}'", self.id, self.chain))?,
            threshold_currency: self.threshold_currency,
            threshold_amount: self.threshold_amount,
            status,
            trust_record_id: self.trust_record_id,
            created_at: self.created_at,
        })
    }
}
