//! Analytics DB queries — daily summaries, fee breakdowns, top transactions, flow timeseries.
//!
//! All monetary values use rust_decimal::Decimal. Never f64.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::CustodyDb;

// ---------------------------------------------------------------------------
// Row types
// ---------------------------------------------------------------------------

/// A daily aggregate row returned from `daily_summary`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DailySummaryRow {
    pub date: NaiveDate,
    pub total_deposits: i64,
    pub total_withdrawals: i64,
    pub inflow_usd: Option<rust_decimal::Decimal>,
    pub outflow_usd: Option<rust_decimal::Decimal>,
    pub fees_usd: Option<rust_decimal::Decimal>,
    pub busiest_chain: Option<String>,
    pub largest_tx_usd: Option<rust_decimal::Decimal>,
}

/// Per-chain fee totals.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FeeBreakdownRow {
    pub chain: String,
    pub total_fees_native: Option<Decimal>,
    pub total_fees_usd: Option<Decimal>,
    pub tx_count: i64,
}

/// A single top transaction entry.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TopTransactionRow {
    pub id: Uuid,
    pub tx_type: String,
    pub asset: String,
    pub chain: String,
    pub amount_usd: Option<Decimal>,
    pub fee_usd: Option<Decimal>,
    pub created_at: DateTime<Utc>,
    pub tx_hash: Option<String>,
}

/// Flow time-series point.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FlowTimeseriesRow {
    pub bucket: DateTime<Utc>,
    pub inflow_usd: Option<Decimal>,
    pub outflow_usd: Option<Decimal>,
    pub fees_usd: Option<Decimal>,
    pub tx_count: i64,
}

// ---------------------------------------------------------------------------
// CustodyDb impl
// ---------------------------------------------------------------------------

impl CustodyDb {
    /// Aggregate deposits, withdrawals, and fees for a single day.
    ///
    /// Queries the `transactions` table (unified view of deposits + withdrawals).
    pub async fn daily_summary(&self, date: NaiveDate) -> anyhow::Result<DailySummaryRow> {
        let start: DateTime<Utc> = date.and_hms_opt(0, 0, 0)
            .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
            .ok_or_else(|| anyhow::anyhow!("invalid date: {date}"))?;
        let end = start + chrono::Duration::days(1);

        let row: Option<DailySummaryRow> = sqlx::query_as(
            r#"
            SELECT
                CAST($1 AS DATE)                                    AS date,
                COUNT(*) FILTER (WHERE tx_type = 'deposit')         AS total_deposits,
                COUNT(*) FILTER (WHERE tx_type = 'withdrawal')       AS total_withdrawals,
                SUM(amount_usd) FILTER (WHERE tx_type = 'deposit')   AS inflow_usd,
                SUM(amount_usd) FILTER (WHERE tx_type = 'withdrawal') AS outflow_usd,
                SUM(fee_usd)                                         AS fees_usd,
                (
                    SELECT chain FROM transactions
                    WHERE created_at >= $2 AND created_at < $3
                    GROUP BY chain ORDER BY COUNT(*) DESC LIMIT 1
                )                                                    AS busiest_chain,
                MAX(amount_usd)                                      AS largest_tx_usd
            FROM transactions
            WHERE created_at >= $2 AND created_at < $3
            "#,
        )
        .bind(date)
        .bind(start)
        .bind(end)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.unwrap_or(DailySummaryRow {
            date,
            total_deposits: 0,
            total_withdrawals: 0,
            inflow_usd: None,
            outflow_usd: None,
            fees_usd: None,
            busiest_chain: None,
            largest_tx_usd: None,
        }))
    }

    /// Per-chain fee totals for a date range.
    pub async fn fee_breakdown_by_chain(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> anyhow::Result<Vec<FeeBreakdownRow>> {
        let rows: Vec<FeeBreakdownRow> = sqlx::query_as(
            r#"
            SELECT
                chain,
                SUM(fee_native)  AS total_fees_native,
                SUM(fee_usd)     AS total_fees_usd,
                COUNT(*)         AS tx_count
            FROM transactions
            WHERE created_at >= $1 AND created_at < $2
              AND fee_usd IS NOT NULL
            GROUP BY chain
            ORDER BY total_fees_usd DESC NULLS LAST
            "#,
        )
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Top N transactions by USD amount in a date range.
    pub async fn top_transactions(
        &self,
        limit: i64,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> anyhow::Result<Vec<TopTransactionRow>> {
        let rows: Vec<TopTransactionRow> = sqlx::query_as(
            r#"
            SELECT
                id,
                tx_type,
                asset,
                chain,
                amount_usd,
                fee_usd,
                created_at,
                tx_hash
            FROM transactions
            WHERE created_at >= $1 AND created_at < $2
            ORDER BY amount_usd DESC NULLS LAST
            LIMIT $3
            "#,
        )
        .bind(from)
        .bind(to)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Time-series flow data bucketed by `granularity_hours`.
    ///
    /// Uses `date_trunc` for daily/hourly granularity.
    pub async fn flow_timeseries(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        granularity_hours: u32,
    ) -> anyhow::Result<Vec<FlowTimeseriesRow>> {
        // Truncation unit: 'hour' for < 24h, 'day' for >= 24h.
        let trunc_unit = if granularity_hours < 24 { "hour" } else { "day" };

        let rows: Vec<FlowTimeseriesRow> = sqlx::query_as(&format!(
            r#"
            SELECT
                date_trunc('{trunc_unit}', created_at) AS bucket,
                SUM(amount_usd) FILTER (WHERE tx_type = 'deposit')   AS inflow_usd,
                SUM(amount_usd) FILTER (WHERE tx_type = 'withdrawal') AS outflow_usd,
                SUM(fee_usd)                                          AS fees_usd,
                COUNT(*)                                              AS tx_count
            FROM transactions
            WHERE created_at >= $1 AND created_at < $2
            GROUP BY bucket
            ORDER BY bucket
            "#
        ))
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

// ---------------------------------------------------------------------------
// Tests (unit, no DB)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn daily_summary_row_default_zero() {
        let date = NaiveDate::from_ymd_opt(2026, 3, 27).unwrap();
        let row = DailySummaryRow {
            date,
            total_deposits: 0,
            total_withdrawals: 0,
            inflow_usd: None,
            outflow_usd: None,
            fees_usd: None,
            busiest_chain: None,
            largest_tx_usd: None,
        };
        assert_eq!(row.total_deposits, 0);
        assert_eq!(row.total_withdrawals, 0);
        assert!(row.inflow_usd.is_none());
    }

    #[test]
    fn fee_breakdown_row_fields() {
        let row = FeeBreakdownRow {
            chain: "ethereum".into(),
            total_fees_native: Some(rust_decimal_macros::dec!(0.015)),
            total_fees_usd: Some(rust_decimal_macros::dec!(45.0)),
            tx_count: 3,
        };
        assert_eq!(row.chain, "ethereum");
        assert_eq!(row.tx_count, 3);
    }

    #[test]
    fn top_transaction_row_fields() {
        let row = TopTransactionRow {
            id: Uuid::nil(),
            tx_type: "withdrawal".into(),
            asset: "USDT".into(),
            chain: "tron".into(),
            amount_usd: Some(rust_decimal_macros::dec!(50000)),
            fee_usd: Some(rust_decimal_macros::dec!(1.5)),
            created_at: Utc::now(),
            tx_hash: Some("abc123".into()),
        };
        assert_eq!(row.tx_type, "withdrawal");
        assert_eq!(row.chain, "tron");
    }

    #[test]
    fn flow_timeseries_row_fields() {
        let row = FlowTimeseriesRow {
            bucket: Utc.with_ymd_and_hms(2026, 3, 27, 0, 0, 0).unwrap(),
            inflow_usd: Some(rust_decimal_macros::dec!(100000)),
            outflow_usd: Some(rust_decimal_macros::dec!(80000)),
            fees_usd: Some(rust_decimal_macros::dec!(200)),
            tx_count: 42,
        };
        assert_eq!(row.tx_count, 42);
        assert!(row.inflow_usd.is_some());
    }
}
