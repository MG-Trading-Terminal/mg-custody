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
    /// Emergency freeze activated by operator.
    EmergencyFreeze,
    /// Emergency freeze lifted by operator.
    EmergencyUnfreeze,
    /// Withdrawal state overridden by operator (force-complete or force-cancel).
    WithdrawalOverride,
    /// RBAC permission denied (user tried to access resource above their role).
    PermissionDenied,
    /// Operator created a readiness alert silence (planned
    /// maintenance window). Alert-suppressing actions must leave a
    /// trail so a postmortem can answer "was the alert suppressed
    /// when it should have fired?".
    ReadinessSilenceCreated,
    /// Operator ended a silence early.
    ReadinessSilenceEnded,
    /// Chaos override injected (alert-pipeline drill).
    ReadinessChaosInjected,
    /// Chaos override cleared.
    ReadinessChaosCleared,
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

    /// Create an audit entry for a readiness silence creation.
    /// `details` carries the feature pattern, duration, and reason —
    /// everything a postmortem needs to understand why alerts were
    /// suppressed during a window.
    pub fn readiness_silence_created(
        actor: &str,
        feature: &str,
        duration_secs: i64,
        reason: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            operation: AuditOperation::ReadinessSilenceCreated,
            actor: actor.to_string(),
            chain: None,
            derivation_path: None,
            details: Some(format!(
                "feature={feature}, duration_secs={duration_secs}, reason={reason}"
            )),
            prev_hash: None,
        }
    }

    /// Audit entry for an early-ended silence.
    pub fn readiness_silence_ended(actor: &str, feature: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            operation: AuditOperation::ReadinessSilenceEnded,
            actor: actor.to_string(),
            chain: None,
            derivation_path: None,
            details: Some(format!("feature={feature}")),
            prev_hash: None,
        }
    }

    /// Audit entry for a chaos override being applied. These are
    /// operational drills, not incidents — but the audit trail
    /// still needs to distinguish "scanner.ethereum was marked
    /// Degraded by the refresh task" from "scanner.ethereum was
    /// degraded by a human, manually".
    pub fn readiness_chaos_injected(
        actor: &str,
        feature: &str,
        forced_state: &str,
        reason: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            operation: AuditOperation::ReadinessChaosInjected,
            actor: actor.to_string(),
            chain: None,
            derivation_path: None,
            details: Some(format!(
                "feature={feature}, forced_state={forced_state}, reason={reason}"
            )),
            prev_hash: None,
        }
    }

    /// Audit entry for a chaos override being cleared.
    pub fn readiness_chaos_cleared(actor: &str, feature: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            operation: AuditOperation::ReadinessChaosCleared,
            actor: actor.to_string(),
            chain: None,
            derivation_path: None,
            details: Some(format!("feature={feature}")),
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

    /// Create a new audit entry for RBAC permission denial.
    pub fn permission_denied(
        actor: &str,
        required_role: &str,
        actual_role: &str,
        method: &str,
        path: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            operation: AuditOperation::PermissionDenied,
            actor: actor.to_string(),
            chain: None,
            derivation_path: None,
            details: Some(format!(
                "required={required_role}, actual={actual_role}, {method} {path}"
            )),
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
    ///
    /// The chain is linear: each entry's `prev_hash` must match
    /// the `prev_hash` of the immediately preceding row. If two
    /// handlers insert concurrently (freeze + approval decision,
    /// signing + RBAC denial, etc.) a naive read-then-write would
    /// let both read the same parent and write siblings — the
    /// chain forks and tamper-evidence is lost: a deleted row
    /// between two siblings is undetectable by chain-walking.
    ///
    /// Serialize with a PostgreSQL transaction-scoped advisory
    /// lock keyed on a constant. Every insert acquires the same
    /// lock, so SELECT-prev + INSERT form an atomic unit. The
    /// advisory lock is scoped to the transaction so it's
    /// released when we commit (or rollback) — no risk of a
    /// permanent lock if the caller panics.
    pub async fn insert_audit_entry(&self, entry: &AuditEntry) -> anyhow::Result<()> {
        // Arbitrary but fixed key so every caller contends on
        // the same lock. 0x_A441_1_06 = "audit log chain head"
        // mnemonic — value doesn't matter, just needs to be the
        // same everywhere.
        const AUDIT_LOCK_KEY: i64 = 0x_A441_1_06;

        let mut tx = self.pool.begin().await?;

        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(AUDIT_LOCK_KEY)
            .execute(&mut *tx)
            .await?;

        // Now safe: no other audit insert can read/write the
        // chain head until we commit.
        let prev_hash: Option<String> = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT prev_hash FROM audit_log ORDER BY timestamp DESC LIMIT 1",
        )
        .fetch_optional(&mut *tx)
        .await?
        .and_then(|r| r.0);

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
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        tracing::info!(
            operation = ?entry.operation,
            actor = %entry.actor,
            chain = ?entry.chain,
            "Audit log entry recorded"
        );
        Ok(())
    }

    /// Read the most recent freeze-transition from the audit log.
    ///
    /// Returns `true` when the latest `EmergencyFreeze` /
    /// `EmergencyUnfreeze` row is a freeze, `false` otherwise
    /// (including the "no row at all" case — fresh install).
    ///
    /// The withdrawal-pipeline's `FreezeFlag` is an in-memory
    /// `AtomicBool`; without this query a process restart silently
    /// clears a freeze set by an operator, which is exactly the
    /// scenario a freeze is meant to survive (suspected key
    /// compromise, active exploit, forensic hold). Startup reads
    /// this value and hydrates the flag; the freeze/unfreeze
    /// handlers persist the new state via `insert_audit_entry`
    /// BEFORE flipping the in-memory flag so a crash during the
    /// transition leaves the DB as the authoritative record.
    pub async fn latest_freeze_state(&self) -> anyhow::Result<bool> {
        // Serialized form matches how the handlers write the
        // operation (serde rename_all = snake_case on AuditOperation).
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT operation FROM audit_log
             WHERE operation IN ('\"emergency_freeze\"', '\"emergency_unfreeze\"')
             ORDER BY timestamp DESC LIMIT 1"
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(matches!(row, Some((ref op,)) if op == "\"emergency_freeze\""))
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
            // An unknown `operation` string used to silently
            // deserialize as `SigningRequest`. That misreports
            // the audit record to anyone reading the UI or the
            // recent-entries endpoint: a `WithdrawalOverride`
            // (or a newly-added operation from a future build)
            // would appear as a benign SigningRequest row. For
            // a tamper-evident log the FIRST job is to not lie
            // about what happened. Log loudly on an unknown
            // operation; keep the SigningRequest fallback only
            // so the API doesn't 500 on a schema-drift scenario
            // — an operator sees the WARN and investigates.
            let op: AuditOperation = match serde_json::from_str(&operation) {
                Ok(o) => o,
                Err(e) => {
                    tracing::warn!(
                        audit_id = %id,
                        raw_operation = %operation,
                        error = %e,
                        "audit_log: unknown operation string — falling back to \
                         SigningRequest for API shape; the row's real type is \
                         whatever the raw string says. Likely a newer build \
                         wrote an operation the current reader doesn't know, \
                         or DB tampering."
                    );
                    AuditOperation::SigningRequest
                }
            };
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
