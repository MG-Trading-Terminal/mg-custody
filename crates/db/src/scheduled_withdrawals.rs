//! Scheduled withdrawal repository.
//!
//! CRUD for scheduled_withdrawals table. Amounts stored as TEXT to preserve
//! exact Decimal precision — never cast to float at any point.

use chrono::{DateTime, Utc};
use custody_common::scheduled_withdrawal::{ScheduledWithdrawal, WithdrawalSchedule};
use rust_decimal::Decimal;
use std::str::FromStr;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    /// Insert a new scheduled withdrawal.
    ///
    /// Idempotent: if the same `id` already exists, the insert is a no-op
    /// (ON CONFLICT DO NOTHING). The caller should check for the existing row
    /// if needed.
    pub async fn insert_scheduled_withdrawal(
        &self,
        sw: &ScheduledWithdrawal,
    ) -> anyhow::Result<()> {
        let (schedule_type, schedule_config) = encode_schedule(&sw.schedule);

        sqlx::query(
            "INSERT INTO scheduled_withdrawals
             (id, user_id, chain, asset, amount, destination,
              schedule_type, schedule_config,
              next_execution, last_executed,
              executions_count, max_executions, enabled, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(sw.id)
        .bind(sw.user_id)
        .bind(sw.chain.as_str())
        .bind(&sw.asset)
        .bind(sw.amount.to_string()) // TEXT — exact decimal, no float
        .bind(&sw.destination)
        .bind(&schedule_type)
        .bind(&schedule_config)
        .bind(sw.next_execution)
        .bind(sw.last_executed)
        .bind(sw.executions_count as i32)
        .bind(sw.max_executions.map(|x| x as i32))
        .bind(sw.enabled)
        .bind(sw.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List all scheduled withdrawals for a user.
    pub async fn list_scheduled_withdrawals(
        &self,
        user_id: Uuid,
    ) -> anyhow::Result<Vec<ScheduledWithdrawal>> {
        let rows: Vec<SwRow> = sqlx::query_as(
            "SELECT * FROM scheduled_withdrawals
             WHERE user_id = $1
             ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_sw).collect()
    }

    /// Load all enabled schedules due for execution at `now`.
    ///
    /// Used by the background scheduler loop (every 60 seconds).
    pub async fn due_scheduled_withdrawals(
        &self,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Vec<ScheduledWithdrawal>> {
        let rows: Vec<SwRow> = sqlx::query_as(
            "SELECT * FROM scheduled_withdrawals
             WHERE enabled = true AND next_execution <= $1
             ORDER BY next_execution ASC",
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_sw).collect()
    }

    /// Advance a schedule after a successful execution.
    ///
    /// Updates `last_executed`, `executions_count`, `next_execution`, and
    /// `enabled` atomically.
    pub async fn advance_scheduled_withdrawal(
        &self,
        id: Uuid,
        last_executed: DateTime<Utc>,
        next_execution: DateTime<Utc>,
        executions_count: u32,
        enabled: bool,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE scheduled_withdrawals
             SET last_executed = $1,
                 next_execution = $2,
                 executions_count = $3,
                 enabled = $4
             WHERE id = $5",
        )
        .bind(last_executed)
        .bind(next_execution)
        .bind(executions_count as i32)
        .bind(enabled)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Disable (cancel) a scheduled withdrawal.
    ///
    /// Returns `true` if a row was updated, `false` if `id` not found.
    pub async fn cancel_scheduled_withdrawal(
        &self,
        id: Uuid,
        user_id: Uuid,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE scheduled_withdrawals
             SET enabled = false
             WHERE id = $1 AND user_id = $2 AND enabled = true",
        )
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Find a specific scheduled withdrawal by ID.
    pub async fn find_scheduled_withdrawal(
        &self,
        id: Uuid,
    ) -> anyhow::Result<Option<ScheduledWithdrawal>> {
        let row: Option<SwRow> = sqlx::query_as(
            "SELECT * FROM scheduled_withdrawals WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_sw).transpose()
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct SwRow {
    id: Uuid,
    user_id: Uuid,
    chain: String,
    asset: String,
    amount: String,
    destination: String,
    schedule_type: String,
    schedule_config: serde_json::Value,
    next_execution: DateTime<Utc>,
    last_executed: Option<DateTime<Utc>>,
    executions_count: i32,
    max_executions: Option<i32>,
    enabled: bool,
    created_at: DateTime<Utc>,
}

fn row_to_sw(row: SwRow) -> anyhow::Result<ScheduledWithdrawal> {
    use crate::addresses::chain_from_str;
    

    let schedule = decode_schedule(&row.schedule_type, &row.schedule_config)?;
    let amount = Decimal::from_str(&row.amount)
        .map_err(|e| anyhow::anyhow!("Bad amount in scheduled_withdrawal {}: {}", row.id, e))?;

    Ok(ScheduledWithdrawal {
        id: row.id,
        user_id: row.user_id,
        chain: chain_from_str(&row.chain),
        asset: row.asset,
        amount,
        destination: row.destination,
        schedule,
        next_execution: row.next_execution,
        last_executed: row.last_executed,
        executions_count: row.executions_count as u32,
        max_executions: row.max_executions.map(|x| x as u32),
        enabled: row.enabled,
        created_at: row.created_at,
    })
}

/// Serialize a `WithdrawalSchedule` to (schedule_type TEXT, schedule_config JSONB).
fn encode_schedule(schedule: &WithdrawalSchedule) -> (String, serde_json::Value) {
    match schedule {
        WithdrawalSchedule::Once { at } => (
            "once".into(),
            serde_json::json!({ "at": at.to_rfc3339() }),
        ),
        WithdrawalSchedule::Daily { hour_utc } => (
            "daily".into(),
            serde_json::json!({ "hour_utc": hour_utc }),
        ),
        WithdrawalSchedule::Weekly { day, hour_utc } => (
            "weekly".into(),
            serde_json::json!({ "day": format!("{:?}", day).to_lowercase(), "hour_utc": hour_utc }),
        ),
        WithdrawalSchedule::Monthly { day_of_month, hour_utc } => (
            "monthly".into(),
            serde_json::json!({ "day_of_month": day_of_month, "hour_utc": hour_utc }),
        ),
    }
}

/// Deserialize a `WithdrawalSchedule` from (schedule_type TEXT, schedule_config JSONB).
fn decode_schedule(
    schedule_type: &str,
    config: &serde_json::Value,
) -> anyhow::Result<WithdrawalSchedule> {
    

    match schedule_type {
        "once" => {
            let at_str = config["at"].as_str().ok_or_else(|| anyhow::anyhow!("missing 'at'"))?;
            let at = at_str.parse::<DateTime<Utc>>()
                .map_err(|e| anyhow::anyhow!("bad 'at': {e}"))?;
            Ok(WithdrawalSchedule::Once { at })
        }
        "daily" => {
            let hour_utc = config["hour_utc"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("missing 'hour_utc'"))? as u8;
            Ok(WithdrawalSchedule::Daily { hour_utc })
        }
        "weekly" => {
            let day_str = config["day"].as_str().ok_or_else(|| anyhow::anyhow!("missing 'day'"))?;
            let day = parse_weekday(day_str)
                .ok_or_else(|| anyhow::anyhow!("unknown weekday: {day_str}"))?;
            let hour_utc = config["hour_utc"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("missing 'hour_utc'"))? as u8;
            Ok(WithdrawalSchedule::Weekly { day, hour_utc })
        }
        "monthly" => {
            let day_of_month = config["day_of_month"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("missing 'day_of_month'"))? as u8;
            let hour_utc = config["hour_utc"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("missing 'hour_utc'"))? as u8;
            Ok(WithdrawalSchedule::Monthly { day_of_month, hour_utc })
        }
        other => Err(anyhow::anyhow!("Unknown schedule_type: {other}")),
    }
}

fn parse_weekday(s: &str) -> Option<chrono::Weekday> {
    use chrono::Weekday::*;
    match s.to_lowercase().as_str() {
        "mon" | "monday" => Some(Mon),
        "tue" | "tuesday" => Some(Tue),
        "wed" | "wednesday" => Some(Wed),
        "thu" | "thursday" => Some(Thu),
        "fri" | "friday" => Some(Fri),
        "sat" | "saturday" => Some(Sat),
        "sun" | "sunday" => Some(Sun),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Weekday;

    #[test]
    fn encode_decode_once() {
        let at = Utc::now();
        let schedule = WithdrawalSchedule::Once { at };
        let (t, c) = encode_schedule(&schedule);
        assert_eq!(t, "once");
        let decoded = decode_schedule(&t, &c).unwrap();
        match decoded {
            WithdrawalSchedule::Once { at: decoded_at } => {
                // RFC3339 round-trip: second precision
                assert_eq!(decoded_at.timestamp(), at.timestamp());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn encode_decode_daily() {
        let schedule = WithdrawalSchedule::Daily { hour_utc: 9 };
        let (t, c) = encode_schedule(&schedule);
        let decoded = decode_schedule(&t, &c).unwrap();
        assert_eq!(decoded, WithdrawalSchedule::Daily { hour_utc: 9 });
    }

    #[test]
    fn encode_decode_weekly() {
        let schedule = WithdrawalSchedule::Weekly { day: Weekday::Fri, hour_utc: 14 };
        let (t, c) = encode_schedule(&schedule);
        let decoded = decode_schedule(&t, &c).unwrap();
        assert_eq!(decoded, WithdrawalSchedule::Weekly { day: Weekday::Fri, hour_utc: 14 });
    }

    #[test]
    fn encode_decode_monthly() {
        let schedule = WithdrawalSchedule::Monthly { day_of_month: 15, hour_utc: 0 };
        let (t, c) = encode_schedule(&schedule);
        let decoded = decode_schedule(&t, &c).unwrap();
        assert_eq!(decoded, WithdrawalSchedule::Monthly { day_of_month: 15, hour_utc: 0 });
    }

    #[test]
    fn decode_unknown_type_errors() {
        let result = decode_schedule("hourly", &serde_json::json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn parse_weekday_all_variants() {
        for (s, expected) in [
            ("mon", Weekday::Mon), ("tuesday", Weekday::Tue),
            ("WED", Weekday::Wed), ("thu", Weekday::Thu),
            ("fri", Weekday::Fri), ("saturday", Weekday::Sat),
            ("sun", Weekday::Sun),
        ] {
            assert_eq!(parse_weekday(s), Some(expected), "failed for {s}");
        }
    }

    #[test]
    fn amount_roundtrip_no_float() {
        // Exact decimal preserved through TEXT storage
        let original = rust_decimal_macros::dec!(123456789.000000001);
        let stored = original.to_string();
        let recovered = Decimal::from_str(&stored).unwrap();
        assert_eq!(recovered, original);
    }
}
