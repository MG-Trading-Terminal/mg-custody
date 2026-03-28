//! Compliance report API endpoints — SAR/CTR management and export.
//!
//! All endpoints require Admin role. Endpoints:
//! - `GET  /v1/admin/compliance/reports`        — list SAR/CTR reports
//! - `POST /v1/admin/compliance/reports/sar`    — create SAR draft
//! - `GET  /v1/admin/compliance/reports/{id}/export` — export report as XML/CSV

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::Response,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use custody_common::compliance_reporting::{
    CtrStatus, CurrencyTransactionReport, CtrParty, ExportFormat,
    SarFilingType, SarStatus, SarSubject, SuspiciousActivityReport,
};
use custody_compliance::reporting::{generate_sar, generate_ctr, export_sar_xml, export_ctr_csv, TransactionDetails, CTR_THRESHOLD_USD};

use crate::errors::ApiError;
use crate::rbac_middleware::RequireAdmin;
use crate::state::AppState;

/// Build the compliance reports router.
pub fn compliance_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/admin/compliance/reports", get(list_reports))
        .route("/v1/admin/compliance/reports/sar", post(create_sar))
        .route("/v1/admin/compliance/reports/{id}/export", get(export_report))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ListReportsQuery {
    /// Filter by report type: "sar" | "ctr"
    report_type: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
}

#[derive(Debug, Serialize)]
struct ReportListEntry {
    id: Uuid,
    report_type: String,
    status: String,
    amount_usd: Decimal,
    created_at: DateTime<Utc>,
    filed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSarRequest {
    /// Subject's blockchain address.
    pub subject_address: String,
    /// Chain identifier (e.g. "ethereum").
    pub chain: String,
    /// Subject's name if known.
    pub subject_name: Option<String>,
    /// Subject's user_id in the exchange system if known.
    pub subject_user_id: Option<Uuid>,
    /// List of transaction IDs covered by this SAR.
    pub transaction_ids: Vec<Uuid>,
    /// Total amount across all covered transactions in USD.
    pub total_amount_usd: Decimal,
    /// Compliance officer narrative.
    pub narrative: String,
    /// Observed risk indicators.
    pub risk_indicators: Vec<String>,
    /// SAR filing type.
    pub filing_type: SarFilingType,
}

#[derive(Debug, Deserialize)]
struct ExportQuery {
    /// "xml" for SAR XML, "csv" for CTR CSV. Defaults to "xml".
    format: Option<String>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// List SAR and CTR reports. GET /v1/admin/compliance/reports
///
/// Returns a summary list. In a full implementation this would query the DB;
/// here we return an empty list when no in-memory store is present, since
/// compliance report persistence requires a DB migration outside this sprint.
async fn list_reports(
    _auth: RequireAdmin,
    State(_state): State<AppState>,
    Query(params): Query<ListReportsQuery>,
) -> Result<Json<Vec<ReportListEntry>>, ApiError> {
    let _limit = params.limit.unwrap_or(50).min(200);
    let _offset = params.offset.unwrap_or(0);

    // Return an empty list — production implementation would query the DB.
    // The SAR/CTR types and generation functions are fully implemented;
    // DB-backed storage is the next step (separate sprint).
    tracing::info!(
        report_type = ?params.report_type,
        "list_reports called — DB-backed storage not yet wired"
    );
    Ok(Json(vec![]))
}

/// Create a SAR draft. POST /v1/admin/compliance/reports/sar
async fn create_sar(
    _auth: RequireAdmin,
    State(_state): State<AppState>,
    Json(req): Json<CreateSarRequest>,
) -> Result<(StatusCode, Json<SuspiciousActivityReport>), ApiError> {
    if req.narrative.trim().is_empty() {
        return Err(ApiError::bad_request("SAR narrative must not be empty"));
    }
    if req.transaction_ids.is_empty() {
        return Err(ApiError::bad_request(
            "SAR must reference at least one transaction",
        ));
    }
    if req.total_amount_usd <= Decimal::ZERO {
        return Err(ApiError::bad_request(
            "SAR total_amount_usd must be positive",
        ));
    }

    let report = SuspiciousActivityReport {
        id: Uuid::new_v4(),
        reference_number: format!(
            "SAR-{}-{}",
            Utc::now().format("%Y%m%d"),
            &Uuid::new_v4().to_string()[..8],
        ),
        filing_type: req.filing_type,
        subject: SarSubject {
            name: req.subject_name,
            address: req.subject_address,
            chain: req.chain,
            user_id: req.subject_user_id,
        },
        transactions: req.transaction_ids,
        total_amount_usd: req.total_amount_usd,
        narrative: req.narrative,
        risk_indicators: req.risk_indicators,
        status: SarStatus::Draft,
        filed_at: None,
        created_at: Utc::now(),
    };

    tracing::info!(
        id = %report.id,
        ref_num = %report.reference_number,
        amount = %report.total_amount_usd,
        "SAR draft created"
    );

    Ok((StatusCode::CREATED, Json(report)))
}

/// Export a report as XML (SAR) or CSV (CTR). GET /v1/admin/compliance/reports/{id}/export
async fn export_report(
    _auth: RequireAdmin,
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    let format = params.format.as_deref().unwrap_or("xml");

    // In production this would load from DB. For now we return a demo export
    // to validate the format generation is wired end-to-end.
    match format {
        "xml" => {
            // Produce a demo SAR XML for the requested ID.
            let demo_sar = SuspiciousActivityReport {
                id,
                reference_number: format!("SAR-DEMO-{}", &id.to_string()[..8]),
                filing_type: SarFilingType::Initial,
                subject: SarSubject {
                    name: Some("Demo Subject".into()),
                    address: "0x0000000000000000000000000000000000000000".into(),
                    chain: "ethereum".into(),
                    user_id: None,
                },
                transactions: vec![Uuid::nil()],
                total_amount_usd: Decimal::ZERO,
                narrative: "Demo SAR export.".into(),
                risk_indicators: vec!["Demo indicator".into()],
                status: SarStatus::Draft,
                filed_at: None,
                created_at: Utc::now(),
            };
            let xml = export_sar_xml(&demo_sar);
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
                .header(
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"sar-{id}.xml\""),
                )
                .body(axum::body::Body::from(xml))
                .unwrap())
        }
        "csv" => {
            // Produce an empty CTR CSV (no in-memory store yet).
            let csv = export_ctr_csv(&[]);
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
                .header(
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"ctr-{id}.csv\""),
                )
                .body(axum::body::Body::from(csv))
                .unwrap())
        }
        _ => Err(ApiError::bad_request(
            "Unsupported export format. Use 'xml' or 'csv'.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_sar_request_deserializes() {
        let json = serde_json::json!({
            "subject_address": "0xabc",
            "chain": "ethereum",
            "subject_name": "Alice",
            "subject_user_id": null,
            "transaction_ids": ["00000000-0000-0000-0000-000000000001"],
            "total_amount_usd": "75000.00",
            "narrative": "Unusual transaction pattern detected.",
            "risk_indicators": ["High volume", "Mixer association"],
            "filing_type": "initial"
        });
        let req: CreateSarRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.subject_address, "0xabc");
        assert_eq!(req.chain, "ethereum");
        assert_eq!(req.risk_indicators.len(), 2);
        assert!(matches!(req.filing_type, SarFilingType::Initial));
    }
}
