//! SDK generation types and API client specs.
//!
//! Defines the interface that generated SDKs (Python, TypeScript, Go, Rust)
//! should implement. The actual SDK generation happens via OpenAPI codegen.

use serde::{Deserialize, Serialize};

/// SDK configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkConfig {
    pub api_base_url: String,
    pub api_key: String,
    pub hmac_secret: String,
    pub timeout_secs: u64,
    pub max_retries: u32,
}

impl Default for SdkConfig {
    fn default() -> Self {
        Self {
            api_base_url: "http://localhost:8443".into(),
            api_key: String::new(),
            hmac_secret: String::new(),
            timeout_secs: 30,
            max_retries: 3,
        }
    }
}

/// SDK method signatures (what each language SDK must implement).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkMethod {
    pub name: String,
    pub http_method: String,
    pub path: String,
    pub description: String,
    pub request_type: Option<String>,
    pub response_type: String,
}

/// Generate SDK method definitions from our API.
pub fn sdk_methods() -> Vec<SdkMethod> {
    vec![
        SdkMethod {
            name: "create_address".into(), http_method: "POST".into(),
            path: "/v1/addresses".into(), description: "Generate a deposit address".into(),
            request_type: Some("CreateAddressRequest".into()), response_type: "CreateAddressResponse".into(),
        },
        SdkMethod {
            name: "submit_withdrawal".into(), http_method: "POST".into(),
            path: "/v1/withdrawals".into(), description: "Submit a withdrawal request".into(),
            request_type: Some("SubmitWithdrawalRequest".into()), response_type: "SubmitWithdrawalResponse".into(),
        },
        SdkMethod {
            name: "get_withdrawal".into(), http_method: "GET".into(),
            path: "/v1/withdrawals/{id}".into(), description: "Check withdrawal status".into(),
            request_type: None, response_type: "WithdrawalStatusResponse".into(),
        },
        SdkMethod {
            name: "approve_withdrawal".into(), http_method: "POST".into(),
            path: "/v1/withdrawals/{id}/approve".into(), description: "Approve or reject a withdrawal".into(),
            request_type: Some("ApprovalDecision".into()), response_type: "ApprovalResult".into(),
        },
        SdkMethod {
            name: "get_balance".into(), http_method: "GET".into(),
            path: "/v1/wallets/{asset}".into(), description: "Get wallet balance".into(),
            request_type: None, response_type: "WalletBalanceResponse".into(),
        },
        SdkMethod {
            name: "health".into(), http_method: "GET".into(),
            path: "/health".into(), description: "Health check".into(),
            request_type: None, response_type: "String".into(),
        },
        SdkMethod {
            name: "chain_health".into(), http_method: "GET".into(),
            path: "/health/chains".into(), description: "Per-chain health status".into(),
            request_type: None, response_type: "ChainHealthResponse".into(),
        },
    ]
}

/// Generate a Python SDK stub.
pub fn generate_python_stub() -> String {
    let mut code = String::new();
    code.push_str("# Auto-generated MG Custody Python SDK\n");
    code.push_str("# pip install requests\n\n");
    code.push_str("import hmac, hashlib, time, json, requests\n\n");
    code.push_str("class CustodyClient:\n");
    code.push_str("    def __init__(self, base_url, api_key, hmac_secret):\n");
    code.push_str("        self.base_url = base_url\n");
    code.push_str("        self.api_key = api_key\n");
    code.push_str("        self.hmac_secret = hmac_secret\n\n");

    for method in sdk_methods() {
        code.push_str(&format!("    def {}(self, **kwargs):\n", method.name));
        code.push_str(&format!("        \"\"\"{}. {} {}\"\"\"\n", method.description, method.http_method, method.path));
        if method.http_method == "GET" {
            code.push_str(&format!("        return self._get('{}')\n\n", method.path));
        } else {
            code.push_str(&format!("        return self._post('{}', kwargs)\n\n", method.path));
        }
    }

    code.push_str("    def _sign(self, method, path, body):\n");
    code.push_str("        ts = str(int(time.time()))\n");
    code.push_str("        msg = method + path + ts + body\n");
    code.push_str("        sig = hmac.new(self.hmac_secret.encode(), msg.encode(), hashlib.sha256).hexdigest()\n");
    code.push_str("        return ts, sig\n");
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_methods_complete() {
        let methods = sdk_methods();
        assert!(methods.len() >= 7);
        assert!(methods.iter().any(|m| m.name == "create_address"));
        assert!(methods.iter().any(|m| m.name == "submit_withdrawal"));
        assert!(methods.iter().any(|m| m.name == "approve_withdrawal"));
    }

    #[test]
    fn python_stub_generation() {
        let stub = generate_python_stub();
        assert!(stub.contains("class CustodyClient"));
        assert!(stub.contains("def create_address"));
        assert!(stub.contains("def submit_withdrawal"));
        assert!(stub.contains("hmac"));
    }

    #[test]
    fn sdk_config_default() {
        let config = SdkConfig::default();
        assert_eq!(config.api_base_url, "http://localhost:8443");
        assert_eq!(config.timeout_secs, 30);
    }
}
