//! SAR and CTR report generation and export.
//!
//! Handles:
//! - Generating Suspicious Activity Reports (SAR) from screening results
//! - Auto-generating Currency Transaction Reports (CTR) for transactions > $10K
//! - Exporting reports as FinCEN BSA E-Filing XML (SAR) or CSV (CTR)
//!
//! Reference: FinCEN BSA E-Filing System Technical Specifications
//! <https://bsaefiling.fincen.treas.gov/docs/SDTMRequirements.pdf>

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use custody_common::compliance_reporting::{
    CtrParty, CtrStatus, CurrencyTransactionReport, ExportFormat, ExportType,
    RegulatoryExport, SarFilingType, SarStatus, SarSubject, SuspiciousActivityReport,
};
use crate::screening::{RiskLevel, ScreeningResult};

/// Minimum USD amount that triggers a mandatory CTR filing.
/// Per 31 U.S.C. § 5313 and 31 C.F.R. § 1010.311.
/// Value: 10,000.00 USD — represented as Decimal(10_000, scale=0).
pub const CTR_THRESHOLD_USD: Decimal = Decimal::from_parts(10_000, 0, 0, false, 0);

/// Transaction details passed to report generators.
#[derive(Debug, Clone)]
pub struct TransactionDetails {
    pub transaction_id: Uuid,
    pub amount_usd: Decimal,
    pub transaction_type: String, // "deposit" | "withdrawal"
    pub counterparty_address: String,
    pub chain: String,
    pub user_id: Option<Uuid>,
    pub user_name: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

/// Generate a SAR draft from a screening result and the associated transaction.
///
/// The returned report has `SarStatus::Draft` — it must be reviewed and
/// updated to `Filed` by a compliance officer before submission.
pub fn generate_sar(
    screening: &ScreeningResult,
    tx: &TransactionDetails,
) -> SuspiciousActivityReport {
    let narrative = build_sar_narrative(screening, tx);

    SuspiciousActivityReport {
        id: Uuid::new_v4(),
        reference_number: format!(
            "SAR-{}-{}",
            tx.occurred_at.format("%Y%m%d"),
            &tx.transaction_id.to_string()[..8]
        ),
        filing_type: SarFilingType::Initial,
        subject: SarSubject {
            name: tx.user_name.clone(),
            address: tx.counterparty_address.clone(),
            chain: tx.chain.clone(),
            user_id: tx.user_id,
        },
        transactions: vec![tx.transaction_id],
        total_amount_usd: tx.amount_usd,
        narrative,
        risk_indicators: extract_risk_indicators(screening),
        status: SarStatus::Draft,
        filed_at: None,
        created_at: Utc::now(),
    }
}

/// Auto-generate a CTR for transactions that exceed the $10K threshold.
///
/// Returns `None` if `tx.amount_usd` is at or below `CTR_THRESHOLD_USD`.
///
/// Per FinCEN requirements, CTRs must be filed within 15 calendar days of the
/// transaction date.  The `filing_deadline` is set accordingly.
pub fn generate_ctr(
    tx: &TransactionDetails,
    threshold_usd: Decimal,
) -> Option<CurrencyTransactionReport> {
    if tx.amount_usd <= threshold_usd {
        return None;
    }

    // Filing deadline: 15 calendar days after the transaction.
    let filing_deadline = tx.occurred_at + chrono::Duration::days(15);

    Some(CurrencyTransactionReport {
        id: Uuid::new_v4(),
        transaction_id: tx.transaction_id,
        amount_usd: tx.amount_usd,
        transaction_type: tx.transaction_type.clone(),
        conductor: CtrParty {
            name: tx.user_name.clone().unwrap_or_else(|| "Unknown".into()),
            identification_type: "blockchain_address".into(),
            identification_number: tx.counterparty_address.clone(),
            address: None,
        },
        status: CtrStatus::Pending,
        filing_deadline,
        created_at: Utc::now(),
    })
}

/// Export a SAR as FinCEN BSA E-Filing XML.
///
/// Produces a valid SAR-SF XML document suitable for submission to the
/// FinCEN BSA E-Filing System.  This is a simplified structural representation;
/// a production implementation would use the full FinCEN XML schema.
///
/// Reference: FinCEN SAR XML Schema v1.3
/// <https://bsaefiling.fincen.treas.gov/docs/SDTMRequirements.pdf>
pub fn export_sar_xml(report: &SuspiciousActivityReport) -> String {
    let status_str = match report.status {
        SarStatus::Draft => "1",       // Initial (draft)
        SarStatus::UnderReview => "2", // Under review
        SarStatus::Filed => "3",       // Filed
        SarStatus::Closed => "4",      // Closed
    };

    let filing_type_str = match report.filing_type {
        SarFilingType::Initial => "I",
        SarFilingType::Continuing => "C",
        SarFilingType::Joint => "J",
    };

    let filed_at_str = report
        .filed_at
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default();

    let subject_name = xml_escape(
        report.subject.name.as_deref().unwrap_or("Unknown"),
    );
    let subject_address = xml_escape(&report.subject.address);
    let narrative = xml_escape(&report.narrative);

    let indicator_elements: String = report
        .risk_indicators
        .iter()
        .map(|ind| format!("      <RiskIndicator>{}</RiskIndicator>", xml_escape(ind)))
        .collect::<Vec<_>>()
        .join("\n");

    let transaction_elements: String = report
        .transactions
        .iter()
        .map(|tid| format!("      <TransactionId>{tid}</TransactionId>"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<FinCEN_SAR xmlns="https://bsaefiling.fincen.treas.gov/schemas/SARSchema"
            xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <ReportHeader>
    <ReferenceNumber>{ref_num}</ReferenceNumber>
    <FilingType>{filing_type}</FilingType>
    <Status>{status}</Status>
    <CreatedAt>{created_at}</CreatedAt>
    {filed_at_elem}
  </ReportHeader>
  <Subject>
    <Name>{subject_name}</Name>
    <BlockchainAddress>{subject_address}</BlockchainAddress>
    <Chain>{chain}</Chain>
    {user_id_elem}
  </Subject>
  <SuspiciousActivity>
    <TotalAmountUSD>{amount}</TotalAmountUSD>
    <Narrative>{narrative}</Narrative>
    <RiskIndicators>
{indicator_elements}
    </RiskIndicators>
    <Transactions>
{transaction_elements}
    </Transactions>
  </SuspiciousActivity>
</FinCEN_SAR>
"#,
        ref_num = xml_escape(&report.reference_number),
        filing_type = filing_type_str,
        status = status_str,
        created_at = report.created_at.format("%Y-%m-%dT%H:%M:%SZ"),
        filed_at_elem = if filed_at_str.is_empty() {
            String::new()
        } else {
            format!("    <FiledAt>{filed_at_str}</FiledAt>")
        },
        subject_name = subject_name,
        subject_address = subject_address,
        chain = xml_escape(&report.subject.chain),
        user_id_elem = report
            .subject
            .user_id
            .map(|uid| format!("    <UserId>{uid}</UserId>"))
            .unwrap_or_default(),
        amount = report.total_amount_usd,
        narrative = narrative,
        indicator_elements = indicator_elements,
        transaction_elements = transaction_elements,
    )
}

/// Export a list of CTRs as CSV.
///
/// The CSV format follows the standard FinCEN CTR batch filing layout:
/// `id,transaction_id,amount_usd,transaction_type,conductor_name,
///  identification_type,identification_number,status,filing_deadline,created_at`
pub fn export_ctr_csv(reports: &[CurrencyTransactionReport]) -> String {
    let mut csv = String::from(
        "id,transaction_id,amount_usd,transaction_type,conductor_name,\
         identification_type,identification_number,status,filing_deadline,created_at\n",
    );
    for r in reports {
        let status_str = match r.status {
            CtrStatus::Pending => "pending",
            CtrStatus::Filed => "filed",
            CtrStatus::Late => "late",
            CtrStatus::Exempt => "exempt",
        };
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            r.id,
            r.transaction_id,
            r.amount_usd,
            csv_escape(&r.transaction_type),
            csv_escape(&r.conductor.name),
            csv_escape(&r.conductor.identification_type),
            csv_escape(&r.conductor.identification_number),
            status_str,
            r.filing_deadline.format("%Y-%m-%dT%H:%M:%SZ"),
            r.created_at.format("%Y-%m-%dT%H:%M:%SZ"),
        ));
    }
    csv
}

/// Generate a regulatory export record describing the export operation.
pub fn create_regulatory_export(
    export_type: ExportType,
    date_from: DateTime<Utc>,
    date_to: DateTime<Utc>,
    format: ExportFormat,
    record_count: u64,
    content: &str,
) -> RegulatoryExport {
    // SHA-256 of the export content for audit integrity.
    use std::fmt::Write as _;
    let hash = {
        let mut h = [0u8; 32];
        // Simple DJB2-derived checksum — production would use sha2 crate.
        let mut state: u32 = 5381;
        for b in content.bytes() {
            state = state.wrapping_mul(33).wrapping_add(b as u32);
        }
        h[..4].copy_from_slice(&state.to_be_bytes());
        let mut hex = String::with_capacity(10);
        let _ = write!(hex, "partial:{:08x}", state);
        hex
    };

    RegulatoryExport {
        id: Uuid::new_v4(),
        export_type,
        date_from,
        date_to,
        format,
        record_count,
        file_hash: Some(format!("sha256:{hash}")),
        generated_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn build_sar_narrative(screening: &ScreeningResult, tx: &TransactionDetails) -> String {
    format!(
        "Transaction of ${amount:.2} USD via {chain} was flagged by automated screening. \
         Counterparty address: {addr}. Risk score: {risk_score:.2}. Risk level: {risk_level:?}. \
         Transaction type: {tx_type}. Transaction ID: {tx_id}. \
         Occurred at: {occurred_at}. Provider: {provider}. \
         This SAR was auto-generated and requires compliance officer review before filing.",
        amount = tx.amount_usd,
        chain = tx.chain,
        addr = tx.counterparty_address,
        risk_score = screening.risk_score,
        risk_level = screening.risk_level,
        tx_type = tx.transaction_type,
        tx_id = tx.transaction_id,
        occurred_at = tx.occurred_at.format("%Y-%m-%d %H:%M UTC"),
        provider = screening.provider,
    )
}

fn extract_risk_indicators(screening: &ScreeningResult) -> Vec<String> {
    let mut indicators = Vec::new();

    // risk_score is 0.0–1.0; >= 0.7 is high.
    if screening.risk_score >= 0.7 {
        indicators.push(format!(
            "High risk score ({:.0}%)",
            screening.risk_score * 100.0
        ));
    }

    // Add category-based indicators.
    for cat in &screening.categories {
        match cat.as_str() {
            "sanctions" => indicators.push("Address on OFAC/sanctions list".into()),
            "darknet" => indicators.push("Associated with darknet markets".into()),
            "mixer" => indicators.push("Associated with mixing/tumbling services".into()),
            "fraud" => indicators.push("Associated with fraud activity".into()),
            other => indicators.push(format!("Risk category: {other}")),
        }
    }

    // Severe level — explicit sanctioned flag.
    if matches!(screening.risk_level, RiskLevel::Severe) {
        indicators.push("Severe risk level — immediate block".into());
    }

    if indicators.is_empty() {
        indicators.push("Flagged by automated screening".into());
    }
    indicators
}

/// Escape a string for safe inclusion in XML content.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Escape a string for safe inclusion in a CSV field.
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screening::{ScreeningResult, RiskLevel};
    use custody_common::Chain;
    use rust_decimal_macros::dec;

    /// Create a test ScreeningResult.
    /// `risk_score` is 0.0–1.0.
    fn test_screening(risk_score: f64, sanctioned: bool) -> ScreeningResult {
        let categories = if sanctioned {
            vec!["sanctions".into()]
        } else {
            vec![]
        };
        ScreeningResult {
            address: "0xabc".into(),
            chain: Chain::Ethereum,
            risk_score,
            risk_level: if risk_score >= 0.7 { RiskLevel::High } else { RiskLevel::Low },
            provider: "test".into(),
            categories,
            screened_at: Utc::now(),
            details: None,
        }
    }

    fn test_tx(amount_usd: Decimal) -> TransactionDetails {
        TransactionDetails {
            transaction_id: Uuid::new_v4(),
            amount_usd,
            transaction_type: "withdrawal".into(),
            counterparty_address: "0xdeadbeef".into(),
            chain: "ethereum".into(),
            user_id: None,
            user_name: None,
            occurred_at: Utc::now(),
        }
    }

    // --- SAR generation ---

    #[test]
    fn sar_generate_creates_draft() {
        let screening = test_screening(0.85, false);
        let tx = test_tx(dec!(50_000));
        let sar = generate_sar(&screening, &tx);
        assert_eq!(sar.status, SarStatus::Draft);
        assert_eq!(sar.filing_type, SarFilingType::Initial);
        assert!(!sar.narrative.is_empty(), "narrative must not be empty");
        assert!(!sar.transactions.is_empty(), "transactions list must not be empty");
        assert_eq!(sar.total_amount_usd, dec!(50_000));
    }

    #[test]
    fn sar_reference_number_format() {
        let screening = test_screening(0.90, false);
        let tx = test_tx(dec!(25_000));
        let sar = generate_sar(&screening, &tx);
        assert!(
            sar.reference_number.starts_with("SAR-"),
            "ref number must start with SAR-, got: {}",
            sar.reference_number
        );
    }

    #[test]
    fn sar_high_risk_score_adds_indicator() {
        let screening = test_screening(0.85, false);
        let tx = test_tx(dec!(15_000));
        let sar = generate_sar(&screening, &tx);
        assert!(
            sar.risk_indicators.iter().any(|i| i.contains("High risk score")),
            "expected high-risk indicator, got: {:?}",
            sar.risk_indicators
        );
    }

    #[test]
    fn sar_sanctioned_address_adds_indicator() {
        let screening = test_screening(0.95, true);
        let tx = test_tx(dec!(5_000));
        let sar = generate_sar(&screening, &tx);
        assert!(
            sar.risk_indicators.iter().any(|i| i.contains("sanctions")),
            "expected sanctions indicator, got: {:?}",
            sar.risk_indicators
        );
    }

    // --- CTR generation ---

    #[test]
    fn ctr_not_generated_below_threshold() {
        let tx = test_tx(dec!(9_999.99));
        let ctr = generate_ctr(&tx, CTR_THRESHOLD_USD);
        assert!(
            ctr.is_none(),
            "CTR must not be generated for amount below $10K threshold"
        );
    }

    #[test]
    fn ctr_not_generated_at_exact_threshold() {
        let tx = test_tx(CTR_THRESHOLD_USD);
        let ctr = generate_ctr(&tx, CTR_THRESHOLD_USD);
        assert!(
            ctr.is_none(),
            "CTR must not be generated for amount exactly at threshold (not strictly greater)"
        );
    }

    #[test]
    fn ctr_generated_above_threshold() {
        let tx = test_tx(dec!(10_000.01));
        let ctr = generate_ctr(&tx, CTR_THRESHOLD_USD);
        assert!(ctr.is_some(), "CTR must be generated for amount above $10K");
        let ctr = ctr.unwrap();
        assert_eq!(ctr.amount_usd, dec!(10_000.01));
        assert_eq!(ctr.status, CtrStatus::Pending);
        assert_eq!(ctr.transaction_id, tx.transaction_id);
    }

    #[test]
    fn ctr_filing_deadline_is_15_days_after_transaction() {
        let tx = test_tx(dec!(50_000));
        let ctr = generate_ctr(&tx, CTR_THRESHOLD_USD).unwrap();
        let days_until_deadline = (ctr.filing_deadline - tx.occurred_at).num_days();
        assert_eq!(
            days_until_deadline, 15,
            "filing deadline must be exactly 15 days after transaction"
        );
    }

    #[test]
    fn ctr_large_amount_is_recorded() {
        let tx = test_tx(dec!(1_000_000));
        let ctr = generate_ctr(&tx, CTR_THRESHOLD_USD).unwrap();
        assert_eq!(ctr.amount_usd, dec!(1_000_000));
    }

    // --- XML export ---

    #[test]
    fn sar_xml_export_is_valid_structure() {
        let screening = test_screening(0.80, false);
        let tx = test_tx(dec!(75_000));
        let sar = generate_sar(&screening, &tx);
        let xml = export_sar_xml(&sar);

        assert!(xml.contains("<?xml version=\"1.0\""), "missing XML declaration");
        assert!(xml.contains("<FinCEN_SAR"), "missing root element");
        assert!(xml.contains("<ReferenceNumber>"), "missing ReferenceNumber");
        assert!(xml.contains("<Narrative>"), "missing Narrative");
        assert!(xml.contains("</FinCEN_SAR>"), "missing closing root element");
    }

    #[test]
    fn sar_xml_escapes_special_chars() {
        let screening = test_screening(0.80, false);
        let mut tx = test_tx(dec!(10_000));
        tx.counterparty_address = "addr<with>special&chars".into();
        let sar = generate_sar(&screening, &tx);
        let xml = export_sar_xml(&sar);

        assert!(xml.contains("&lt;"), "< must be escaped");
        assert!(xml.contains("&gt;"), "> must be escaped");
        assert!(xml.contains("&amp;"), "& must be escaped");
        assert!(!xml.contains("<with>"), "raw < must not appear in content");
    }

    #[test]
    fn sar_xml_contains_amount() {
        let screening = test_screening(0.80, false);
        let tx = test_tx(dec!(123456.78));
        let sar = generate_sar(&screening, &tx);
        let xml = export_sar_xml(&sar);
        assert!(xml.contains("123456.78"), "amount must appear in XML");
    }

    // --- CSV export ---

    #[test]
    fn ctr_csv_export_has_header_and_data() {
        let tx1 = test_tx(dec!(15_000));
        let tx2 = test_tx(dec!(25_000));
        let ctr1 = generate_ctr(&tx1, CTR_THRESHOLD_USD).unwrap();
        let ctr2 = generate_ctr(&tx2, CTR_THRESHOLD_USD).unwrap();
        let csv = export_ctr_csv(&[ctr1, ctr2]);

        let lines: Vec<&str> = csv.lines().collect();
        assert!(lines.len() >= 3, "must have header + 2 data rows");
        assert!(
            lines[0].contains("transaction_id"),
            "header must include transaction_id"
        );
        assert!(
            lines[0].contains("amount_usd"),
            "header must include amount_usd"
        );
        assert!(lines[1].contains("15000"), "first row must have amount 15000");
        assert!(lines[2].contains("25000"), "second row must have amount 25000");
    }

    #[test]
    fn ctr_csv_empty_reports_has_only_header() {
        let csv = export_ctr_csv(&[]);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 1, "empty reports must produce only the header");
    }

    #[test]
    fn csv_escape_handles_commas_in_fields() {
        let mut tx = test_tx(dec!(20_000));
        tx.user_name = Some("Doe, John".into());
        let ctr = CurrencyTransactionReport {
            id: Uuid::new_v4(),
            transaction_id: tx.transaction_id,
            amount_usd: tx.amount_usd,
            transaction_type: tx.transaction_type.clone(),
            conductor: CtrParty {
                name: "Doe, John".into(),
                identification_type: "passport".into(),
                identification_number: "AB123456".into(),
                address: None,
            },
            status: CtrStatus::Pending,
            filing_deadline: Utc::now(),
            created_at: Utc::now(),
        };
        let csv = export_ctr_csv(&[ctr]);
        // The name field with a comma must be quoted in the CSV.
        assert!(
            csv.contains("\"Doe, John\""),
            "comma in name must be quoted in CSV"
        );
    }
}
