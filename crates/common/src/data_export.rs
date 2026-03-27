//! Data export — structured exports for auditors, tax, and compliance.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::Chain;

/// Tax report for a fiscal year.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxReport {
    pub year: u32,
    pub jurisdiction: String,
    pub total_deposits_usd: Decimal,
    pub total_withdrawals_usd: Decimal,
    pub total_fees_usd: Decimal,
    pub realized_gains_usd: Decimal,
    pub unrealized_gains_usd: Decimal,
    pub cost_basis_method: CostBasisMethod,
    pub entries: Vec<TaxEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostBasisMethod { Fifo, Lifo, SpecificId, AverageCost }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxEntry {
    pub date: DateTime<Utc>,
    pub tx_type: String,
    pub asset: String,
    pub amount: Decimal,
    pub cost_basis_usd: Decimal,
    pub proceeds_usd: Decimal,
    pub gain_loss_usd: Decimal,
    pub holding_period: HoldingPeriod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldingPeriod { ShortTerm, LongTerm }

/// Audit trail export — complete history for external auditors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditExport {
    pub generated_at: DateTime<Utc>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_transactions: u64,
    pub total_volume_usd: Decimal,
    pub chains_covered: Vec<Chain>,
    pub export_hash: String,
    pub format: ExportFileFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFileFormat { CsvZip, JsonLines, Parquet, Excel }

/// Address ownership proof for compliance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressOwnershipProof {
    pub address: String,
    pub chain: Chain,
    pub message: String,
    pub signature: String,
    pub public_key: String,
    pub verified: bool,
    pub generated_at: DateTime<Utc>,
}

/// Generate CSV header for transaction export.
pub fn tx_export_csv_header() -> &'static str {
    "id,date,type,chain,asset,amount,fee,from,to,tx_hash,status,vault_id,labels"
}

/// Generate CSV row for a transaction.
pub fn tx_export_csv_row(
    id: &Uuid, date: &DateTime<Utc>, tx_type: &str, chain: &str,
    asset: &str, amount: &Decimal, fee: Option<&Decimal>,
    from: &str, to: &str, tx_hash: &str, status: &str,
    vault_id: &str, labels: &str,
) -> String {
    format!(
        "{},{},{},{},{},{},{},{},{},{},{},{},{}",
        id, date.to_rfc3339(), tx_type, chain, asset, amount,
        fee.map(|f| f.to_string()).unwrap_or_default(),
        from, to, tx_hash, status, vault_id, labels,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn cost_basis_methods() {
        assert_eq!(serde_json::to_string(&CostBasisMethod::Fifo).unwrap(), r#""fifo""#);
        assert_eq!(serde_json::to_string(&CostBasisMethod::SpecificId).unwrap(), r#""specific_id""#);
    }

    #[test]
    fn holding_period() {
        assert_eq!(serde_json::to_string(&HoldingPeriod::LongTerm).unwrap(), r#""long_term""#);
    }

    #[test]
    fn csv_header() {
        let header = tx_export_csv_header();
        assert!(header.contains("id,date,type"));
        assert!(header.contains("labels"));
    }

    #[test]
    fn csv_row_generation() {
        let row = tx_export_csv_row(
            &Uuid::nil(), &Utc::now(), "withdrawal", "ethereum",
            "ETH", &dec!(1.5), Some(&dec!(0.003)),
            "0xfrom", "0xto", "0xhash", "confirmed",
            "vault-1", "payroll",
        );
        assert!(row.contains("withdrawal"));
        assert!(row.contains("1.5"));
    }

    #[test]
    fn export_formats() {
        assert_eq!(serde_json::to_string(&ExportFileFormat::Parquet).unwrap(), r#""parquet""#);
        assert_eq!(serde_json::to_string(&ExportFileFormat::JsonLines).unwrap(), r#""json_lines""#);
    }

    #[test]
    fn tax_report_structure() {
        let report = TaxReport {
            year: 2026, jurisdiction: "US".into(),
            total_deposits_usd: dec!(1000000), total_withdrawals_usd: dec!(800000),
            total_fees_usd: dec!(5000), realized_gains_usd: dec!(50000),
            unrealized_gains_usd: dec!(100000),
            cost_basis_method: CostBasisMethod::Fifo,
            entries: vec![],
        };
        assert_eq!(report.year, 2026);
    }

    #[test]
    fn address_ownership_proof() {
        let proof = AddressOwnershipProof {
            address: "0x123".into(), chain: Chain::Ethereum,
            message: "I own this address".into(),
            signature: "0xsig".into(), public_key: "0xpk".into(),
            verified: true, generated_at: Utc::now(),
        };
        assert!(proof.verified);
    }
}
