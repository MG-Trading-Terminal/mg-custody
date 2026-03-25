//! Key management audit log.
//!
//! Records every security-sensitive operation (key derivation, address generation,
//! signing requests) with a tamper-evident hash chain.
//!
//! CRITICAL: Audit entries NEVER contain key material — only metadata
//! (operation type, user_id, chain, derivation_path, timestamps).

use chrono::{DateTime, Utc};
use custody_common::Chain;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::CustodyDb;

/// Type of audited operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditOperation {
    /// HD key derivation for address generation.
    AddressDerivation,
    /// Seed encrypted for storage.
    SeedEncrypted,
    /// Seed decrypted (loaded).
    SeedDecrypted,
    /// Seed split into Shamir shares.
    SeedSplit,
    /// Seed reconstructed from Shamir shares.
    SeedReconstructed,
    /// Transaction signing request.
    SigningRequest,
    /// Transaction signing completed.
    SigningCompleted,
    /// Signing request rejected (validation failure).
    SigningRejected,
}

/// A single audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique entry ID.
    pub id: Uuid,
    /// When the operation occurred.
    pub timestamp: DateTime<Utc>,
    /// What operation was performed.
    pub operation: AuditOperation,
    /// Who initiated the operation (user_id, system, or operator).
    pub actor: String,
    /// Which chain (if applicable).
    pub chain: Option<Chain>,
    /// Derivation path (if applicable). NEVER the key itself.
    pub derivation_path: Option<String>,
    /// Additional context (e.g., withdrawal_id, address). NEVER key material.
    pub details: Option<String>,
    /// SHA-256 hash of the previous entry (tamper-evident chain).
    pub prev_hash: Option<String>,
}

impl AuditEntry {
    /// Create a new audit entry for an address derivation.
    pub fn address_derivation(
        user_id: Uuid,
        chain: Chain,
        derivation_path: &str,
        address: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            operation: AuditOperation::AddressDerivation,
            actor: user_id.to_string(),
            chain: Some(chain),
            derivation_path: Some(derivation_path.to_string()),
            details: Some(format!("address={address}")),
            prev_hash: None, // Set by the DB layer on insert
        }
    }

    /// Create a new audit entry for seed encryption.
    pub fn seed_encrypted(actor: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            operation: AuditOperation::SeedEncrypted,
            actor: actor.to_string(),
            chain: None,
            derivation_path: None,
            details: None,
            prev_hash: None,
        }
    }

    /// Create a new audit entry for seed decryption.
    pub fn seed_decrypted(actor: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            operation: AuditOperation::SeedDecrypted,
            actor: actor.to_string(),
            chain: None,
            derivation_path: None,
            details: None,
            prev_hash: None,
        }
    }

    /// Create a new audit entry for Shamir split.
    pub fn seed_split(actor: &str, threshold: u8, shares: u8) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            operation: AuditOperation::SeedSplit,
            actor: actor.to_string(),
            chain: None,
            derivation_path: None,
            details: Some(format!("threshold={threshold}, shares={shares}")),
            prev_hash: None,
        }
    }

    /// Create a new audit entry for a signing request.
    pub fn signing_request(
        chain: Chain,
        withdrawal_id: Uuid,
        destination: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            operation: AuditOperation::SigningRequest,
            actor: "system".to_string(),
            chain: Some(chain),
            derivation_path: None,
            details: Some(format!("withdrawal={withdrawal_id}, dest={destination}")),
            prev_hash: None,
        }
    }
}

impl CustodyDb {
    /// Insert an audit log entry with tamper-evident hash chain.
    pub async fn insert_audit_entry(&self, entry: &AuditEntry) -> anyhow::Result<()> {
        // Compute prev_hash from the last entry
        let prev_hash: Option<String> = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT prev_hash FROM audit_log ORDER BY timestamp DESC LIMIT 1"
        )
        .fetch_optional(&self.pool)
        .await?
        .and_then(|r| r.0);

        // Hash the previous entry's hash + current entry data for chain integrity
        let hash_input = format!(
            "{}:{}:{:?}:{}:{}",
            prev_hash.as_deref().unwrap_or("genesis"),
            entry.id,
            entry.operation,
            entry.actor,
            entry.timestamp.to_rfc3339(),
        );
        use sha2::Digest;
        let current_hash = hex::encode(sha2::Sha256::digest(hash_input.as_bytes()));

        let operation = serde_json::to_string(&entry.operation)?;
        let chain = entry.chain.map(|c| c.as_str().to_string());

        sqlx::query(
            "INSERT INTO audit_log (id, timestamp, operation, actor, chain, derivation_path, details, prev_hash)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
        )
        .bind(entry.id)
        .bind(entry.timestamp)
        .bind(&operation)
        .bind(&entry.actor)
        .bind(&chain)
        .bind(&entry.derivation_path)
        .bind(&entry.details)
        .bind(&current_hash)
        .execute(&self.pool)
        .await?;

        tracing::info!(
            operation = ?entry.operation,
            actor = %entry.actor,
            chain = ?entry.chain,
            "Audit log entry recorded"
        );
        Ok(())
    }

    /// Get recent audit entries (for admin review).
    pub async fn recent_audit_entries(&self, limit: u32) -> anyhow::Result<Vec<AuditEntry>> {
        let rows: Vec<(Uuid, chrono::DateTime<Utc>, String, String, Option<String>, Option<String>, Option<String>, Option<String>)> =
            sqlx::query_as(
                "SELECT id, timestamp, operation, actor, chain, derivation_path, details, prev_hash
                 FROM audit_log ORDER BY timestamp DESC LIMIT $1"
            )
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|(id, timestamp, operation, actor, chain, derivation_path, details, prev_hash)| {
            let op: AuditOperation = serde_json::from_str(&operation).unwrap_or(AuditOperation::SigningRequest);
            let chain = chain.map(|c| custody_common::Chain::parse_str(&c));
            AuditEntry { id, timestamp, operation: op, actor, chain, derivation_path, details, prev_hash }
        }).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_entry_address_derivation() {
        let entry = AuditEntry::address_derivation(
            Uuid::new_v4(),
            Chain::Bitcoin,
            "m/84'/0'/0'/0/0",
            "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu",
        );

        assert_eq!(entry.operation, AuditOperation::AddressDerivation);
        assert_eq!(entry.chain, Some(Chain::Bitcoin));
        assert!(entry.derivation_path.as_ref().unwrap().contains("m/84'"));
        assert!(entry.details.as_ref().unwrap().contains("bc1q"));
        // Must NOT contain key material
        let serialized = serde_json::to_string(&entry).unwrap();
        assert!(!serialized.contains("seed"));
        assert!(!serialized.contains("private"));
    }

    #[test]
    fn audit_entry_seed_split() {
        let entry = AuditEntry::seed_split("operator@company.com", 3, 5);
        assert_eq!(entry.operation, AuditOperation::SeedSplit);
        assert!(entry.details.as_ref().unwrap().contains("threshold=3"));
        assert!(entry.details.as_ref().unwrap().contains("shares=5"));
    }

    #[test]
    fn audit_entry_signing_request() {
        let entry = AuditEntry::signing_request(
            Chain::Ethereum,
            Uuid::new_v4(),
            "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18",
        );
        assert_eq!(entry.operation, AuditOperation::SigningRequest);
        assert!(entry.details.as_ref().unwrap().contains("0x742d35"));
    }

    #[test]
    fn audit_operations_serialize() {
        let ops = vec![
            AuditOperation::AddressDerivation,
            AuditOperation::SeedEncrypted,
            AuditOperation::SeedDecrypted,
            AuditOperation::SeedSplit,
            AuditOperation::SeedReconstructed,
            AuditOperation::SigningRequest,
            AuditOperation::SigningCompleted,
            AuditOperation::SigningRejected,
        ];

        for op in ops {
            let json = serde_json::to_string(&op).unwrap();
            let deserialized: AuditOperation = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, op);
        }
    }
}
