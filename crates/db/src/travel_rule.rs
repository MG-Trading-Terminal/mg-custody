//! Travel rule data persistence.

use chrono::{DateTime, Utc};
use custody_common::platform::{TravelRuleData, TravelRuleParty, TravelRuleStatus};
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

    pub async fn find_travel_rule_by_withdrawal(&self, withdrawal_id: Uuid) -> anyhow::Result<Option<TravelRuleData>> {
        let row: Option<TravelRuleRow> = sqlx::query_as(
            "SELECT id, withdrawal_id, originator, beneficiary, amount, asset, chain, threshold_currency, threshold_amount, status, created_at
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
    created_at: DateTime<Utc>,
}

impl TravelRuleRow {
    fn into_data(self) -> anyhow::Result<TravelRuleData> {
        let originator: TravelRuleParty = serde_json::from_value(self.originator)?;
        let beneficiary: TravelRuleParty = serde_json::from_value(self.beneficiary)?;
        let status = match self.status.as_str() {
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
            created_at: self.created_at,
        })
    }
}
