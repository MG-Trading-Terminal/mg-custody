//! Solana Actions & Blinks — shareable transaction links.

use serde::{Deserialize, Serialize};

/// Solana Action metadata (following Solana Actions spec).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionMetadata {
    pub icon: String,
    pub title: String,
    pub description: String,
    pub label: String,
    pub links: Option<ActionLinks>,
    pub disabled: Option<bool>,
    pub error: Option<ActionError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionLinks {
    pub actions: Vec<LinkedAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedAction {
    pub label: String,
    pub href: String,
    pub parameters: Option<Vec<ActionParameter>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionParameter {
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    pub param_type: Option<String>,
    pub required: Option<bool>,
    pub pattern: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionError {
    pub message: String,
}

/// Action POST request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPostRequest {
    pub account: String,
}

/// Action POST response — returns a transaction to sign.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPostResponse {
    pub transaction: String, // base64 encoded
    pub message: Option<String>,
}

/// Known Solana Action providers.
pub fn known_action_providers() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Jupiter Swap", "https://jup.ag/swap"),
        ("Marinade Stake", "https://marinade.finance/stake"),
        ("Tensor NFT", "https://www.tensor.trade"),
        ("Raydium LP", "https://raydium.io"),
        ("Drift Trade", "https://app.drift.trade"),
    ]
}

/// Blink URL builder.
pub fn build_blink_url(action_url: &str) -> String {
    format!("https://dial.to/?action=solana-action:{}", urlencoding(action_url))
}

fn urlencoding(s: &str) -> String {
    s.replace(':', "%3A").replace('/', "%2F").replace('?', "%3F").replace('&', "%26")
}

/// Validate an action URL.
pub fn validate_action_url(url: &str) -> bool {
    url.starts_with("https://") && url.contains("/api/actions/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_metadata_serialization() {
        let meta = ActionMetadata {
            icon: "https://example.com/icon.png".into(),
            title: "Stake SOL".into(), description: "Stake with Marinade".into(),
            label: "Stake".into(), links: None, disabled: None, error: None,
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("Stake SOL"));
    }

    #[test]
    fn action_with_parameters() {
        let action = LinkedAction {
            label: "Swap".into(), href: "/api/actions/swap?amount={amount}".into(),
            parameters: Some(vec![ActionParameter {
                name: "amount".into(), label: "SOL Amount".into(),
                param_type: Some("number".into()), required: Some(true),
                pattern: None, min: Some(0.01), max: Some(1000.0),
            }]),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("amount"));
    }

    #[test]
    fn blink_url() {
        let url = build_blink_url("https://example.com/api/actions/stake");
        assert!(url.starts_with("https://dial.to/"));
        assert!(url.contains("solana-action"));
    }

    #[test]
    fn validate_action_url_valid() {
        assert!(validate_action_url("https://app.example.com/api/actions/stake"));
    }

    #[test]
    fn validate_action_url_invalid() {
        assert!(!validate_action_url("http://insecure.com/api/actions/stake")); // no https
        assert!(!validate_action_url("https://example.com/other")); // no /api/actions/
    }

    #[test]
    fn known_providers() {
        let providers = known_action_providers();
        assert!(providers.len() >= 5);
    }

    #[test]
    fn action_post_response() {
        let resp = ActionPostResponse {
            transaction: "base64tx==".into(),
            message: Some("Stake 1 SOL with Marinade".into()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("base64tx"));
    }
}
