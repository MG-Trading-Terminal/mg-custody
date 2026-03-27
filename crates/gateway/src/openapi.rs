//! OpenAPI 3.1 spec generator.
//!
//! Builds the complete OpenAPI 3.1 specification from route definitions.
//! No heavy dependencies — the spec is assembled from `serde_json::Value` directly.
//!
//! The spec is served at `GET /v1/openapi.json`.
//!
//! ## Approach
//! - Collect all known endpoints from `custody_common::developer::all_endpoints()`
//!   plus the swap routes added in Phase 24.
//! - Add per-endpoint schemas, parameters, and request/response bodies where known.
//! - Include auth schemes: Bearer token (session) and HMAC-SHA256 (service-to-service).
//! - Return as `serde_json::Value` — no code generation or reflection required.

use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Generate an OpenAPI 3.1 specification for the custody service API.
///
/// Includes all routes (core, swap, vault, staking, governance, etc.)
/// with request/response schemas, auth requirements, and tag grouping.
pub fn generate_openapi_v3() -> Value {
    let paths = build_paths();
    let components = build_components();
    let tags = build_tags();

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "MG Custody API",
            "description": "Self-hosted institutional cryptocurrency custody service.\n\nInternal-only — never exposed to the internet directly. All requests authenticated via Bearer session token or HMAC-SHA256 (service-to-service).",
            "version": env!("CARGO_PKG_VERSION"),
            "contact": {
                "name": "MG Custody",
                "url": "https://github.com/mg-custody"
            },
            "license": {
                "name": "Proprietary"
            }
        },
        "servers": [
            {
                "url": "http://localhost:8443",
                "description": "Local development"
            },
            {
                "url": "https://custody.internal:8443",
                "description": "Production (internal network only)"
            }
        ],
        "security": [
            {"bearerAuth": []},
            {"hmacAuth": []}
        ],
        "paths": paths,
        "components": components,
        "tags": tags
    })
}

// ---------------------------------------------------------------------------
// Path builders
// ---------------------------------------------------------------------------

fn build_paths() -> Value {
    let mut paths = serde_json::Map::new();

    // ---- Health ----
    add_path(&mut paths, "/health", "get", json!({
        "summary": "Basic health check",
        "description": "Returns 'ok' when the service is running.",
        "tags": ["health"],
        "security": [],
        "responses": {
            "200": {
                "description": "Service is healthy",
                "content": {
                    "text/plain": {
                        "schema": {"type": "string", "example": "ok"}
                    }
                }
            }
        }
    }));

    add_path(&mut paths, "/health/chains", "get", json!({
        "summary": "Per-chain node health",
        "description": "Returns connectivity status for each supported blockchain node.",
        "tags": ["health"],
        "security": [],
        "responses": {
            "200": {
                "description": "Chain status map",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/ChainHealthResponse"}
                    }
                }
            }
        }
    }));

    add_path(&mut paths, "/health/deep", "get", json!({
        "summary": "Deep health check (DB + chains)",
        "tags": ["health"],
        "security": [],
        "responses": {
            "200": {"description": "Deep health result"},
            "503": {"description": "One or more subsystems unhealthy"}
        }
    }));

    add_path(&mut paths, "/metrics", "get", json!({
        "summary": "Prometheus metrics",
        "description": "Prometheus-format metrics for scraping.",
        "tags": ["health"],
        "security": [],
        "responses": {
            "200": {
                "description": "Metrics in Prometheus text format",
                "content": {"text/plain": {"schema": {"type": "string"}}}
            }
        }
    }));

    // ---- Core: Addresses ----
    add_path(&mut paths, "/v1/addresses", "post", json!({
        "summary": "Create deposit address",
        "description": "Creates or returns an existing deposit address for a user+asset+chain combination. Idempotent.",
        "tags": ["core"],
        "security": [{"bearerAuth": []}, {"hmacAuth": []}],
        "requestBody": {
            "required": true,
            "content": {
                "application/json": {
                    "schema": {"$ref": "#/components/schemas/CreateAddressRequest"}
                }
            }
        },
        "responses": {
            "200": {
                "description": "Deposit address",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/CreateAddressResponse"}
                    }
                }
            },
            "400": {"$ref": "#/components/responses/BadRequest"},
            "401": {"$ref": "#/components/responses/Unauthorized"},
            "500": {"$ref": "#/components/responses/InternalError"}
        }
    }));

    // ---- Core: Withdrawals ----
    add_path(&mut paths, "/v1/withdrawals", "post", json!({
        "summary": "Submit withdrawal request",
        "description": "Submit a new withdrawal. Idempotent on withdrawal_id.",
        "tags": ["core"],
        "security": [{"bearerAuth": []}, {"hmacAuth": []}],
        "requestBody": {
            "required": true,
            "content": {
                "application/json": {
                    "schema": {"$ref": "#/components/schemas/SubmitWithdrawalRequest"}
                }
            }
        },
        "responses": {
            "202": {
                "description": "Withdrawal accepted",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/SubmitWithdrawalResponse"}
                    }
                }
            },
            "200": {"description": "Already submitted (idempotent)"},
            "400": {"$ref": "#/components/responses/BadRequest"},
            "401": {"$ref": "#/components/responses/Unauthorized"}
        }
    }));

    add_path(&mut paths, "/v1/withdrawals/{id}", "get", json!({
        "summary": "Get withdrawal status",
        "tags": ["core"],
        "security": [{"bearerAuth": []}, {"hmacAuth": []}],
        "parameters": [path_param("id", "UUID of the withdrawal", "string", "uuid")],
        "responses": {
            "200": {
                "description": "Withdrawal status",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/WithdrawalStatusResponse"}
                    }
                }
            },
            "404": {"$ref": "#/components/responses/NotFound"}
        }
    }));

    add_path(&mut paths, "/v1/withdrawals/{id}/approve", "post", json!({
        "summary": "Approve or reject a withdrawal",
        "description": "Requires Signer role. Approves or rejects a withdrawal pending approval.",
        "tags": ["core"],
        "security": [{"bearerAuth": []}],
        "parameters": [path_param("id", "UUID of the withdrawal", "string", "uuid")],
        "requestBody": {
            "required": true,
            "content": {
                "application/json": {
                    "schema": {
                        "type": "object",
                        "properties": {
                            "approved": {"type": "boolean"},
                            "approver": {"type": "string"}
                        },
                        "required": ["approved"]
                    }
                }
            }
        },
        "responses": {
            "200": {"description": "Decision recorded"},
            "403": {"$ref": "#/components/responses/Forbidden"},
            "404": {"$ref": "#/components/responses/NotFound"},
            "409": {"$ref": "#/components/responses/Conflict"}
        }
    }));

    // ---- Core: Wallets ----
    add_path(&mut paths, "/v1/wallets/{asset}", "get", json!({
        "summary": "Get wallet balance for asset",
        "tags": ["core"],
        "security": [{"bearerAuth": []}, {"hmacAuth": []}],
        "parameters": [path_param("asset", "Asset symbol (e.g. ETH, USDT)", "string", "")],
        "responses": {
            "200": {
                "description": "Wallet balance breakdown",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/WalletBalanceResponse"}
                    }
                }
            }
        }
    }));

    // ---- Swaps (Phase 24) ----
    add_path(&mut paths, "/v1/swaps/quote", "post", json!({
        "summary": "Get a swap quote",
        "description": "Get a price quote for swapping tokens. Supports UniswapV3 (Ethereum) and Jupiter (Solana). Quotes expire in 30 seconds.",
        "tags": ["swaps"],
        "security": [{"bearerAuth": []}],
        "requestBody": {
            "required": true,
            "content": {
                "application/json": {
                    "schema": {"$ref": "#/components/schemas/SwapQuoteRequest"}
                }
            }
        },
        "responses": {
            "200": {
                "description": "Swap quote",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/SwapQuoteResponse"}
                    }
                }
            },
            "400": {"$ref": "#/components/responses/BadRequest"},
            "401": {"$ref": "#/components/responses/Unauthorized"}
        }
    }));

    add_path(&mut paths, "/v1/swaps", "post", json!({
        "summary": "Execute a swap quote",
        "description": "Build transaction calldata/payload from a quote. Requires Signer role. Does NOT sign or broadcast — returns the unsigned transaction for the signing service.",
        "tags": ["swaps"],
        "security": [{"bearerAuth": []}],
        "requestBody": {
            "required": true,
            "content": {
                "application/json": {
                    "schema": {"$ref": "#/components/schemas/SwapExecuteRequest"}
                }
            }
        },
        "responses": {
            "201": {
                "description": "Swap execution ready for signing",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/SwapExecuteResponse"}
                    }
                }
            },
            "400": {"$ref": "#/components/responses/BadRequest"},
            "401": {"$ref": "#/components/responses/Unauthorized"},
            "403": {"$ref": "#/components/responses/Forbidden"}
        }
    }));

    add_path(&mut paths, "/v1/swaps/{id}", "get", json!({
        "summary": "Get swap status",
        "tags": ["swaps"],
        "security": [{"bearerAuth": []}, {"hmacAuth": []}],
        "parameters": [path_param("id", "UUID of the swap execution", "string", "uuid")],
        "responses": {
            "200": {
                "description": "Swap execution status",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/SwapStatusResponse"}
                    }
                }
            },
            "404": {"$ref": "#/components/responses/NotFound"}
        }
    }));

    // ---- Bulk endpoints from developer catalog ----
    // Include remaining endpoints from the existing catalog (abbreviated for doc completeness)
    for ep in remaining_catalog_entries() {
        let op = build_generic_operation(&ep);
        add_path(&mut paths, &ep.path, &ep.method.to_lowercase(), op);
    }

    Value::Object(paths)
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

fn build_components() -> Value {
    json!({
        "securitySchemes": {
            "bearerAuth": {
                "type": "http",
                "scheme": "bearer",
                "bearerFormat": "JWT",
                "description": "Session token obtained from POST /v1/auth/sessions"
            },
            "hmacAuth": {
                "type": "apiKey",
                "in": "header",
                "name": "Authorization",
                "description": "HMAC-SHA256(method + path + timestamp + body). Format: HMAC <hex_signature>. Include X-Timestamp header with Unix timestamp."
            }
        },
        "responses": {
            "BadRequest": {
                "description": "Bad request — invalid parameters",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/ApiError"}
                    }
                }
            },
            "Unauthorized": {
                "description": "Authentication required",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/ApiError"}
                    }
                }
            },
            "Forbidden": {
                "description": "Insufficient role/permissions",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/ApiError"}
                    }
                }
            },
            "NotFound": {
                "description": "Resource not found",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/ApiError"}
                    }
                }
            },
            "Conflict": {
                "description": "State conflict",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/ApiError"}
                    }
                }
            },
            "InternalError": {
                "description": "Internal server error",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/ApiError"}
                    }
                }
            }
        },
        "schemas": {
            // Error
            "ApiError": {
                "type": "object",
                "required": ["error", "message", "request_id"],
                "properties": {
                    "error": {"type": "string", "example": "not_found"},
                    "message": {"type": "string", "example": "Withdrawal 123 not found"},
                    "request_id": {"type": "string", "format": "uuid"}
                }
            },
            // Core schemas
            "CreateAddressRequest": {
                "type": "object",
                "required": ["user_id", "asset", "chain"],
                "properties": {
                    "user_id": {"type": "string", "format": "uuid"},
                    "asset": {"type": "string", "example": "USDT"},
                    "chain": {"$ref": "#/components/schemas/Chain"}
                }
            },
            "CreateAddressResponse": {
                "type": "object",
                "required": ["address", "chain"],
                "properties": {
                    "address": {"type": "string", "example": "0xAbCd..."},
                    "chain": {"$ref": "#/components/schemas/Chain"},
                    "memo": {"type": "string", "nullable": true}
                }
            },
            "SubmitWithdrawalRequest": {
                "type": "object",
                "required": ["withdrawal_id", "asset", "chain", "address", "amount"],
                "properties": {
                    "withdrawal_id": {"type": "string", "format": "uuid"},
                    "asset": {"type": "string", "example": "USDT"},
                    "chain": {"$ref": "#/components/schemas/Chain"},
                    "address": {"type": "string"},
                    "amount": {"type": "string", "description": "Decimal amount as string — never float", "example": "1000.50"}
                }
            },
            "SubmitWithdrawalResponse": {
                "type": "object",
                "required": ["custody_ref", "status"],
                "properties": {
                    "custody_ref": {"type": "string", "format": "uuid"},
                    "status": {"$ref": "#/components/schemas/WithdrawalStatus"}
                }
            },
            "WithdrawalStatusResponse": {
                "type": "object",
                "properties": {
                    "custody_ref": {"type": "string", "format": "uuid"},
                    "exchange_withdrawal_id": {"type": "string", "format": "uuid"},
                    "status": {"$ref": "#/components/schemas/WithdrawalStatus"},
                    "tx_hash": {"type": "string", "nullable": true},
                    "confirmations": {"type": "integer"},
                    "required_confirmations": {"type": "integer"},
                    "error": {"type": "string", "nullable": true}
                }
            },
            "WalletBalanceResponse": {
                "type": "object",
                "required": ["asset", "hot_balance", "warm_balance", "total_managed"],
                "properties": {
                    "asset": {"type": "string"},
                    "hot_balance": {"type": "string", "description": "Decimal string"},
                    "warm_balance": {"type": "string", "description": "Decimal string"},
                    "total_managed": {"type": "string", "description": "Decimal string"}
                }
            },
            // Chain types
            "Chain": {
                "type": "string",
                "enum": ["ethereum", "bitcoin", "tron", "solana"],
                "description": "Supported blockchain (lowercase per API serialization)"
            },
            "WithdrawalStatus": {
                "type": "string",
                "enum": ["received", "validated", "signed", "broadcast", "confirmed", "failed", "cancelled", "pending_approval"],
                "description": "Withdrawal lifecycle state (snake_case per API serialization)"
            },
            // Health
            "ChainHealthResponse": {
                "type": "object",
                "properties": {
                    "chains": {
                        "type": "object",
                        "additionalProperties": {
                            "type": "object",
                            "properties": {
                                "status": {"type": "string", "enum": ["ok", "degraded", "unknown"]}
                            }
                        }
                    }
                }
            },
            // Swap schemas (Phase 24)
            "SwapQuoteRequest": {
                "type": "object",
                "required": ["chain", "from_token", "to_token", "amount_in", "slippage_bps"],
                "properties": {
                    "chain": {"$ref": "#/components/schemas/Chain"},
                    "from_token": {
                        "type": "string",
                        "description": "EVM address (0x...) for Ethereum, mint address for Solana",
                        "example": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
                    },
                    "to_token": {
                        "type": "string",
                        "example": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
                    },
                    "amount_in": {
                        "type": "string",
                        "description": "Amount in from_token's display units. Always a decimal string.",
                        "example": "1000.00"
                    },
                    "slippage_bps": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 200,
                        "description": "Slippage tolerance in basis points (max 200 = 2%)",
                        "example": 50
                    }
                }
            },
            "SwapQuoteResponse": {
                "type": "object",
                "required": ["quote"],
                "properties": {
                    "quote": {"$ref": "#/components/schemas/SwapQuote"}
                }
            },
            "SwapQuote": {
                "type": "object",
                "required": ["quote_id", "chain", "from_token", "to_token", "amount_in", "expected_out", "minimum_out", "slippage_bps", "protocol", "expires_at"],
                "properties": {
                    "quote_id": {"type": "string", "format": "uuid"},
                    "chain": {"$ref": "#/components/schemas/Chain"},
                    "from_token": {"type": "string"},
                    "to_token": {"type": "string"},
                    "amount_in": {"type": "string", "description": "Decimal string"},
                    "expected_out": {"type": "string", "description": "Decimal string"},
                    "minimum_out": {"type": "string", "description": "Minimum output after slippage — Decimal string"},
                    "slippage_bps": {"type": "integer"},
                    "price_impact_bps": {"type": "integer"},
                    "estimated_gas": {"type": "string", "nullable": true, "description": "Estimated gas in native token — Decimal string"},
                    "protocol": {
                        "type": "string",
                        "enum": ["uniswap_v3", "jupiter"],
                        "description": "DEX protocol used for this quote"
                    },
                    "expires_at": {"type": "string", "format": "date-time"}
                }
            },
            "SwapExecuteRequest": {
                "type": "object",
                "required": ["quote_id", "quote"],
                "properties": {
                    "quote_id": {"type": "string", "format": "uuid"},
                    "quote": {"$ref": "#/components/schemas/SwapQuote"}
                }
            },
            "SwapExecuteResponse": {
                "type": "object",
                "required": ["swap_id", "status"],
                "properties": {
                    "swap_id": {"type": "string", "format": "uuid"},
                    "status": {"$ref": "#/components/schemas/SwapExecutionStatus"},
                    "calldata": {
                        "type": "string",
                        "nullable": true,
                        "description": "Hex-encoded EVM calldata (Ethereum only)"
                    },
                    "encoded_tx": {
                        "type": "string",
                        "nullable": true,
                        "description": "Base64-encoded versioned transaction (Solana only)"
                    },
                    "router_address": {
                        "type": "string",
                        "nullable": true,
                        "description": "Contract address to send calldata to (Ethereum only)"
                    }
                }
            },
            "SwapStatusResponse": {
                "type": "object",
                "properties": {
                    "swap_id": {"type": "string", "format": "uuid"},
                    "status": {"$ref": "#/components/schemas/SwapExecutionStatus"},
                    "chain": {"$ref": "#/components/schemas/Chain"},
                    "from_token": {"type": "string"},
                    "to_token": {"type": "string"},
                    "amount_in": {"type": "string"},
                    "tx_hash": {"type": "string", "nullable": true}
                }
            },
            "SwapExecutionStatus": {
                "type": "string",
                "enum": ["pending", "ready_to_sign", "broadcast", "confirmed", "failed"],
                "description": "Swap execution lifecycle state"
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

fn build_tags() -> Value {
    json!([
        {"name": "health", "description": "Health checks and readiness probes"},
        {"name": "core", "description": "Core custody operations: deposit addresses and withdrawals"},
        {"name": "swaps", "description": "Token swap execution via DEX aggregators (UniswapV3, Jupiter)"},
        {"name": "vaults", "description": "Vault and vault account management"},
        {"name": "transactions", "description": "Transaction history and lifecycle management"},
        {"name": "transfers", "description": "Internal vault-to-vault transfers"},
        {"name": "assets", "description": "Asset and token registry management"},
        {"name": "webhooks", "description": "Webhook endpoint configuration"},
        {"name": "address_book", "description": "Saved destination address management"},
        {"name": "audit", "description": "Immutable audit trail query"},
        {"name": "connections", "description": "Exchange connection management"},
        {"name": "api_keys", "description": "API key lifecycle management"},
        {"name": "notifications", "description": "Notification channel configuration"},
        {"name": "system", "description": "System configuration and operational endpoints"},
        {"name": "workspaces", "description": "Multi-tenant workspace management"},
        {"name": "addresses", "description": "Deposit address management"},
        {"name": "reports", "description": "Financial and compliance reporting"},
        {"name": "compliance", "description": "Travel Rule, AML screening"},
        {"name": "policies", "description": "Vault spending policies"},
        {"name": "approvals", "description": "Multi-party approval workflows"},
        {"name": "networks", "description": "Supported blockchain network information"},
        {"name": "staking", "description": "On-chain staking operations"},
        {"name": "scheduling", "description": "Scheduled transaction management"},
        {"name": "operations", "description": "Operational and monitoring endpoints"},
        {"name": "events", "description": "Real-time WebSocket event streaming"},
        {"name": "auth", "description": "Authentication and session management"},
        {"name": "fido2", "description": "FIDO2/WebAuthn multi-factor authentication"},
        {"name": "governance", "description": "On-chain governance operations"},
        {"name": "platform", "description": "Platform-level administration"},
        {"name": "settlement", "description": "Settlement operations"},
        {"name": "embedded", "description": "Embedded wallet operations"},
        {"name": "vault_ops", "description": "Vault operational tools"},
        {"name": "connectivity", "description": "Node connectivity management"}
    ])
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn add_path(paths: &mut serde_json::Map<String, Value>, path: &str, method: &str, op: Value) {
    let entry = paths
        .entry(path.to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Value::Object(ref mut obj) = *entry {
        obj.insert(method.to_string(), op);
    }
}

fn path_param(name: &str, description: &str, ty: &str, format: &str) -> Value {
    if format.is_empty() {
        json!({
            "name": name,
            "in": "path",
            "required": true,
            "description": description,
            "schema": {"type": ty}
        })
    } else {
        json!({
            "name": name,
            "in": "path",
            "required": true,
            "description": description,
            "schema": {"type": ty, "format": format}
        })
    }
}

/// Build a generic operation entry for endpoints from the developer catalog.
fn build_generic_operation(ep: &custody_common::developer::ApiEndpoint) -> Value {
    let security = if ep.auth_required {
        json!([{"bearerAuth": []}, {"hmacAuth": []}])
    } else {
        json!([])
    };

    // Extract path parameters from {placeholder} segments
    let params: Vec<Value> = ep.path
        .split('/')
        .filter(|seg| seg.starts_with('{') && seg.ends_with('}'))
        .map(|seg| {
            let name = seg.trim_matches(|c| c == '{' || c == '}');
            path_param(name, &format!("{} identifier", name), "string", "")
        })
        .collect();

    let mut op = serde_json::Map::new();
    op.insert("summary".into(), json!(ep.summary));
    op.insert("tags".into(), json!([ep.tag]));
    op.insert("security".into(), security);
    op.insert("responses".into(), json!({
        "200": {"description": "Success"},
        "400": {"$ref": "#/components/responses/BadRequest"},
        "401": {"$ref": "#/components/responses/Unauthorized"},
        "403": {"$ref": "#/components/responses/Forbidden"},
        "404": {"$ref": "#/components/responses/NotFound"},
        "500": {"$ref": "#/components/responses/InternalError"}
    }));

    if let Some(rate) = ep.rate_limit {
        op.insert("x-rate-limit".into(), json!(rate));
    }

    if !params.is_empty() {
        op.insert("parameters".into(), json!(params));
    }

    Value::Object(op)
}

/// Endpoints from the developer catalog that are NOT already explicitly defined above.
fn remaining_catalog_entries() -> Vec<custody_common::developer::ApiEndpoint> {
    // Paths already covered with full schemas in build_paths()
    let covered = [
        "/health",
        "/health/chains",
        "/health/deep",
        "/metrics",
        "/v1/addresses",
        "/v1/withdrawals",
        "/v1/withdrawals/{id}",
        "/v1/withdrawals/{id}/approve",
        "/v1/wallets/{asset}",
        "/v1/swaps/quote",
        "/v1/swaps",
        "/v1/swaps/{id}",
    ];

    custody_common::developer::all_endpoints()
        .into_iter()
        .filter(|ep| !covered.contains(&ep.path.as_str()))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_version_is_3_1() {
        let spec = generate_openapi_v3();
        assert_eq!(spec["openapi"], "3.1.0");
    }

    #[test]
    fn spec_has_info_block() {
        let spec = generate_openapi_v3();
        assert_eq!(spec["info"]["title"], "MG Custody API");
        assert!(spec["info"]["description"].as_str().unwrap().contains("custody"));
        assert!(!spec["info"]["version"].as_str().unwrap().is_empty());
    }

    #[test]
    fn spec_has_servers() {
        let spec = generate_openapi_v3();
        let servers = spec["servers"].as_array().unwrap();
        assert!(servers.len() >= 2);
        assert!(servers[0]["url"].as_str().unwrap().contains("localhost"));
    }

    #[test]
    fn spec_has_both_security_schemes() {
        let spec = generate_openapi_v3();
        let schemes = spec["components"]["securitySchemes"].as_object().unwrap();
        assert!(schemes.contains_key("bearerAuth"), "Missing bearerAuth scheme");
        assert!(schemes.contains_key("hmacAuth"), "Missing hmacAuth scheme");
    }

    #[test]
    fn spec_bearer_auth_is_http_bearer() {
        let spec = generate_openapi_v3();
        let bearer = &spec["components"]["securitySchemes"]["bearerAuth"];
        assert_eq!(bearer["type"], "http");
        assert_eq!(bearer["scheme"], "bearer");
    }

    #[test]
    fn spec_hmac_auth_is_api_key_in_header() {
        let spec = generate_openapi_v3();
        let hmac = &spec["components"]["securitySchemes"]["hmacAuth"];
        assert_eq!(hmac["type"], "apiKey");
        assert_eq!(hmac["in"], "header");
    }

    #[test]
    fn spec_contains_health_endpoint() {
        let spec = generate_openapi_v3();
        let paths = spec["paths"].as_object().unwrap();
        assert!(paths.contains_key("/health"), "Missing /health path");
        assert!(paths["/health"]["get"]["security"].as_array().unwrap().is_empty(),
            "Health endpoint should not require auth");
    }

    #[test]
    fn spec_contains_swap_endpoints() {
        let spec = generate_openapi_v3();
        let paths = spec["paths"].as_object().unwrap();
        assert!(paths.contains_key("/v1/swaps/quote"), "Missing /v1/swaps/quote");
        assert!(paths.contains_key("/v1/swaps"), "Missing /v1/swaps");
        assert!(paths.contains_key("/v1/swaps/{id}"), "Missing /v1/swaps/{{id}}");
    }

    #[test]
    fn swap_quote_endpoint_has_post() {
        let spec = generate_openapi_v3();
        let quote_path = &spec["paths"]["/v1/swaps/quote"];
        assert!(!quote_path["post"].is_null(), "/v1/swaps/quote must have POST");
    }

    #[test]
    fn swap_execute_endpoint_has_post() {
        let spec = generate_openapi_v3();
        let path = &spec["paths"]["/v1/swaps"];
        assert!(!path["post"].is_null(), "/v1/swaps must have POST");
    }

    #[test]
    fn spec_contains_core_endpoints() {
        let spec = generate_openapi_v3();
        let paths = spec["paths"].as_object().unwrap();
        assert!(paths.contains_key("/v1/addresses"));
        assert!(paths.contains_key("/v1/withdrawals"));
        assert!(paths.contains_key("/v1/withdrawals/{id}"));
        assert!(paths.contains_key("/v1/wallets/{asset}"));
    }

    #[test]
    fn spec_has_common_error_response_refs() {
        let spec = generate_openapi_v3();
        let responses = spec["components"]["responses"].as_object().unwrap();
        assert!(responses.contains_key("BadRequest"));
        assert!(responses.contains_key("Unauthorized"));
        assert!(responses.contains_key("Forbidden"));
        assert!(responses.contains_key("NotFound"));
        assert!(responses.contains_key("InternalError"));
    }

    #[test]
    fn spec_has_core_schemas() {
        let spec = generate_openapi_v3();
        let schemas = spec["components"]["schemas"].as_object().unwrap();
        assert!(schemas.contains_key("ApiError"));
        assert!(schemas.contains_key("Chain"));
        assert!(schemas.contains_key("WithdrawalStatus"));
        assert!(schemas.contains_key("CreateAddressRequest"));
        assert!(schemas.contains_key("SubmitWithdrawalRequest"));
        assert!(schemas.contains_key("WalletBalanceResponse"));
    }

    #[test]
    fn spec_has_swap_schemas() {
        let spec = generate_openapi_v3();
        let schemas = spec["components"]["schemas"].as_object().unwrap();
        assert!(schemas.contains_key("SwapQuoteRequest"), "Missing SwapQuoteRequest schema");
        assert!(schemas.contains_key("SwapQuote"), "Missing SwapQuote schema");
        assert!(schemas.contains_key("SwapExecuteRequest"), "Missing SwapExecuteRequest schema");
        assert!(schemas.contains_key("SwapExecuteResponse"), "Missing SwapExecuteResponse schema");
        assert!(schemas.contains_key("SwapStatusResponse"), "Missing SwapStatusResponse schema");
        assert!(schemas.contains_key("SwapExecutionStatus"), "Missing SwapExecutionStatus schema");
    }

    #[test]
    fn swap_quote_schema_has_correct_slippage_bounds() {
        let spec = generate_openapi_v3();
        let schema = &spec["components"]["schemas"]["SwapQuoteRequest"];
        let slippage = &schema["properties"]["slippage_bps"];
        assert_eq!(slippage["minimum"], 1);
        assert_eq!(slippage["maximum"], 200);
    }

    #[test]
    fn chain_schema_has_all_chains() {
        let spec = generate_openapi_v3();
        let chain_enum = spec["components"]["schemas"]["Chain"]["enum"].as_array().unwrap();
        let chains: Vec<&str> = chain_enum.iter().map(|v| v.as_str().unwrap()).collect();
        // Chain serializes as lowercase per serde(rename_all = "lowercase")
        assert!(chains.contains(&"ethereum"));
        assert!(chains.contains(&"bitcoin"));
        assert!(chains.contains(&"tron"));
        assert!(chains.contains(&"solana"));
    }

    #[test]
    fn spec_has_tags_including_swaps() {
        let spec = generate_openapi_v3();
        let tags = spec["tags"].as_array().unwrap();
        let tag_names: Vec<&str> = tags.iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(tag_names.contains(&"swaps"), "Missing 'swaps' tag");
        assert!(tag_names.contains(&"health"));
        assert!(tag_names.contains(&"core"));
        assert!(tag_names.contains(&"vaults"));
    }

    #[test]
    fn spec_total_paths_count() {
        let spec = generate_openapi_v3();
        let paths = spec["paths"].as_object().unwrap();
        // Core (5 paths) + swaps (3) + catalog (many) = well over 50
        assert!(paths.len() >= 50,
            "Expected 50+ paths in spec, got {}", paths.len());
    }

    #[test]
    fn spec_is_valid_json() {
        let spec = generate_openapi_v3();
        // Round-trip through JSON serialization
        let json_str = serde_json::to_string(&spec).unwrap();
        let _reparsed: Value = serde_json::from_str(&json_str).unwrap();
    }

    #[test]
    fn amounts_described_as_string_not_number() {
        let spec = generate_openapi_v3();
        // amount_in in SwapQuoteRequest must be type string (not number) — no floats
        let amount_type = &spec["components"]["schemas"]["SwapQuoteRequest"]
            ["properties"]["amount_in"]["type"];
        assert_eq!(amount_type, "string",
            "amount_in must be described as string to prevent float JSON deserialization");
    }

    #[test]
    fn withdrawal_amount_described_as_string() {
        let spec = generate_openapi_v3();
        let amount_type = &spec["components"]["schemas"]["SubmitWithdrawalRequest"]
            ["properties"]["amount"]["type"];
        assert_eq!(amount_type, "string",
            "withdrawal amount must be string type in schema");
    }

    #[test]
    fn all_authenticated_endpoints_have_security() {
        let spec = generate_openapi_v3();
        let paths = spec["paths"].as_object().unwrap();
        // Check a known authenticated endpoint
        let withdrawal_post = &paths["/v1/withdrawals"]["post"];
        let security = withdrawal_post["security"].as_array().unwrap();
        assert!(!security.is_empty(), "POST /v1/withdrawals should require auth");
    }

    #[test]
    fn spec_global_security_is_set() {
        let spec = generate_openapi_v3();
        let global_security = spec["security"].as_array().unwrap();
        assert!(!global_security.is_empty(), "Global security must be set");
    }
}
