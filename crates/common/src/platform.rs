//! Platform types — workspaces, users, reporting, travel rule, key info.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Asset, Chain};

// ---------------------------------------------------------------------------
// Workspace / user management
// ---------------------------------------------------------------------------

/// A workspace represents an isolated tenant environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDetail {
    pub id: Uuid,
    pub name: String,
    pub environment: WorkspaceEnv,
    pub owner_email: String,
    pub vault_count: u32,
    pub api_key_count: u32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceEnv {
    Sandbox,
    Testnet,
    Mainnet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub environment: WorkspaceEnv,
    pub owner_email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
    pub enabled: Option<bool>,
}

/// A user within a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceUser {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub email: String,
    pub role: UserRole,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Owner,
    Admin,
    Signer,
    Viewer,
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Owner => f.write_str("owner"),
            Self::Admin => f.write_str("admin"),
            Self::Signer => f.write_str("signer"),
            Self::Viewer => f.write_str("viewer"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub email: String,
    pub role: UserRole,
}

// ---------------------------------------------------------------------------
// Deposit address → vault binding
// ---------------------------------------------------------------------------

/// Binds a deposit address to a specific vault account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositAddressBinding {
    pub address: String,
    pub chain: Chain,
    pub vault_id: Uuid,
    pub vault_account_id: Option<Uuid>,
    pub label: Option<String>,
    pub user_id: Uuid,
    pub derivation_path: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindAddressRequest {
    pub vault_id: Uuid,
    pub label: Option<String>,
}

// ---------------------------------------------------------------------------
// Reporting & export
// ---------------------------------------------------------------------------

/// Report generation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportRequest {
    pub report_type: ReportType,
    pub format: ReportFormat,
    pub date_from: DateTime<Utc>,
    pub date_to: DateTime<Utc>,
    pub chain: Option<Chain>,
    pub asset: Option<String>,
    pub vault_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportType {
    Transactions,
    Balances,
    Deposits,
    Withdrawals,
    AuditTrail,
    Reconciliation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportFormat {
    Json,
    Csv,
}

/// Generated report metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportResult {
    pub id: Uuid,
    pub report_type: ReportType,
    pub format: ReportFormat,
    pub row_count: usize,
    pub generated_at: DateTime<Utc>,
    /// For CSV: the actual CSV content. For JSON: serialized array.
    pub data: String,
}

// ---------------------------------------------------------------------------
// Signing key info (public only — NEVER private keys)
// ---------------------------------------------------------------------------

/// Public key information for a derived signing key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningKeyInfo {
    pub chain: Chain,
    pub algorithm: String,
    pub derivation_path: String,
    /// Hex-encoded public key.
    pub public_key: String,
    /// Chain-specific address derived from this key.
    pub address: String,
    pub key_type: KeyType,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyType {
    Software,
    Hsm,
    Mpc,
}

/// Request key info for a vault account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyInfoRequest {
    pub vault_id: Uuid,
    pub chain: Chain,
}

// ---------------------------------------------------------------------------
// Travel rule data (FATF Recommendation 16)
// ---------------------------------------------------------------------------

/// Originator/beneficiary information for FATF Travel Rule compliance.
///
/// Required for transfers above thresholds (EU: €1,000 / US: $3,000).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TravelRuleData {
    pub id: Uuid,
    pub withdrawal_id: Uuid,
    pub originator: TravelRuleParty,
    pub beneficiary: TravelRuleParty,
    pub amount: Decimal,
    pub asset: Asset,
    pub chain: Chain,
    pub threshold_currency: String,
    pub threshold_amount: Decimal,
    pub status: TravelRuleStatus,
    /// 21 Travel Rule record ID returned after successful TRUST dispatch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_record_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Party information for travel rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TravelRuleParty {
    pub name: String,
    pub account_number: Option<String>,
    /// VASP (Virtual Asset Service Provider) DID or LEI.
    pub vasp_id: Option<String>,
    pub address: Option<PhysicalAddress>,
    pub country: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicalAddress {
    pub street: String,
    pub city: String,
    pub state: Option<String>,
    pub postal_code: String,
    pub country: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TravelRuleStatus {
    Required,
    Dispatching,
    Submitted,
    Verified,
    Exempted,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitTravelRuleRequest {
    pub withdrawal_id: Uuid,
    pub originator: TravelRuleParty,
    pub beneficiary: TravelRuleParty,
    /// TRUST identity fields. Custody supplies the asset, amount and transaction ID.
    #[serde(default)]
    pub trust: Option<TrustParties>,
}

/// Identity details from the 21 Travel Rule Address Book and the withdrawal request.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustParties {
    pub originator: TrustOriginator,
    pub beneficiary_name: TrustBeneficiaryName,
}

/// Registered 21 Travel Rule originator for the asset being withdrawn.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustOriginator {
    pub id: String,
    pub address: TrustAddress,
    pub customer_identification: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legal_id: Option<String>,
}

/// Postal address in the TRUST GraphQL input format.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustAddress {
    pub address_line: String,
    pub postal_code: String,
    pub town: String,
    pub country: String,
}

/// TRUST beneficiary name with exactly one of the accepted IVMS name types.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum TrustBeneficiaryName {
    Natural {
        #[serde(rename = "firstName")]
        first_name: String,
        #[serde(rename = "lastName")]
        last_name: String,
    },
    Legal { name: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_env_serialization() {
        assert_eq!(serde_json::to_string(&WorkspaceEnv::Mainnet).unwrap(), r#""mainnet""#);
        assert_eq!(serde_json::to_string(&WorkspaceEnv::Sandbox).unwrap(), r#""sandbox""#);
    }

    #[test]
    fn user_role_display() {
        assert_eq!(UserRole::Owner.to_string(), "owner");
        assert_eq!(UserRole::Signer.to_string(), "signer");
    }

    #[test]
    fn report_type_serialization() {
        let json = serde_json::to_string(&ReportType::Transactions).unwrap();
        assert_eq!(json, r#""transactions""#);
        let json = serde_json::to_string(&ReportType::AuditTrail).unwrap();
        assert_eq!(json, r#""audit_trail""#);
    }

    #[test]
    fn report_format_variants() {
        assert_eq!(serde_json::to_string(&ReportFormat::Csv).unwrap(), r#""csv""#);
        assert_eq!(serde_json::to_string(&ReportFormat::Json).unwrap(), r#""json""#);
    }

    #[test]
    fn key_type_serialization() {
        assert_eq!(serde_json::to_string(&KeyType::Mpc).unwrap(), r#""mpc""#);
        assert_eq!(serde_json::to_string(&KeyType::Hsm).unwrap(), r#""hsm""#);
    }

    #[test]
    fn signing_key_info_structure() {
        let info = SigningKeyInfo {
            chain: Chain::Ethereum,
            algorithm: "secp256k1".into(),
            derivation_path: "m/44'/60'/0'/0/0".into(),
            public_key: "02abc123".into(),
            address: "0x1234".into(),
            key_type: KeyType::Software,
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("secp256k1"));
        assert!(json.contains("m/44'"));
    }

    #[test]
    fn travel_rule_status() {
        let json = serde_json::to_string(&TravelRuleStatus::Required).unwrap();
        assert_eq!(json, r#""required""#);
        let json = serde_json::to_string(&TravelRuleStatus::Verified).unwrap();
        assert_eq!(json, r#""verified""#);
    }

    #[test]
    fn travel_rule_party() {
        let party = TravelRuleParty {
            name: "Alice Corp".into(),
            account_number: Some("ACC-001".into()),
            vasp_id: Some("did:web:alice.vasp".into()),
            address: Some(PhysicalAddress {
                street: "123 Main St".into(),
                city: "Zurich".into(),
                state: None,
                postal_code: "8001".into(),
                country: "CH".into(),
            }),
            country: Some("CH".into()),
        };
        let json = serde_json::to_string(&party).unwrap();
        assert!(json.contains("Alice Corp"));
        assert!(json.contains("did:web"));
    }

    #[test]
    fn deposit_address_binding() {
        let binding = DepositAddressBinding {
            address: "0xabc".into(),
            chain: Chain::Ethereum,
            vault_id: Uuid::nil(),
            vault_account_id: None,
            label: Some("Cold storage".into()),
            user_id: Uuid::nil(),
            derivation_path: "m/44'/60'/0'/0/0".into(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&binding).unwrap();
        assert!(json.contains("Cold storage"));
    }

    #[test]
    fn create_workspace_request() {
        let json = r#"{"name":"Acme Corp","environment":"mainnet","owner_email":"admin@acme.com"}"#;
        let req: CreateWorkspaceRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "Acme Corp");
        assert_eq!(req.environment, WorkspaceEnv::Mainnet);
    }

    #[test]
    fn report_request_structure() {
        let req = ReportRequest {
            report_type: ReportType::Transactions,
            format: ReportFormat::Csv,
            date_from: Utc::now(),
            date_to: Utc::now(),
            chain: Some(Chain::Ethereum),
            asset: Some("ETH".into()),
            vault_id: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("csv"));
        assert!(json.contains("transactions"));
    }
}
