//! Custody statements — monthly account statements for workspace holders.
//!
//! A statement captures opening/closing balances, all deposits/withdrawals,
//! and fee totals for a calendar month. It is signed with a SHA-256 attestation
//! hash so recipients can verify the document has not been altered.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// Calendar month (year + month number 1-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StatementPeriod {
    pub year: i32,
    pub month: u32,
}

impl StatementPeriod {
    /// Construct and validate.
    pub fn new(year: i32, month: u32) -> anyhow::Result<Self> {
        if !(1..=12).contains(&month) {
            anyhow::bail!("month must be 1-12, got {month}");
        }
        Ok(Self { year, month })
    }

    /// First day of the period.
    pub fn start_date(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.year, self.month, 1).unwrap()
    }

    /// First day of the next period (exclusive end).
    pub fn end_date_exclusive(&self) -> NaiveDate {
        if self.month == 12 {
            NaiveDate::from_ymd_opt(self.year + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(self.year, self.month + 1, 1).unwrap()
        }
    }

    /// Human-readable label e.g. "2026-03".
    pub fn label(&self) -> String {
        format!("{}-{:02}", self.year, self.month)
    }

    /// Parse from "YYYY-MM" string.
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        let parts: Vec<&str> = s.splitn(2, '-').collect();
        if parts.len() != 2 {
            anyhow::bail!("StatementPeriod parse error: expected YYYY-MM, got '{s}'");
        }
        let year: i32 = parts[0].parse().map_err(|_| anyhow::anyhow!("invalid year in '{s}'"))?;
        let month: u32 = parts[1].parse().map_err(|_| anyhow::anyhow!("invalid month in '{s}'"))?;
        Self::new(year, month)
    }
}

impl std::fmt::Display for StatementPeriod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Balance for a single asset at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetBalance {
    pub asset: String,
    pub chain: String,
    pub balance: Decimal,
    /// USD equivalent at statement generation time.
    pub balance_usd: Decimal,
}

/// One deposit or withdrawal line in a statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatementEntry {
    pub date: DateTime<Utc>,
    pub asset: String,
    pub chain: String,
    /// Signed amount: positive for deposits, negative for withdrawals.
    pub amount: Decimal,
    /// USD equivalent at transaction time.
    pub amount_usd: Decimal,
    pub tx_hash: Option<String>,
    pub status: String,
    pub fee_usd: Option<Decimal>,
}

/// Per-asset fee summary for the statement period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeSummary {
    pub asset: String,
    pub chain: String,
    pub total_fees_native: Decimal,
    pub total_fees_usd: Decimal,
    pub tx_count: u64,
}

/// Complete monthly custody statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustodyStatement {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub period: StatementPeriod,
    pub generated_at: DateTime<Utc>,
    /// Balances at the start of the period (first second of first day).
    pub opening_balances: Vec<AssetBalance>,
    /// Balances at the end of the period (last second of last day).
    pub closing_balances: Vec<AssetBalance>,
    /// All confirmed deposits in the period.
    pub deposits: Vec<StatementEntry>,
    /// All confirmed withdrawals in the period.
    pub withdrawals: Vec<StatementEntry>,
    /// Fee summary grouped by asset/chain.
    pub fees_summary: Vec<FeeSummary>,
    /// Total assets under management in USD at generation time.
    pub total_aum_usd: Decimal,
    /// SHA-256 of the canonical JSON representation (hex-encoded).
    /// Allows recipients to verify the statement has not been tampered with.
    pub attestation_hash: String,
}

impl CustodyStatement {
    /// Compute the attestation hash from the statement data.
    ///
    /// The hash covers all material fields except `attestation_hash` itself.
    /// JSON serialization uses sorted keys via `serde_json` with deterministic
    /// field ordering (struct field order).
    pub fn compute_attestation_hash(
        id: Uuid,
        workspace_id: Uuid,
        period: &StatementPeriod,
        generated_at: DateTime<Utc>,
        opening_balances: &[AssetBalance],
        closing_balances: &[AssetBalance],
        deposits: &[StatementEntry],
        withdrawals: &[StatementEntry],
        fees_summary: &[FeeSummary],
        total_aum_usd: Decimal,
    ) -> String {
        let payload = serde_json::json!({
            "id": id,
            "workspace_id": workspace_id,
            "period": period.label(),
            "generated_at": generated_at.to_rfc3339(),
            "opening_balances": opening_balances,
            "closing_balances": closing_balances,
            "deposits": deposits,
            "withdrawals": withdrawals,
            "fees_summary": fees_summary,
            "total_aum_usd": total_aum_usd.to_string(),
        });
        let canonical = payload.to_string();
        hex::encode(Sha256::digest(canonical.as_bytes()))
    }

    /// Verify the statement's attestation hash matches recomputed value.
    pub fn verify_attestation(&self) -> bool {
        let expected = Self::compute_attestation_hash(
            self.id,
            self.workspace_id,
            &self.period,
            self.generated_at,
            &self.opening_balances,
            &self.closing_balances,
            &self.deposits,
            &self.withdrawals,
            &self.fees_summary,
            self.total_aum_usd,
        );
        self.attestation_hash == expected
    }

    /// Total deposit volume in USD for the period.
    pub fn total_inflow_usd(&self) -> Decimal {
        self.deposits.iter().map(|e| e.amount_usd).sum()
    }

    /// Total withdrawal volume in USD for the period.
    pub fn total_outflow_usd(&self) -> Decimal {
        self.withdrawals.iter().map(|e| e.amount_usd).sum()
    }

    /// Net flow (deposits - withdrawals) in USD.
    pub fn net_flow_usd(&self) -> Decimal {
        self.total_inflow_usd() - self.total_outflow_usd()
    }

    /// Total fees paid across all assets in USD.
    pub fn total_fees_usd(&self) -> Decimal {
        self.fees_summary.iter().map(|f| f.total_fees_usd).sum()
    }
}

// ---------------------------------------------------------------------------
// Export functions
// ---------------------------------------------------------------------------

/// Export statement as indented JSON string.
pub fn export_statement_json(statement: &CustodyStatement) -> anyhow::Result<String> {
    serde_json::to_string_pretty(statement).map_err(|e| anyhow::anyhow!("JSON export failed: {e}"))
}

/// Export statement as UTF-8 plain text.
pub fn export_statement_text(statement: &CustodyStatement) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "CUSTODY STATEMENT — {}\n",
        statement.period.label()
    ));
    out.push_str(&format!(
        "Workspace: {}\n",
        statement.workspace_id
    ));
    out.push_str(&format!(
        "Generated: {}\n",
        statement.generated_at.format("%Y-%m-%d %H:%M:%S UTC")
    ));
    out.push_str(&format!(
        "Attestation: {}\n",
        statement.attestation_hash
    ));
    out.push_str("\n--- OPENING BALANCES ---\n");
    for b in &statement.opening_balances {
        out.push_str(&format!(
            "  {} ({}) : {} (${} USD)\n",
            b.asset, b.chain, b.balance, b.balance_usd
        ));
    }
    out.push_str("\n--- CLOSING BALANCES ---\n");
    for b in &statement.closing_balances {
        out.push_str(&format!(
            "  {} ({}) : {} (${} USD)\n",
            b.asset, b.chain, b.balance, b.balance_usd
        ));
    }
    out.push_str("\n--- DEPOSITS ---\n");
    for d in &statement.deposits {
        out.push_str(&format!(
            "  {} {} {} {} {}\n",
            d.date.format("%Y-%m-%d"),
            d.asset,
            d.chain,
            d.amount,
            d.status
        ));
    }
    out.push_str("\n--- WITHDRAWALS ---\n");
    for w in &statement.withdrawals {
        out.push_str(&format!(
            "  {} {} {} {} {}\n",
            w.date.format("%Y-%m-%d"),
            w.asset,
            w.chain,
            w.amount,
            w.status
        ));
    }
    out.push_str("\n--- FEES SUMMARY ---\n");
    for f in &statement.fees_summary {
        out.push_str(&format!(
            "  {} ({}) : {} native, ${} USD across {} txs\n",
            f.asset, f.chain, f.total_fees_native, f.total_fees_usd, f.tx_count
        ));
    }
    out.push_str(&format!("\nTotal AUM: ${} USD\n", statement.total_aum_usd));
    out.push_str(&format!("Total Inflow: ${} USD\n", statement.total_inflow_usd()));
    out.push_str(&format!("Total Outflow: ${} USD\n", statement.total_outflow_usd()));
    out.push_str(&format!("Net Flow: ${} USD\n", statement.net_flow_usd()));
    out.push_str(&format!("Total Fees: ${} USD\n", statement.total_fees_usd()));
    out
}

/// Export statement as CSV (deposits and withdrawals in one file).
pub fn export_statement_csv(statement: &CustodyStatement) -> String {
    let mut out = String::new();
    out.push_str("type,date,asset,chain,amount,amount_usd,fee_usd,tx_hash,status\n");
    for d in &statement.deposits {
        out.push_str(&format!(
            "deposit,{},{},{},{},{},{},{},{}\n",
            d.date.format("%Y-%m-%dT%H:%M:%SZ"),
            d.asset,
            d.chain,
            d.amount,
            d.amount_usd,
            d.fee_usd.unwrap_or(Decimal::ZERO),
            d.tx_hash.as_deref().unwrap_or(""),
            d.status,
        ));
    }
    for w in &statement.withdrawals {
        out.push_str(&format!(
            "withdrawal,{},{},{},{},{},{},{},{}\n",
            w.date.format("%Y-%m-%dT%H:%M:%SZ"),
            w.asset,
            w.chain,
            w.amount,
            w.amount_usd,
            w.fee_usd.unwrap_or(Decimal::ZERO),
            w.tx_hash.as_deref().unwrap_or(""),
            w.status,
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn sample_statement() -> CustodyStatement {
        let id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let period = StatementPeriod::new(2026, 3).unwrap();
        let generated_at = Utc::now();

        let opening = vec![
            AssetBalance { asset: "ETH".into(), chain: "ethereum".into(), balance: dec!(10), balance_usd: dec!(30000) },
            AssetBalance { asset: "USDT".into(), chain: "tron".into(), balance: dec!(50000), balance_usd: dec!(50000) },
        ];
        let closing = vec![
            AssetBalance { asset: "ETH".into(), chain: "ethereum".into(), balance: dec!(12), balance_usd: dec!(36000) },
            AssetBalance { asset: "USDT".into(), chain: "tron".into(), balance: dec!(45000), balance_usd: dec!(45000) },
        ];
        let deposits = vec![
            StatementEntry {
                date: Utc::now(), asset: "ETH".into(), chain: "ethereum".into(),
                amount: dec!(2), amount_usd: dec!(6000),
                tx_hash: Some("0xabc".into()), status: "confirmed".into(), fee_usd: Some(dec!(3)),
            },
        ];
        let withdrawals = vec![
            StatementEntry {
                date: Utc::now(), asset: "USDT".into(), chain: "tron".into(),
                amount: dec!(5000), amount_usd: dec!(5000),
                tx_hash: Some("Txhash123".into()), status: "confirmed".into(), fee_usd: Some(dec!(1)),
            },
        ];
        let fees_summary = vec![
            FeeSummary {
                asset: "ETH".into(), chain: "ethereum".into(),
                total_fees_native: dec!(0.001), total_fees_usd: dec!(3), tx_count: 1,
            },
            FeeSummary {
                asset: "TRX".into(), chain: "tron".into(),
                total_fees_native: dec!(10), total_fees_usd: dec!(1), tx_count: 1,
            },
        ];
        let total_aum_usd = dec!(81000);

        let attestation_hash = CustodyStatement::compute_attestation_hash(
            id, workspace_id, &period, generated_at,
            &opening, &closing, &deposits, &withdrawals, &fees_summary, total_aum_usd,
        );

        CustodyStatement {
            id, workspace_id, period, generated_at,
            opening_balances: opening,
            closing_balances: closing,
            deposits,
            withdrawals,
            fees_summary,
            total_aum_usd,
            attestation_hash,
        }
    }

    // -- StatementPeriod --

    #[test]
    fn period_label() {
        assert_eq!(StatementPeriod::new(2026, 3).unwrap().label(), "2026-03");
        assert_eq!(StatementPeriod::new(2026, 12).unwrap().label(), "2026-12");
    }

    #[test]
    fn period_parse_valid() {
        let p = StatementPeriod::parse("2026-03").unwrap();
        assert_eq!(p.year, 2026);
        assert_eq!(p.month, 3);
    }

    #[test]
    fn period_parse_invalid_format() {
        assert!(StatementPeriod::parse("2026").is_err());
        assert!(StatementPeriod::parse("not-a-date").is_err());
    }

    #[test]
    fn period_invalid_month_rejected() {
        assert!(StatementPeriod::new(2026, 0).is_err());
        assert!(StatementPeriod::new(2026, 13).is_err());
    }

    #[test]
    fn period_end_date_march() {
        let p = StatementPeriod::new(2026, 3).unwrap();
        assert_eq!(p.start_date(), NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());
        assert_eq!(p.end_date_exclusive(), NaiveDate::from_ymd_opt(2026, 4, 1).unwrap());
    }

    #[test]
    fn period_end_date_december_wraps_year() {
        let p = StatementPeriod::new(2026, 12).unwrap();
        assert_eq!(p.end_date_exclusive(), NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
    }

    // -- Statement generation --

    #[test]
    fn statement_attestation_hash_is_64_hex_chars() {
        let s = sample_statement();
        assert_eq!(s.attestation_hash.len(), 64);
        assert!(s.attestation_hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn statement_attestation_verifies() {
        let s = sample_statement();
        assert!(s.verify_attestation());
    }

    #[test]
    fn statement_tampered_hash_fails_verification() {
        let mut s = sample_statement();
        s.total_aum_usd = dec!(99999999); // tamper
        // attestation_hash is now stale
        assert!(!s.verify_attestation());
    }

    #[test]
    fn statement_flow_calculations() {
        let s = sample_statement();
        assert_eq!(s.total_inflow_usd(), dec!(6000));
        assert_eq!(s.total_outflow_usd(), dec!(5000));
        assert_eq!(s.net_flow_usd(), dec!(1000));
        assert_eq!(s.total_fees_usd(), dec!(4)); // 3 + 1
    }

    // -- Export formats --

    #[test]
    fn export_json_is_valid_json() {
        let s = sample_statement();
        let json_str = export_statement_json(&s).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed.get("period").is_some());
        assert!(parsed.get("attestation_hash").is_some());
    }

    #[test]
    fn export_text_contains_key_fields() {
        let s = sample_statement();
        let text = export_statement_text(&s);
        assert!(text.contains("CUSTODY STATEMENT"));
        assert!(text.contains("2026-03"));
        assert!(text.contains("DEPOSITS"));
        assert!(text.contains("WITHDRAWALS"));
        assert!(text.contains("FEES SUMMARY"));
        assert!(text.contains("ETH"));
    }

    #[test]
    fn export_csv_header_and_rows() {
        let s = sample_statement();
        let csv = export_statement_csv(&s);
        assert!(csv.starts_with("type,date,asset,chain,amount"));
        assert!(csv.contains("deposit,"));
        assert!(csv.contains("withdrawal,"));
        // One deposit + one withdrawal = 2 data rows
        let rows: Vec<&str> = csv.lines().filter(|l| !l.starts_with("type")).collect();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn export_csv_amounts_are_not_floats() {
        // Amounts must be Decimal strings, not scientific notation.
        let s = sample_statement();
        let csv = export_statement_csv(&s);
        // Scientific notation would contain 'e' or 'E' in amounts.
        // Decimal::to_string never does this.
        assert!(!csv.contains("6e3"), "CSV must not use scientific notation");
    }

    #[test]
    fn deterministic_attestation_hash() {
        // Same inputs produce the same hash.
        let s = sample_statement();
        let hash2 = CustodyStatement::compute_attestation_hash(
            s.id, s.workspace_id, &s.period, s.generated_at,
            &s.opening_balances, &s.closing_balances,
            &s.deposits, &s.withdrawals, &s.fees_summary, s.total_aum_usd,
        );
        assert_eq!(s.attestation_hash, hash2);
    }
}
