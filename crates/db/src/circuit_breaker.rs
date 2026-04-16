//! Persistent circuit breaker state.
//!
//! CRITICAL FIX: Previously counters were in-memory only. Process restart = counter reset.
//! Now all outflow tracking survives restarts via PostgreSQL.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;

use crate::CustodyDb;

/// Persisted circuit breaker state.
#[derive(Debug, Clone)]
pub struct CircuitBreakerRow {
    pub hourly_window_start: DateTime<Utc>,
    pub daily_window_start: DateTime<Utc>,
    pub hourly_outflow_usd: Decimal,
    pub daily_outflow_usd: Decimal,
    pub is_tripped: bool,
    pub tripped_at: Option<DateTime<Utc>>,
    pub trip_reason: Option<String>,
    pub single_tx_limit_usd: Decimal,
    pub hourly_limit_usd: Decimal,
    pub daily_limit_usd: Decimal,
}

impl CustodyDb {
    /// Load circuit breaker state from DB. Called on startup.
    pub async fn load_circuit_breaker(&self) -> anyhow::Result<CircuitBreakerRow> {
        let row: Option<(
            DateTime<Utc>, DateTime<Utc>, Decimal, Decimal,
            bool, Option<DateTime<Utc>>, Option<String>,
            Decimal, Decimal, Decimal,
        )> = sqlx::query_as(
            "SELECT hourly_window_start, daily_window_start, hourly_outflow_usd, daily_outflow_usd,
                    is_tripped, tripped_at, trip_reason,
                    single_tx_limit_usd, hourly_limit_usd, daily_limit_usd
             FROM circuit_breaker_state WHERE id = 'global'",
        )
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some((hws, dws, hourly, daily, tripped, tripped_at, reason, single, h_limit, d_limit)) => {
                let now = Utc::now();
                let mut state = CircuitBreakerRow {
                    hourly_window_start: hws,
                    daily_window_start: dws,
                    hourly_outflow_usd: hourly,
                    daily_outflow_usd: daily,
                    is_tripped: tripped,
                    tripped_at,
                    trip_reason: reason,
                    single_tx_limit_usd: single,
                    hourly_limit_usd: h_limit,
                    daily_limit_usd: d_limit,
                };

                // Roll over windows if expired
                if now - state.hourly_window_start > Duration::hours(1) {
                    state.hourly_window_start = now;
                    state.hourly_outflow_usd = Decimal::ZERO;
                }
                if now - state.daily_window_start > Duration::hours(24) {
                    state.daily_window_start = now;
                    state.daily_outflow_usd = Decimal::ZERO;
                }

                Ok(state)
            }
            None => {
                // No state yet — return defaults
                Ok(CircuitBreakerRow {
                    hourly_window_start: Utc::now(),
                    daily_window_start: Utc::now(),
                    hourly_outflow_usd: Decimal::ZERO,
                    daily_outflow_usd: Decimal::ZERO,
                    is_tripped: false,
                    tripped_at: None,
                    trip_reason: None,
                    single_tx_limit_usd: Decimal::new(500_000, 0),
                    hourly_limit_usd: Decimal::new(10_000_000, 0),
                    daily_limit_usd: Decimal::new(50_000_000, 0),
                })
            }
        }
    }

    /// Check if a withdrawal amount is allowed. Returns Err with reason if blocked.
    /// Also emits Prometheus metrics for monitoring.
    pub async fn check_circuit_breaker(&self, amount_usd: Decimal) -> anyhow::Result<()> {
        use custody_common::prometheus_metrics::{CIRCUIT_BREAKER_TRIPPED, CIRCUIT_BREAKER_OUTFLOW_USD};
        use rust_decimal::prelude::ToPrimitive;

        let state = self.load_circuit_breaker().await?;

        // Emit current state to Prometheus
        let tripped_val = if state.is_tripped { 1.0 } else { 0.0 };
        CIRCUIT_BREAKER_TRIPPED.with_label_values(&["global"]).set(tripped_val);
        CIRCUIT_BREAKER_OUTFLOW_USD.with_label_values(&["hourly"])
            .set(state.hourly_outflow_usd.to_f64().unwrap_or(0.0));
        CIRCUIT_BREAKER_OUTFLOW_USD.with_label_values(&["daily"])
            .set(state.daily_outflow_usd.to_f64().unwrap_or(0.0));

        if state.is_tripped {
            anyhow::bail!("Circuit breaker is tripped: {}", state.trip_reason.as_deref().unwrap_or("unknown"));
        }
        if amount_usd > state.single_tx_limit_usd {
            anyhow::bail!(
                "Single transaction ${} exceeds limit ${}",
                amount_usd, state.single_tx_limit_usd
            );
        }
        if state.hourly_outflow_usd + amount_usd > state.hourly_limit_usd {
            anyhow::bail!(
                "Hourly outflow ${} + ${} exceeds limit ${}",
                state.hourly_outflow_usd, amount_usd, state.hourly_limit_usd
            );
        }
        if state.daily_outflow_usd + amount_usd > state.daily_limit_usd {
            anyhow::bail!(
                "Daily outflow ${} + ${} exceeds limit ${}",
                state.daily_outflow_usd, amount_usd, state.daily_limit_usd
            );
        }
        Ok(())
    }

    /// Record a withdrawal in the circuit breaker.
    /// MUST be called in the same DB transaction as the withdrawal insert.
    pub async fn record_outflow(&self, amount_usd: Decimal) -> anyhow::Result<()> {
        // Atomic update: increment counters, roll over windows if needed
        sqlx::query(
            "UPDATE circuit_breaker_state SET
                hourly_window_start = CASE
                    WHEN NOW() - hourly_window_start > INTERVAL '1 hour' THEN NOW()
                    ELSE hourly_window_start
                END,
                hourly_outflow_usd = CASE
                    WHEN NOW() - hourly_window_start > INTERVAL '1 hour' THEN $1
                    ELSE hourly_outflow_usd + $1
                END,
                daily_window_start = CASE
                    WHEN NOW() - daily_window_start > INTERVAL '24 hours' THEN NOW()
                    ELSE daily_window_start
                END,
                daily_outflow_usd = CASE
                    WHEN NOW() - daily_window_start > INTERVAL '24 hours' THEN $1
                    ELSE daily_outflow_usd + $1
                END,
                last_updated = NOW()
             WHERE id = 'global'",
        )
        .bind(amount_usd)
        .execute(&self.pool)
        .await?;

        // Check if we should trip after update
        let state = self.load_circuit_breaker().await?;
        if state.hourly_outflow_usd > state.hourly_limit_usd
            || state.daily_outflow_usd > state.daily_limit_usd
        {
            self.trip_circuit_breaker("Outflow limit exceeded").await?;
        }

        Ok(())
    }

    /// Trip the circuit breaker.
    pub async fn trip_circuit_breaker(&self, reason: &str) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE circuit_breaker_state SET
                is_tripped = true,
                tripped_at = NOW(),
                trip_reason = $1,
                last_updated = NOW()
             WHERE id = 'global'",
        )
        .bind(reason)
        .execute(&self.pool)
        .await?;

        tracing::error!(reason = reason, "CIRCUIT BREAKER TRIPPED — all withdrawals halted");
        Ok(())
    }

    /// Reset the circuit breaker (manual admin action).
    pub async fn reset_circuit_breaker(&self) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE circuit_breaker_state SET
                is_tripped = false,
                tripped_at = NULL,
                trip_reason = NULL,
                hourly_outflow_usd = 0,
                daily_outflow_usd = 0,
                hourly_window_start = NOW(),
                daily_window_start = NOW(),
                last_updated = NOW()
             WHERE id = 'global'",
        )
        .execute(&self.pool)
        .await?;

        tracing::warn!("Circuit breaker RESET by admin");
        Ok(())
    }

    /// Get current outflow for monitoring.
    pub async fn get_circuit_breaker_status(&self) -> anyhow::Result<CircuitBreakerRow> {
        self.load_circuit_breaker().await
    }
}
