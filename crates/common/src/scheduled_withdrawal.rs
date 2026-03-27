//! Scheduled and recurring withdrawal definitions.
//!
//! Supports one-shot, daily, weekly, and monthly schedules.
//! All amounts use `rust_decimal::Decimal` — never f64.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

// ---------------------------------------------------------------------------
// Schedule types
// ---------------------------------------------------------------------------

/// When a withdrawal should fire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WithdrawalSchedule {
    /// Execute once at the given UTC timestamp.
    Once {
        at: DateTime<Utc>,
    },
    /// Execute every day at `hour_utc` (0–23).
    Daily {
        hour_utc: u8,
    },
    /// Execute every week on `day` at `hour_utc`.
    Weekly {
        day: Weekday,
        hour_utc: u8,
    },
    /// Execute on `day_of_month` (1–28) at `hour_utc`.
    /// Days 29–31 are clamped to 28 for reliability across all months.
    Monthly {
        day_of_month: u8,
        hour_utc: u8,
    },
}

impl WithdrawalSchedule {
    /// Compute the next execution time strictly after `after`.
    ///
    /// Returns `None` for `Once` when `after` is already past the scheduled time
    /// (the event has already occurred or been consumed).
    pub fn next_occurrence(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            WithdrawalSchedule::Once { at } => {
                if *at > after { Some(*at) } else { None }
            }

            WithdrawalSchedule::Daily { hour_utc } => {
                let h = *hour_utc as u32;
                // Candidate: today at hour_utc
                let candidate = after
                    .date_naive()
                    .and_hms_opt(h, 0, 0)
                    .map(|naive| Utc.from_utc_datetime(&naive))?;

                if candidate > after {
                    Some(candidate)
                } else {
                    // Tomorrow
                    let tomorrow = (after + Duration::days(1))
                        .date_naive()
                        .and_hms_opt(h, 0, 0)
                        .map(|naive| Utc.from_utc_datetime(&naive))?;
                    Some(tomorrow)
                }
            }

            WithdrawalSchedule::Weekly { day, hour_utc } => {
                let target_day = *day;
                let h = *hour_utc as u32;
                // Scan up to 8 days forward to find the next matching weekday
                let mut cursor = after;
                for _ in 0..8 {
                    cursor += Duration::days(1);
                    if cursor.weekday() == target_day {
                        let candidate = cursor
                            .date_naive()
                            .and_hms_opt(h, 0, 0)
                            .map(|naive| Utc.from_utc_datetime(&naive))?;
                        return Some(candidate);
                    }
                }
                None // unreachable, but safe fallback
            }

            WithdrawalSchedule::Monthly { day_of_month, hour_utc } => {
                let dom = (*day_of_month).min(28) as u32; // clamp for safety
                let h = *hour_utc as u32;

                // Try this month first
                let candidate_date = after.date_naive().with_day(dom)?;
                let candidate = candidate_date
                    .and_hms_opt(h, 0, 0)
                    .map(|naive| Utc.from_utc_datetime(&naive))?;

                if candidate > after {
                    return Some(candidate);
                }

                // Try next month
                let next_month = if after.month() == 12 {
                    NaiveDate::from_ymd_opt(after.year() + 1, 1, dom)?
                } else {
                    NaiveDate::from_ymd_opt(after.year(), after.month() + 1, dom)?
                };
                let next_dt = next_month
                    .and_hms_opt(h, 0, 0)
                    .map(|naive| Utc.from_utc_datetime(&naive))?;
                Some(next_dt)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ScheduledWithdrawal
// ---------------------------------------------------------------------------

/// A recurring or one-shot withdrawal definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledWithdrawal {
    pub id: Uuid,
    pub user_id: Uuid,
    pub chain: Chain,
    pub asset: String,
    /// Amount per execution. Must be positive. Never f64.
    pub amount: Decimal,
    /// Validated destination address.
    pub destination: String,
    pub schedule: WithdrawalSchedule,
    /// Computed next execution time.
    pub next_execution: DateTime<Utc>,
    pub last_executed: Option<DateTime<Utc>>,
    /// Total executions completed.
    pub executions_count: u32,
    /// `None` means unlimited.
    pub max_executions: Option<u32>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

impl ScheduledWithdrawal {
    /// Create a new scheduled withdrawal. The first `next_execution` is
    /// computed from the schedule starting from `created_at`.
    pub fn new(
        user_id: Uuid,
        chain: Chain,
        asset: String,
        amount: Decimal,
        destination: String,
        schedule: WithdrawalSchedule,
    ) -> Option<Self> {
        let now = Utc::now();
        let next = schedule.next_occurrence(now)?;
        Some(Self {
            id: Uuid::new_v4(),
            user_id,
            chain,
            asset,
            amount,
            destination,
            schedule,
            next_execution: next,
            last_executed: None,
            executions_count: 0,
            max_executions: None,
            enabled: true,
            created_at: now,
        })
    }

    /// Returns `true` if this schedule should fire at `now`.
    /// Conditions: enabled, `now >= next_execution`, and max_executions not
    /// exhausted.
    pub fn should_execute(&self, now: DateTime<Utc>) -> bool {
        if !self.enabled {
            return false;
        }
        if now < self.next_execution {
            return false;
        }
        if self.max_executions.is_some_and(|max| self.executions_count >= max) {
            return false;
        }
        true
    }

    /// Advance the schedule after a successful execution.
    ///
    /// Updates:
    /// - `last_executed` to `now`
    /// - `executions_count` incremented
    /// - `next_execution` to the next occurrence after `now` (or disables if `Once`)
    /// - `enabled` set to `false` if max_executions reached or `Once` schedule consumed
    ///
    /// Returns `true` if the schedule is still active after advancement.
    pub fn advance(&mut self, now: DateTime<Utc>) -> bool {
        self.last_executed = Some(now);
        self.executions_count = self.executions_count.saturating_add(1);

        // Check if exhausted
        if self.max_executions.is_some_and(|max| self.executions_count >= max) {
            self.enabled = false;
            return false;
        }

        // Compute next occurrence
        match self.schedule.next_occurrence(now) {
            Some(next) => {
                self.next_execution = next;
                true
            }
            None => {
                // `Once` schedule — no more occurrences
                self.enabled = false;
                false
            }
        }
    }
}

// ---------------------------------------------------------------------------
// API request/response types
// ---------------------------------------------------------------------------

/// Request to create a scheduled withdrawal.
#[derive(Debug, Deserialize)]
pub struct CreateScheduledWithdrawalRequest {
    pub chain: Chain,
    pub asset: String,
    /// String-encoded Decimal — never float in JSON.
    pub amount: Decimal,
    pub destination: String,
    pub schedule: WithdrawalSchedule,
    pub max_executions: Option<u32>,
}

/// Response for a scheduled withdrawal.
#[derive(Debug, Serialize)]
pub struct ScheduledWithdrawalResponse {
    pub id: String,
    pub chain: String,
    pub asset: String,
    pub amount: String,
    pub destination: String,
    pub schedule_type: String,
    pub next_execution: String,
    pub last_executed: Option<String>,
    pub executions_count: u32,
    pub max_executions: Option<u32>,
    pub enabled: bool,
    pub created_at: String,
}

impl From<ScheduledWithdrawal> for ScheduledWithdrawalResponse {
    fn from(s: ScheduledWithdrawal) -> Self {
        let schedule_type = match &s.schedule {
            WithdrawalSchedule::Once { .. } => "once",
            WithdrawalSchedule::Daily { .. } => "daily",
            WithdrawalSchedule::Weekly { .. } => "weekly",
            WithdrawalSchedule::Monthly { .. } => "monthly",
        };
        ScheduledWithdrawalResponse {
            id: s.id.to_string(),
            chain: s.chain.as_str().to_string(),
            asset: s.asset,
            amount: s.amount.to_string(),
            destination: s.destination,
            schedule_type: schedule_type.to_string(),
            next_execution: s.next_execution.to_rfc3339(),
            last_executed: s.last_executed.map(|t| t.to_rfc3339()),
            executions_count: s.executions_count,
            max_executions: s.max_executions,
            enabled: s.enabled,
            created_at: s.created_at.to_rfc3339(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use rust_decimal_macros::dec;

    fn ts(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap()
    }

    // --- Once schedule ---

    #[test]
    fn once_returns_future_time() {
        let schedule = WithdrawalSchedule::Once { at: ts(2027, 1, 15, 10) };
        let after = ts(2027, 1, 14, 10);
        assert_eq!(schedule.next_occurrence(after), Some(ts(2027, 1, 15, 10)));
    }

    #[test]
    fn once_returns_none_after_event() {
        let schedule = WithdrawalSchedule::Once { at: ts(2026, 1, 1, 0) };
        let after = ts(2026, 1, 2, 0); // after the event
        assert_eq!(schedule.next_occurrence(after), None);
    }

    // --- Daily schedule ---

    #[test]
    fn daily_returns_today_if_not_yet_fired() {
        // 09:00 UTC — ask for next after 08:00
        let schedule = WithdrawalSchedule::Daily { hour_utc: 9 };
        let after = ts(2026, 6, 15, 8);
        assert_eq!(schedule.next_occurrence(after), Some(ts(2026, 6, 15, 9)));
    }

    #[test]
    fn daily_returns_tomorrow_if_past_today() {
        // 09:00 UTC — ask for next after 10:00
        let schedule = WithdrawalSchedule::Daily { hour_utc: 9 };
        let after = ts(2026, 6, 15, 10);
        assert_eq!(schedule.next_occurrence(after), Some(ts(2026, 6, 16, 9)));
    }

    #[test]
    fn daily_crosses_month_boundary() {
        let schedule = WithdrawalSchedule::Daily { hour_utc: 0 };
        let after = ts(2026, 3, 31, 1); // after midnight March 31
        assert_eq!(schedule.next_occurrence(after), Some(ts(2026, 4, 1, 0)));
    }

    // --- Weekly schedule ---

    #[test]
    fn weekly_next_monday_from_tuesday() {
        // Current: Wednesday 2026-06-17, want: next Monday
        let schedule = WithdrawalSchedule::Weekly { day: Weekday::Mon, hour_utc: 10 };
        let after = ts(2026, 6, 17, 12); // Wednesday
        let next = schedule.next_occurrence(after).unwrap();
        assert_eq!(next.weekday(), Weekday::Mon);
        assert!(next > after);
    }

    #[test]
    fn weekly_same_day_next_week_if_past_hour() {
        // Monday 10:00 already past, next Monday should be 7 days later
        let schedule = WithdrawalSchedule::Weekly { day: Weekday::Mon, hour_utc: 9 };
        let after = ts(2026, 6, 15, 10); // Monday 10:00 UTC (June 15 2026 is Mon)
        let next = schedule.next_occurrence(after).unwrap();
        assert_eq!(next.weekday(), Weekday::Mon);
        assert!(next > after + Duration::days(6));
    }

    // --- Monthly schedule ---

    #[test]
    fn monthly_this_month_if_dom_not_yet() {
        let schedule = WithdrawalSchedule::Monthly { day_of_month: 20, hour_utc: 12 };
        let after = ts(2026, 6, 10, 0);
        assert_eq!(schedule.next_occurrence(after), Some(ts(2026, 6, 20, 12)));
    }

    #[test]
    fn monthly_next_month_if_dom_passed() {
        let schedule = WithdrawalSchedule::Monthly { day_of_month: 5, hour_utc: 12 };
        let after = ts(2026, 6, 10, 0); // day 5 has passed
        assert_eq!(schedule.next_occurrence(after), Some(ts(2026, 7, 5, 12)));
    }

    #[test]
    fn monthly_clamps_day_29_to_28() {
        // day_of_month=29 → clamped to 28
        let schedule = WithdrawalSchedule::Monthly { day_of_month: 29, hour_utc: 0 };
        let after = ts(2026, 2, 1, 0); // February — no day 29 in non-leap year
        let next = schedule.next_occurrence(after).unwrap();
        assert!(next.day() <= 28);
    }

    // --- should_execute ---

    #[test]
    fn should_execute_true_when_due() {
        let mut sw = ScheduledWithdrawal {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            chain: Chain::Ethereum,
            asset: "USDT".into(),
            amount: dec!(100),
            destination: "0x1234".into(),
            schedule: WithdrawalSchedule::Daily { hour_utc: 9 },
            next_execution: ts(2026, 6, 15, 9),
            last_executed: None,
            executions_count: 0,
            max_executions: None,
            enabled: true,
            created_at: Utc::now(),
        };
        assert!(sw.should_execute(ts(2026, 6, 15, 9)));
        assert!(sw.should_execute(ts(2026, 6, 15, 10)));

        // Disabled — should NOT execute
        sw.enabled = false;
        assert!(!sw.should_execute(ts(2026, 6, 15, 9)));
    }

    #[test]
    fn should_execute_false_when_max_reached() {
        let sw = ScheduledWithdrawal {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            chain: Chain::Ethereum,
            asset: "ETH".into(),
            amount: dec!(0.1),
            destination: "0x1234".into(),
            schedule: WithdrawalSchedule::Daily { hour_utc: 0 },
            next_execution: ts(2026, 1, 1, 0),
            last_executed: None,
            executions_count: 5,
            max_executions: Some(5), // already exhausted
            enabled: true,
            created_at: Utc::now(),
        };
        assert!(!sw.should_execute(ts(2026, 1, 1, 1)));
    }

    // --- advance ---

    #[test]
    fn advance_increments_count_and_next() {
        let mut sw = ScheduledWithdrawal {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            chain: Chain::Ethereum,
            asset: "USDT".into(),
            amount: dec!(50),
            destination: "0x1234".into(),
            schedule: WithdrawalSchedule::Daily { hour_utc: 9 },
            next_execution: ts(2026, 6, 15, 9),
            last_executed: None,
            executions_count: 0,
            max_executions: None,
            enabled: true,
            created_at: Utc::now(),
        };
        let still_active = sw.advance(ts(2026, 6, 15, 9));
        assert!(still_active);
        assert_eq!(sw.executions_count, 1);
        assert!(sw.next_execution > ts(2026, 6, 15, 9));
    }

    #[test]
    fn advance_disables_once_schedule() {
        let schedule_time = ts(2026, 6, 15, 9);
        let mut sw = ScheduledWithdrawal {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            chain: Chain::Ethereum,
            asset: "BTC".into(),
            amount: dec!(0.01),
            destination: "bc1qtest".into(),
            schedule: WithdrawalSchedule::Once { at: schedule_time },
            next_execution: schedule_time,
            last_executed: None,
            executions_count: 0,
            max_executions: None,
            enabled: true,
            created_at: Utc::now(),
        };
        let still_active = sw.advance(schedule_time);
        assert!(!still_active, "Once schedule should be disabled after firing");
        assert!(!sw.enabled);
    }

    #[test]
    fn advance_disables_when_max_executions_reached() {
        let mut sw = ScheduledWithdrawal {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            chain: Chain::Ethereum,
            asset: "ETH".into(),
            amount: dec!(1),
            destination: "0x5678".into(),
            schedule: WithdrawalSchedule::Daily { hour_utc: 0 },
            next_execution: ts(2026, 6, 15, 0),
            last_executed: None,
            executions_count: 4,
            max_executions: Some(5),
            enabled: true,
            created_at: Utc::now(),
        };
        let still_active = sw.advance(ts(2026, 6, 15, 0));
        assert!(!still_active, "Should be disabled after reaching max_executions=5");
        assert_eq!(sw.executions_count, 5);
        assert!(!sw.enabled);
    }

    #[test]
    fn amount_is_decimal_not_float() {
        // Verify no f64 path
        let amount: Decimal = dec!(123456789.123456789);
        let s = amount.to_string();
        // Exact decimal representation, no floating-point rounding
        assert_eq!(s, "123456789.123456789");
    }

    #[test]
    fn response_from_scheduled_withdrawal() {
        let sw = ScheduledWithdrawal {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            chain: Chain::Solana,
            asset: "USDC".into(),
            amount: dec!(500),
            destination: "GkosV9test".into(),
            schedule: WithdrawalSchedule::Weekly { day: Weekday::Fri, hour_utc: 14 },
            next_execution: ts(2026, 6, 19, 14),
            last_executed: None,
            executions_count: 0,
            max_executions: Some(4),
            enabled: true,
            created_at: Utc::now(),
        };
        let resp: ScheduledWithdrawalResponse = sw.into();
        assert_eq!(resp.schedule_type, "weekly");
        assert_eq!(resp.amount, "500");
        assert!(resp.enabled);
        assert_eq!(resp.max_executions, Some(4));
    }
}
