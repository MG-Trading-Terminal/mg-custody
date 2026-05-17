//! Key management audit log.
//!
//! Records every security-sensitive operation (key derivation, address generation,
//! signing requests) with a tamper-evident hash chain.
//!
//! CRITICAL: Audit entries NEVER contain key material — only metadata
//! (operation type, user_id, chain, derivation_path, timestamps).

use chrono::{DateTime, Utc};
use custody_common::Chain;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::CustodyDb;

/// Type of audited operation.
///
/// Serializes / deserializes to a snake_case string. An unknown
/// string deserializes into `Unknown(raw)` — preserves the on-disk
/// value so a forensic reader sees what was actually written rather
/// than a benign-looking fallback. Newly-written rows never use
/// `Unknown` (only the constructors above produce known variants);
/// `Unknown` appears only when an older binary reads a row written
/// by a newer one (forward schema drift) or when DB content was
/// tampered with.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Catch-all for operation strings this build doesn't recognise.
    /// Carries the raw string so the UI / forensic tooling can
    /// render it verbatim instead of misreporting a known variant.
    Unknown(String),
}

impl AuditOperation {
    /// The wire name (snake_case) matching the on-disk JSON form.
    /// `Unknown(s)` returns the raw stored string.
    pub fn as_wire_str(&self) -> &str {
        match self {
            Self::AddressDerivation => "address_derivation",
            Self::SeedEncrypted => "seed_encrypted",
            Self::SeedDecrypted => "seed_decrypted",
            Self::SeedSplit => "seed_split",
            Self::SeedReconstructed => "seed_reconstructed",
            Self::SigningRequest => "signing_request",
            Self::SigningCompleted => "signing_completed",
            Self::SigningRejected => "signing_rejected",
            Self::EmergencyFreeze => "emergency_freeze",
            Self::EmergencyUnfreeze => "emergency_unfreeze",
            Self::WithdrawalOverride => "withdrawal_override",
            Self::PermissionDenied => "permission_denied",
            Self::ReadinessSilenceCreated => "readiness_silence_created",
            Self::ReadinessSilenceEnded => "readiness_silence_ended",
            Self::ReadinessChaosInjected => "readiness_chaos_injected",
            Self::ReadinessChaosCleared => "readiness_chaos_cleared",
            Self::Unknown(s) => s.as_str(),
        }
    }

    fn from_wire_str(s: &str) -> Self {
        match s {
            "address_derivation" => Self::AddressDerivation,
            "seed_encrypted" => Self::SeedEncrypted,
            "seed_decrypted" => Self::SeedDecrypted,
            "seed_split" => Self::SeedSplit,
            "seed_reconstructed" => Self::SeedReconstructed,
            "signing_request" => Self::SigningRequest,
            "signing_completed" => Self::SigningCompleted,
            "signing_rejected" => Self::SigningRejected,
            "emergency_freeze" => Self::EmergencyFreeze,
            "emergency_unfreeze" => Self::EmergencyUnfreeze,
            "withdrawal_override" => Self::WithdrawalOverride,
            "permission_denied" => Self::PermissionDenied,
            "readiness_silence_created" => Self::ReadinessSilenceCreated,
            "readiness_silence_ended" => Self::ReadinessSilenceEnded,
            "readiness_chaos_injected" => Self::ReadinessChaosInjected,
            "readiness_chaos_cleared" => Self::ReadinessChaosCleared,
            other => Self::Unknown(other.to_string()),
        }
    }
}

impl Serialize for AuditOperation {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(self.as_wire_str())
    }
}

impl<'de> Deserialize<'de> for AuditOperation {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = String::deserialize(de)?;
        Ok(Self::from_wire_str(&s))
    }
}

/// Outcome of `CustodyDb::verify_audit_chain`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AuditChainVerification {
    /// Every row's recomputed hash matched its stored hash.
    /// `rows_skipped_unknown` is how many rows the current binary
    /// can't verify because the operation variant is foreign
    /// (forward schema drift).
    Ok {
        rows_verified: u64,
        rows_skipped_unknown: u64,
    },
    /// First row whose recomputed hash diverges from the stored
    /// value. Subsequent rows are not checked — once the chain
    /// breaks, every later row's `prev_hash` no longer means
    /// "the row before me was unchanged" anyway.
    BrokenAt {
        row_id: Uuid,
        position: u64,
        stored: String,
        expected: String,
    },
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

/// Recompute the hash that links a row to its parent.
///
/// Shared between `insert_audit_entry` (writer) and
/// `verify_audit_chain` (forensic walker). Drift between these
/// two would silently cause the verifier to flag every row as
/// broken, so they share a single function. Format is
/// `parent_hash:id:Debug(operation):actor:rfc3339_timestamp`,
/// hashed with SHA-256.
fn compute_chain_hash(
    parent_hash: Option<&str>,
    id: Uuid,
    operation: &AuditOperation,
    actor: &str,
    timestamp: DateTime<Utc>,
) -> String {
    use sha2::Digest;
    let hash_input = format!(
        "{}:{}:{:?}:{}:{}",
        parent_hash.unwrap_or("genesis"),
        id,
        operation,
        actor,
        timestamp.to_rfc3339(),
    );
    hex::encode(sha2::Sha256::digest(hash_input.as_bytes()))
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

        let current_hash = compute_chain_hash(
            prev_hash.as_deref(),
            entry.id,
            &entry.operation,
            &entry.actor,
            entry.timestamp,
        );

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

    /// Walk the entire audit log in `timestamp ASC` order and
    /// recompute each row's hash from its predecessor + payload.
    ///
    /// Operators have no other programmatic way to detect a row
    /// that was modified or deleted between write time and now —
    /// the per-insert advisory lock keeps the chain linear, but
    /// nothing at read time proves the chain hasn't been edited
    /// out of band (DB user with UPDATE, replica that diverged,
    /// snapshot restore that lost a row).
    ///
    /// Returns `BrokenAt` on the first row whose stored hash
    /// disagrees with the recomputed value; success returns the
    /// verified row count and how many rows were skipped because
    /// their `operation` deserialized to `Unknown` (older binary
    /// reading newer rows — Debug-format would not match the
    /// stored hash, so we can't verify those without the original
    /// variant Debug string).
    pub async fn verify_audit_chain(&self) -> anyhow::Result<AuditChainVerification> {
        let rows: Vec<(Uuid, chrono::DateTime<Utc>, String, String, Option<String>)> =
            sqlx::query_as(
                "SELECT id, timestamp, operation, actor, prev_hash
                 FROM audit_log ORDER BY timestamp ASC",
            )
            .fetch_all(&self.pool)
            .await?;

        let mut parent_hash: Option<String> = None;
        let mut rows_verified: u64 = 0;
        let mut rows_skipped_unknown: u64 = 0;

        for (position, (id, timestamp, operation, actor, stored_hash)) in
            rows.into_iter().enumerate()
        {
            let stored_hash = stored_hash.unwrap_or_default();
            let op: AuditOperation = serde_json::from_str(&operation)
                .unwrap_or(AuditOperation::Unknown(operation.clone()));

            // Forward-drift rows: our build doesn't know the
            // original variant, so Debug-format would diverge from
            // what the original writer hashed. Propagate the
            // stored hash forward (so the next row can still be
            // checked against its parent) and count it as skipped.
            if matches!(op, AuditOperation::Unknown(_)) {
                rows_skipped_unknown += 1;
                parent_hash = Some(stored_hash);
                continue;
            }

            let expected = compute_chain_hash(
                parent_hash.as_deref(),
                id,
                &op,
                &actor,
                timestamp,
            );

            if expected != stored_hash {
                return Ok(AuditChainVerification::BrokenAt {
                    row_id: id,
                    position: position as u64,
                    stored: stored_hash,
                    expected,
                });
            }

            parent_hash = Some(stored_hash);
            rows_verified += 1;
        }

        Ok(AuditChainVerification::Ok {
            rows_verified,
            rows_skipped_unknown,
        })
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

        rows.into_iter().map(|(id, timestamp, operation, actor, chain, derivation_path, details, prev_hash)| {
            // `operation` is stored as a JSON-encoded string,
            // e.g. `"address_derivation"`. An unknown string
            // (forward schema drift or DB tampering) is now
            // surfaced as `Unknown(raw)` so the UI shows the
            // exact value rather than misreporting a benign
            // `SigningRequest` fallback. Failing the JSON parse
            // entirely (the column is corrupted, e.g. unbalanced
            // quotes) is still WARN-logged and falls back to
            // Unknown with the raw column value so the API
            // doesn't 500 on a single bad row.
            let op: AuditOperation = match serde_json::from_str(&operation) {
                Ok(o) => o,
                Err(e) => {
                    tracing::warn!(
                        audit_id = %id,
                        raw_operation = %operation,
                        error = %e,
                        "audit_log: operation column is not valid JSON — \
                         exposing as Unknown(raw) so an operator can see \
                         the actual stored value"
                    );
                    AuditOperation::Unknown(operation.clone())
                }
            };
            let chain = chain.map(|c| {
                custody_common::Chain::try_parse_str(&c)
                    .ok_or_else(|| anyhow::anyhow!("audit_log row {id}: unknown chain '{c}'"))
            }).transpose()?;
            Ok(AuditEntry { id, timestamp, operation: op, actor, chain, derivation_path, details, prev_hash })
        }).collect()
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
            AuditOperation::EmergencyFreeze,
            AuditOperation::EmergencyUnfreeze,
            AuditOperation::WithdrawalOverride,
            AuditOperation::PermissionDenied,
            AuditOperation::ReadinessSilenceCreated,
            AuditOperation::ReadinessSilenceEnded,
            AuditOperation::ReadinessChaosInjected,
            AuditOperation::ReadinessChaosCleared,
        ];

        for op in ops {
            let json = serde_json::to_string(&op).unwrap();
            let deserialized: AuditOperation = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, op);
        }
    }

    #[test]
    fn unknown_audit_operation_round_trips_raw_string() {
        // A row written by a future build with an operation this
        // build doesn't know about. Must surface verbatim, not be
        // silently misreported as SigningRequest (the previous
        // fallback) or any other known variant.
        let raw = r#""totally_new_op_v7""#;
        let deserialized: AuditOperation = serde_json::from_str(raw).unwrap();
        assert_eq!(deserialized, AuditOperation::Unknown("totally_new_op_v7".into()));
        // Re-serialize must reproduce the original string exactly.
        assert_eq!(serde_json::to_string(&deserialized).unwrap(), raw);
    }

    #[test]
    fn compute_chain_hash_is_deterministic_and_parent_sensitive() {
        let id = Uuid::nil();
        let ts = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let h1 = compute_chain_hash(
            None, id, &AuditOperation::SigningRequest, "system", ts,
        );
        let h2 = compute_chain_hash(
            None, id, &AuditOperation::SigningRequest, "system", ts,
        );
        assert_eq!(h1, h2, "hash must be deterministic");

        // Changing the parent → different hash.
        let h3 = compute_chain_hash(
            Some("parent"), id, &AuditOperation::SigningRequest, "system", ts,
        );
        assert_ne!(h1, h3);

        // Changing the operation → different hash.
        let h4 = compute_chain_hash(
            None, id, &AuditOperation::EmergencyFreeze, "system", ts,
        );
        assert_ne!(h1, h4);
    }

    #[test]
    fn audit_chain_verification_serializes_with_status_tag() {
        let ok = AuditChainVerification::Ok {
            rows_verified: 42,
            rows_skipped_unknown: 1,
        };
        let json = serde_json::to_string(&ok).unwrap();
        assert!(json.contains(r#""status":"ok""#));
        assert!(json.contains(r#""rows_verified":42"#));

        let broken = AuditChainVerification::BrokenAt {
            row_id: Uuid::nil(),
            position: 7,
            stored: "aa".into(),
            expected: "bb".into(),
        };
        let json = serde_json::to_string(&broken).unwrap();
        assert!(json.contains(r#""status":"broken_at""#));
        assert!(json.contains(r#""position":7"#));
    }

    #[test]
    fn known_variant_wire_strings_match_snake_case() {
        // Sanity: every known variant's `as_wire_str` matches the
        // snake_case form our writers emit. Drift here would
        // silently corrupt the audit chain on a roundtrip.
        assert_eq!(
            AuditOperation::AddressDerivation.as_wire_str(),
            "address_derivation"
        );
        assert_eq!(
            AuditOperation::EmergencyFreeze.as_wire_str(),
            "emergency_freeze"
        );
        assert_eq!(
            AuditOperation::ReadinessChaosCleared.as_wire_str(),
            "readiness_chaos_cleared"
        );
    }
}
