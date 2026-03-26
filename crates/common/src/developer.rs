//! Developer experience types — OpenAPI, SDK, rate limiting, pagination.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// OpenAPI spec metadata
// ---------------------------------------------------------------------------

/// OpenAPI endpoint documentation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEndpoint {
    pub method: String,
    pub path: String,
    pub summary: String,
    pub tag: String,
    pub auth_required: bool,
    pub rate_limit: Option<u32>,
}

/// Generate the full endpoint catalog.
pub fn all_endpoints() -> Vec<ApiEndpoint> {
    vec![
        // Health
        ep("GET", "/health", "Basic health check", "health", false, None),
        ep("GET", "/health/chains", "Per-chain health", "health", false, None),
        ep("GET", "/health/deep", "Deep health with DB check", "health", false, None),
        ep("GET", "/metrics", "Prometheus metrics", "health", false, None),

        // Core
        ep("POST", "/v1/addresses", "Create deposit address", "core", true, Some(100)),
        ep("POST", "/v1/withdrawals", "Submit withdrawal", "core", true, Some(100)),
        ep("GET", "/v1/withdrawals/{id}", "Get withdrawal status", "core", true, Some(200)),
        ep("POST", "/v1/withdrawals/{id}/approve", "Approve/reject withdrawal", "core", true, Some(50)),
        ep("GET", "/v1/wallets/{asset}", "Get wallet balance", "core", true, Some(200)),

        // Vaults
        ep("POST", "/v1/vaults", "Create vault", "vaults", true, Some(50)),
        ep("GET", "/v1/vaults", "List vaults", "vaults", true, Some(200)),
        ep("GET", "/v1/vaults/{id}", "Get vault with accounts", "vaults", true, Some(200)),
        ep("PUT", "/v1/vaults/{id}", "Update vault", "vaults", true, Some(50)),
        ep("DELETE", "/v1/vaults/{id}", "Soft-delete vault", "vaults", true, Some(50)),
        ep("POST", "/v1/vaults/{id}/accounts", "Create vault account", "vaults", true, Some(50)),
        ep("GET", "/v1/vaults/{id}/accounts", "List vault accounts", "vaults", true, Some(200)),
        ep("GET", "/v1/vaults/{vault_id}/accounts/{account_id}", "Get vault account", "vaults", true, Some(200)),
        ep("POST", "/v1/vaults/{id}/raw-sign", "Raw message signing", "vaults", true, Some(20)),
        ep("GET", "/v1/vaults/{id}/keys", "List signing keys", "vaults", true, Some(100)),
        ep("GET", "/v1/vaults/{id}/keys/{chain}", "Get key for chain", "vaults", true, Some(100)),
        ep("GET", "/v1/vaults/{id}/policies", "List vault policies", "vaults", true, Some(100)),
        ep("POST", "/v1/vaults/{id}/policies", "Create vault policy", "vaults", true, Some(50)),
        ep("GET", "/v1/vaults/{id}/approvals", "List pending approvals", "vaults", true, Some(100)),

        // Transactions
        ep("GET", "/v1/transactions", "List transactions (unified)", "transactions", true, Some(200)),
        ep("GET", "/v1/transactions/{id}", "Get transaction", "transactions", true, Some(200)),
        ep("POST", "/v1/transactions/{id}/cancel", "Cancel transaction (RBF)", "transactions", true, Some(20)),
        ep("POST", "/v1/transactions/{id}/accelerate", "Accelerate transaction", "transactions", true, Some(20)),
        ep("GET", "/v1/transactions/{id}/notes", "List transaction notes", "transactions", true, Some(200)),
        ep("POST", "/v1/transactions/{id}/notes", "Add transaction note", "transactions", true, Some(50)),
        ep("POST", "/v1/transactions/batch", "Batch withdrawal", "transactions", true, Some(10)),

        // Transfers
        ep("POST", "/v1/transfers", "Internal vault-to-vault transfer", "transfers", true, Some(50)),
        ep("GET", "/v1/transfers/{id}", "Get transfer status", "transfers", true, Some(200)),

        // Assets
        ep("GET", "/v1/assets", "List managed assets", "assets", true, Some(200)),
        ep("POST", "/v1/assets", "Add asset", "assets", true, Some(50)),
        ep("GET", "/v1/assets/{symbol}/{chain}", "Get asset detail", "assets", true, Some(200)),
        ep("PUT", "/v1/assets/{id}", "Enable/disable asset", "assets", true, Some(50)),
        ep("DELETE", "/v1/assets/{id}", "Remove asset", "assets", true, Some(50)),
        ep("GET", "/v1/estimate-fee", "Estimate transaction fee", "assets", true, Some(200)),

        // Webhooks
        ep("GET", "/v1/webhooks", "List webhook endpoints", "webhooks", true, Some(100)),
        ep("POST", "/v1/webhooks", "Create webhook endpoint", "webhooks", true, Some(50)),
        ep("GET", "/v1/webhooks/{id}", "Get webhook endpoint", "webhooks", true, Some(100)),
        ep("PUT", "/v1/webhooks/{id}", "Update webhook endpoint", "webhooks", true, Some(50)),
        ep("DELETE", "/v1/webhooks/{id}", "Delete webhook endpoint", "webhooks", true, Some(50)),
        ep("POST", "/v1/webhooks/{id}/test", "Test webhook delivery", "webhooks", true, Some(10)),

        // Address book
        ep("GET", "/v1/address-book", "List address book", "address_book", true, Some(200)),
        ep("POST", "/v1/address-book", "Add address", "address_book", true, Some(50)),
        ep("GET", "/v1/address-book/{id}", "Get address entry", "address_book", true, Some(200)),
        ep("PUT", "/v1/address-book/{id}", "Update address", "address_book", true, Some(50)),
        ep("DELETE", "/v1/address-book/{id}", "Remove address", "address_book", true, Some(50)),

        // Audit
        ep("GET", "/v1/audit-log", "Query audit log", "audit", true, Some(100)),

        // Connections
        ep("GET", "/v1/connections", "List exchange connections", "connections", true, Some(100)),
        ep("POST", "/v1/connections", "Create connection", "connections", true, Some(50)),
        ep("GET", "/v1/connections/{id}", "Get connection", "connections", true, Some(100)),
        ep("PUT", "/v1/connections/{id}", "Update connection", "connections", true, Some(50)),
        ep("DELETE", "/v1/connections/{id}", "Delete connection", "connections", true, Some(50)),
        ep("POST", "/v1/connections/{id}/heartbeat", "Heartbeat", "connections", true, Some(200)),

        // API keys
        ep("GET", "/v1/api-keys", "List API keys", "api_keys", true, Some(100)),
        ep("POST", "/v1/api-keys", "Create API key", "api_keys", true, Some(20)),
        ep("GET", "/v1/api-keys/{id}", "Get API key", "api_keys", true, Some(100)),
        ep("POST", "/v1/api-keys/{id}/revoke", "Revoke API key", "api_keys", true, Some(20)),
        ep("DELETE", "/v1/api-keys/{id}", "Delete API key", "api_keys", true, Some(20)),

        // Notifications
        ep("GET", "/v1/notifications/channels", "List channels", "notifications", true, Some(100)),
        ep("POST", "/v1/notifications/channels", "Create channel", "notifications", true, Some(50)),
        ep("DELETE", "/v1/notifications/channels/{id}", "Delete channel", "notifications", true, Some(50)),
        ep("POST", "/v1/notifications/channels/{id}/toggle", "Toggle channel", "notifications", true, Some(50)),
        ep("POST", "/v1/notifications/test/{id}", "Test notification", "notifications", true, Some(10)),

        // System
        ep("GET", "/v1/system/config", "Get system config", "system", true, Some(100)),
        ep("PUT", "/v1/system/config", "Update system config", "system", true, Some(20)),
        ep("GET", "/v1/network/fees", "Network fee cache", "system", true, Some(200)),
        ep("GET", "/v1/network/status", "Network status", "system", true, Some(200)),

        // Workspaces
        ep("GET", "/v1/workspaces", "List workspaces", "workspaces", true, Some(100)),
        ep("POST", "/v1/workspaces", "Create workspace", "workspaces", true, Some(50)),
        ep("GET", "/v1/workspaces/{id}", "Get workspace", "workspaces", true, Some(100)),
        ep("PUT", "/v1/workspaces/{id}", "Update workspace", "workspaces", true, Some(50)),
        ep("GET", "/v1/workspaces/{id}/users", "List workspace users", "workspaces", true, Some(100)),
        ep("POST", "/v1/workspaces/{id}/users", "Add user", "workspaces", true, Some(50)),
        ep("DELETE", "/v1/workspaces/{ws_id}/users/{user_id}", "Remove user", "workspaces", true, Some(50)),

        // Deposit addresses
        ep("GET", "/v1/deposit-addresses", "List deposit addresses", "addresses", true, Some(200)),
        ep("POST", "/v1/deposit-addresses/{addr}/bind", "Bind to vault", "addresses", true, Some(50)),

        // Reporting
        ep("POST", "/v1/reports/generate", "Generate report", "reports", true, Some(10)),

        // Travel rule
        ep("POST", "/v1/travel-rule", "Submit travel rule data", "compliance", true, Some(50)),
        ep("GET", "/v1/travel-rule/{withdrawal_id}", "Get travel rule", "compliance", true, Some(100)),

        // Policies & approvals
        ep("GET", "/v1/policies/{id}", "Get policy", "policies", true, Some(100)),
        ep("PUT", "/v1/policies/{id}", "Update policy", "policies", true, Some(50)),
        ep("DELETE", "/v1/policies/{id}", "Delete policy", "policies", true, Some(50)),
        ep("GET", "/v1/approvals/{withdrawal_id}", "Get approval workflow", "approvals", true, Some(100)),
        ep("POST", "/v1/approvals/{withdrawal_id}/decide", "Submit decision", "approvals", true, Some(50)),

        // Networks
        ep("GET", "/v1/supported-networks", "List supported networks", "networks", false, Some(200)),
        ep("GET", "/v1/supported-networks/{chain}", "Get network detail", "networks", false, Some(200)),
        ep("GET", "/v1/supported-networks/{chain}/tokens", "List network tokens", "networks", false, Some(200)),

        // Staking
        ep("GET", "/v1/staking/positions", "List staking positions", "staking", true, Some(100)),
        ep("POST", "/v1/staking/stake", "Stake", "staking", true, Some(20)),
        ep("POST", "/v1/staking/unstake", "Unstake", "staking", true, Some(20)),

        // Scheduling
        ep("GET", "/v1/scheduled", "List scheduled transactions", "scheduling", true, Some(200)),
        ep("POST", "/v1/scheduled", "Schedule transaction", "scheduling", true, Some(50)),
        ep("GET", "/v1/scheduled/{id}", "Get scheduled transaction", "scheduling", true, Some(200)),
        ep("POST", "/v1/scheduled/{id}/cancel", "Cancel scheduled", "scheduling", true, Some(50)),

        // Screening
        ep("POST", "/v1/screening/address", "Screen address", "compliance", true, Some(50)),
        ep("GET", "/v1/screening/history/{addr}", "Screening history", "compliance", true, Some(100)),

        // Backup
        ep("GET", "/v1/backup/status", "Backup health status", "operations", true, Some(100)),

        // Token discovery
        ep("POST", "/v1/tokens/discover", "Discover token", "assets", true, Some(20)),

        // Dashboard
        ep("GET", "/v1/dashboard/health", "System health dashboard", "operations", true, Some(200)),
        ep("GET", "/v1/dashboard/pipeline", "Pipeline status", "operations", true, Some(200)),

        // Gas station
        ep("GET", "/v1/gas-station/status", "Gas station status", "operations", true, Some(100)),

        // WebSocket
        ep("GET", "/v1/ws", "WebSocket event stream", "events", true, None),
    ]
}

fn ep(method: &str, path: &str, summary: &str, tag: &str, auth: bool, rate: Option<u32>) -> ApiEndpoint {
    ApiEndpoint {
        method: method.into(), path: path.into(), summary: summary.into(),
        tag: tag.into(), auth_required: auth, rate_limit: rate,
    }
}

/// Generate an OpenAPI 3.0 spec as JSON.
pub fn generate_openapi_spec() -> serde_json::Value {
    let endpoints = all_endpoints();

    let mut paths = serde_json::Map::new();
    for ep in &endpoints {
        let path_entry = paths.entry(ep.path.clone()).or_insert_with(|| serde_json::json!({}));
        let method = ep.method.to_lowercase();
        path_entry[&method] = serde_json::json!({
            "summary": ep.summary,
            "tags": [ep.tag],
            "security": if ep.auth_required { serde_json::json!([{"hmac": []}]) } else { serde_json::json!([]) },
            "responses": {
                "200": {"description": "Success"},
                "400": {"description": "Bad request"},
                "404": {"description": "Not found"},
                "500": {"description": "Internal error"},
            }
        });
    }

    serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": "MG Custody API",
            "description": "Self-hosted institutional cryptocurrency custody service",
            "version": env!("CARGO_PKG_VERSION"),
            "contact": {"name": "MG Custody"},
        },
        "servers": [
            {"url": "http://localhost:8443", "description": "Local development"},
        ],
        "paths": paths,
        "components": {
            "securitySchemes": {
                "hmac": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "Authorization",
                    "description": "HMAC-SHA256(method + path + timestamp + body)",
                }
            }
        },
        "tags": [
            {"name": "health", "description": "Health checks"},
            {"name": "core", "description": "Core deposit/withdrawal operations"},
            {"name": "vaults", "description": "Vault management"},
            {"name": "transactions", "description": "Transaction history and management"},
            {"name": "transfers", "description": "Internal vault-to-vault transfers"},
            {"name": "assets", "description": "Asset and token management"},
            {"name": "webhooks", "description": "Webhook endpoint management"},
            {"name": "address_book", "description": "Saved destination addresses"},
            {"name": "audit", "description": "Audit trail"},
            {"name": "connections", "description": "Exchange connections"},
            {"name": "api_keys", "description": "API key management"},
            {"name": "notifications", "description": "Notification channels"},
            {"name": "system", "description": "System configuration"},
            {"name": "workspaces", "description": "Multi-tenant workspaces"},
            {"name": "addresses", "description": "Deposit address management"},
            {"name": "reports", "description": "Reporting and export"},
            {"name": "compliance", "description": "Travel rule and screening"},
            {"name": "policies", "description": "Vault policies"},
            {"name": "approvals", "description": "Approval workflows"},
            {"name": "networks", "description": "Supported blockchain networks"},
            {"name": "staking", "description": "Staking operations"},
            {"name": "scheduling", "description": "Transaction scheduling"},
            {"name": "operations", "description": "Operational endpoints"},
            {"name": "events", "description": "Real-time event streaming"},
        ],
    })
}

/// Render extended SDK method list for all endpoints.
pub fn sdk_methods() -> Vec<SdkMethod> {
    all_endpoints().iter().map(|ep| {
        let method_name = ep.path
            .trim_start_matches("/v1/")
            .replace('/', "_")
            .replace('{', "")
            .replace('}', "");
        let method_name = format!("{}_{}", ep.method.to_lowercase(), method_name);

        SdkMethod {
            name: method_name,
            http_method: ep.method.clone(),
            path: ep.path.clone(),
            description: ep.summary.clone(),
        }
    }).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkMethod {
    pub name: String,
    pub http_method: String,
    pub path: String,
    pub description: String,
}

/// Standard cursor pagination response wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
    pub total_count: Option<u64>,
    pub has_more: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_catalog_count() {
        let eps = all_endpoints();
        assert!(eps.len() >= 100, "Should have 100+ endpoints, got {}", eps.len());
    }

    #[test]
    fn endpoint_catalog_unique_paths() {
        let eps = all_endpoints();
        let mut seen = std::collections::HashSet::new();
        for ep in &eps {
            let key = format!("{} {}", ep.method, ep.path);
            assert!(seen.insert(key.clone()), "Duplicate endpoint: {key}");
        }
    }

    #[test]
    fn openapi_spec_valid_structure() {
        let spec = generate_openapi_spec();
        assert_eq!(spec["openapi"], "3.0.3");
        assert!(spec["info"]["title"].as_str().unwrap().contains("MG Custody"));
        assert!(spec["paths"].as_object().unwrap().len() >= 60);
    }

    #[test]
    fn openapi_spec_has_tags() {
        let spec = generate_openapi_spec();
        let tags = spec["tags"].as_array().unwrap();
        assert!(tags.len() >= 20);
    }

    #[test]
    fn sdk_methods_generated() {
        let methods = sdk_methods();
        assert!(methods.len() >= 100);
        // Check naming convention
        for m in &methods {
            assert!(m.name.starts_with("get_") || m.name.starts_with("post_") ||
                    m.name.starts_with("put_") || m.name.starts_with("delete_"),
                    "Unexpected method name: {}", m.name);
        }
    }

    #[test]
    fn paginated_response_serialization() {
        let resp = PaginatedResponse {
            data: vec!["item1", "item2"],
            next_cursor: Some("abc123".into()),
            total_count: Some(100),
            has_more: true,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("has_more"));
        assert!(json.contains("abc123"));
    }

    #[test]
    fn all_endpoints_have_tags() {
        let eps = all_endpoints();
        for ep in &eps {
            assert!(!ep.tag.is_empty(), "Endpoint {} {} has no tag", ep.method, ep.path);
        }
    }

    #[test]
    fn health_endpoints_no_auth() {
        let eps = all_endpoints();
        let health = eps.iter().filter(|e| e.tag == "health").collect::<Vec<_>>();
        for h in &health {
            assert!(!h.auth_required, "Health endpoint {} should not require auth", h.path);
        }
    }
}
