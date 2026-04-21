//! Runtime system-config persistence.
//!
//! Schema: see migration 039. Singleton row (`id = 1`).
//!
//! Handlers `get`/`update` used to return/accept
//! `SystemConfig::default()` without touching the DB, which meant
//! every restart wiped operator decisions — most dangerously the
//! `withdrawals_paused` flag. These helpers give the handlers a
//! real read/write path.

use rust_decimal::Decimal;

use crate::CustodyDb;
use custody_common::connectivity::{ConfirmationOverride, SystemConfig};

impl CustodyDb {
    /// Fetch the single system_config row. If the table is empty
    /// (fresh deployment that hasn't written one yet), returns
    /// `SystemConfig::default()` so callers have a sensible
    /// starting point without special-casing a 404.
    pub async fn get_system_config(&self) -> anyhow::Result<SystemConfig> {
        let row: Option<(
            Decimal,
            Decimal,
            Decimal,
            bool,
            bool,
            bool,
            serde_json::Value,
        )> = sqlx::query_as(
            "SELECT withdrawal_limit_single, withdrawal_limit_hourly, \
                    withdrawal_limit_daily, withdrawals_paused, \
                    deposits_paused, maintenance_mode, \
                    confirmation_overrides \
             FROM system_config WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        let Some((
            single,
            hourly,
            daily,
            paused_w,
            paused_d,
            maintenance,
            overrides,
        )) = row
        else {
            return Ok(SystemConfig::default());
        };
        let overrides: Vec<ConfirmationOverride> =
            serde_json::from_value(overrides).unwrap_or_default();
        Ok(SystemConfig {
            withdrawal_limit_single: single,
            withdrawal_limit_hourly: hourly,
            withdrawal_limit_daily: daily,
            withdrawals_paused: paused_w,
            deposits_paused: paused_d,
            confirmation_overrides: overrides,
            maintenance_mode: maintenance,
        })
    }

    /// Upsert the singleton row. `updated_by` is the actor id from
    /// the handler's AuthContext — lands in the row for a quick
    /// "who was last here?" check and in audit_log (handler side)
    /// for the full history.
    pub async fn upsert_system_config(
        &self,
        cfg: &SystemConfig,
        updated_by: &str,
    ) -> anyhow::Result<()> {
        let overrides_json =
            serde_json::to_value(&cfg.confirmation_overrides).unwrap_or_default();
        sqlx::query(
            "INSERT INTO system_config (id, withdrawal_limit_single, \
                withdrawal_limit_hourly, withdrawal_limit_daily, \
                withdrawals_paused, deposits_paused, maintenance_mode, \
                confirmation_overrides, updated_by, updated_at) \
             VALUES (1, $1, $2, $3, $4, $5, $6, $7, $8, clock_timestamp()) \
             ON CONFLICT (id) DO UPDATE SET \
                withdrawal_limit_single = EXCLUDED.withdrawal_limit_single, \
                withdrawal_limit_hourly = EXCLUDED.withdrawal_limit_hourly, \
                withdrawal_limit_daily = EXCLUDED.withdrawal_limit_daily, \
                withdrawals_paused = EXCLUDED.withdrawals_paused, \
                deposits_paused = EXCLUDED.deposits_paused, \
                maintenance_mode = EXCLUDED.maintenance_mode, \
                confirmation_overrides = EXCLUDED.confirmation_overrides, \
                updated_by = EXCLUDED.updated_by, \
                updated_at = clock_timestamp()",
        )
        .bind(cfg.withdrawal_limit_single)
        .bind(cfg.withdrawal_limit_hourly)
        .bind(cfg.withdrawal_limit_daily)
        .bind(cfg.withdrawals_paused)
        .bind(cfg.deposits_paused)
        .bind(cfg.maintenance_mode)
        .bind(overrides_json)
        .bind(updated_by)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
