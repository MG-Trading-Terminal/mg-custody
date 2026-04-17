//! Operational tools — stuck withdrawal recovery, historical rescan, UTXO management.

use anyhow::Result;
use custody_common::Chain;
use custody_db::CustodyDb;
use uuid::Uuid;

/// Recover a stuck withdrawal by resetting it to a retryable state.
///
/// Use when a withdrawal is stuck in Broadcast/Confirming status
/// but the tx was never actually mined (e.g., nonce issue).
pub async fn recover_stuck_withdrawal(db: &CustodyDb, withdrawal_id: Uuid) -> Result<String> {
    let w = db.find_withdrawal(withdrawal_id).await?
        .ok_or_else(|| anyhow::anyhow!("Withdrawal not found: {withdrawal_id}"))?;

    match w.status {
        custody_common::WithdrawalStatus::Broadcast |
        custody_common::WithdrawalStatus::Confirming => {
            // Reset to Validated so pipeline re-signs and re-broadcasts
            db.update_withdrawal_status(withdrawal_id, custody_common::WithdrawalStatus::Validated).await?;
            tracing::warn!(id = %withdrawal_id, old_status = ?w.status, "Withdrawal reset to Validated for retry");
            Ok(format!("Reset {withdrawal_id} from {:?} to Validated", w.status))
        }
        other => {
            anyhow::bail!("Cannot recover withdrawal in {:?} status", other)
        }
    }
}

/// Trigger a historical deposit rescan for a chain from a specific block.
pub async fn rescan_deposits(
    db: &CustodyDb,
    chain: Chain,
    from_block: u64,
) -> Result<String> {
    // Reset scan progress to force re-scan
    db.set_scan_progress(chain, from_block).await?;
    tracing::info!(chain = ?chain, from_block, "Deposit rescan scheduled");
    Ok(format!("Rescan scheduled for {:?} from block {from_block}", chain))
}

/// UTXO management: list unspent outputs for Bitcoin.
///
/// Formats the `get_unspent_utxos` DB query for operator consumption.
/// The `address` parameter filters the listing client-side — a future
/// DB-side filter would be nicer but this keeps the path real today.
pub async fn list_utxos(db: &CustodyDb, address: Option<&str>) -> Result<String> {
    let rows = db.get_unspent_utxos().await?;
    let filtered: Vec<_> = rows
        .into_iter()
        .filter(|r| address.is_none_or(|a| r.address.eq_ignore_ascii_case(a)))
        .collect();
    if filtered.is_empty() {
        return Ok("No unspent UTXOs".into());
    }
    let total_sats: u64 = filtered.iter().map(|r| r.amount_sats as u64).sum();
    let mut out = format!("{} UTXO(s), total {} sats:\n", filtered.len(), total_sats);
    for r in &filtered {
        out.push_str(&format!(
            "  {}:{} {} sats {} conf {}{}\n",
            r.txid, r.vout, r.amount_sats, r.confirmations, r.address,
            if r.locked { " [locked]" } else { "" },
        ));
    }
    Ok(out)
}

/// UTXO consolidation: merge small UTXOs into larger ones.
///
/// Returns an honest error until the pipeline wiring for
/// `tx_type = "consolidation"` lands. Previously this returned
/// `Ok("UTXO consolidation planned: ...")` and did nothing — an
/// operator running the CLI subcommand would see a success line
/// while no consolidation had been planned or broadcast.
pub async fn consolidate_utxos(
    _db: &CustodyDb,
    max_inputs: usize,
    min_amount_sats: u64,
) -> Result<String> {
    let _ = (max_inputs, min_amount_sats);
    anyhow::bail!(
        "UTXO consolidation is not wired. The withdrawal pipeline does not yet accept \
         tx_type=\"consolidation\" entries; the scheduler in server/main.rs logs candidates \
         only. Do not run this expecting a broadcast."
    )
}

/// Generate OpenAPI 3.0 spec for the custody API.
pub fn openapi_spec() -> serde_json::Value {
    serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": "MG Custody API",
            "version": "0.1.0",
            "description": "Self-hosted cryptocurrency custody service"
        },
        "servers": [{ "url": "http://localhost:8443" }],
        "paths": {
            "/health": {
                "get": { "summary": "Health check", "responses": { "200": { "description": "OK" } } }
            },
            "/health/chains": {
                "get": { "summary": "Per-chain health", "responses": { "200": { "description": "Chain status" } } }
            },
            "/metrics": {
                "get": { "summary": "Prometheus metrics", "responses": { "200": { "description": "Metrics" } } }
            },
            "/v1/addresses": {
                "post": {
                    "summary": "Create deposit address",
                    "requestBody": { "content": { "application/json": { "schema": {
                        "type": "object",
                        "properties": {
                            "user_id": { "type": "string", "format": "uuid" },
                            "asset": { "type": "string" },
                            "chain": { "type": "string", "enum": ["bitcoin", "ethereum", "tron", "solana"] }
                        },
                        "required": ["user_id", "asset", "chain"]
                    }}}},
                    "responses": { "200": { "description": "Address created" } }
                }
            },
            "/v1/withdrawals": {
                "post": {
                    "summary": "Submit withdrawal",
                    "requestBody": { "content": { "application/json": { "schema": {
                        "type": "object",
                        "properties": {
                            "withdrawal_id": { "type": "string", "format": "uuid" },
                            "asset": { "type": "string" },
                            "chain": { "type": "string" },
                            "address": { "type": "string" },
                            "amount": { "type": "string" }
                        },
                        "required": ["withdrawal_id", "asset", "chain", "address", "amount"]
                    }}}},
                    "responses": { "202": { "description": "Withdrawal accepted" } }
                }
            },
            "/v1/withdrawals/{id}": {
                "get": {
                    "summary": "Get withdrawal status",
                    "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                    "responses": { "200": { "description": "Withdrawal status" } }
                }
            },
            "/v1/withdrawals/{id}/approve": {
                "post": {
                    "summary": "Approve/reject withdrawal",
                    "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                    "responses": { "200": { "description": "Approval result" } }
                }
            },
            "/v1/wallets/{asset}": {
                "get": {
                    "summary": "Get wallet balance",
                    "parameters": [{ "name": "asset", "in": "path", "required": true, "schema": { "type": "string" } }],
                    "responses": { "200": { "description": "Balance info" } }
                }
            }
        },
        "components": {
            "securitySchemes": {
                "hmac": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "X-Custody-Signature",
                    "description": "HMAC-SHA256(method + path + timestamp + body)"
                }
            }
        },
        "security": [{ "hmac": [] }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_spec_valid_structure() {
        let spec = openapi_spec();
        assert_eq!(spec["openapi"], "3.0.3");
        assert!(spec["paths"]["/v1/addresses"]["post"].is_object());
        assert!(spec["paths"]["/v1/withdrawals"]["post"].is_object());
        assert!(spec["paths"]["/v1/withdrawals/{id}"]["get"].is_object());
        assert!(spec["paths"]["/health"]["get"].is_object());
        assert!(spec["paths"]["/metrics"]["get"].is_object());
    }

    #[test]
    fn openapi_has_security_scheme() {
        let spec = openapi_spec();
        assert!(spec["components"]["securitySchemes"]["hmac"].is_object());
    }
}
