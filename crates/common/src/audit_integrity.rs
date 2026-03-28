//! Audit log tamper-evident hash chain verification.
//!
//! The audit log records a `prev_hash` on every entry: `SHA-256` of the concatenation of
//! the previous entry's stored hash, the current entry's UUID, serialised operation, actor,
//! and RFC-3339 timestamp.  This forms a hash chain.  If any entry is altered, deleted, or
//! inserted, the chain breaks at that point.
//!
//! This module re-derives each expected hash from the entry data and compares it to the
//! stored `prev_hash` of the _next_ entry, detecting any tampering.
//!
//! # Note on the first entry
//! The genesis entry has `prev_hash` set to `SHA-256("genesis:...")`.  The verifier
//! receives the full ordered slice and can start from the first known-good entry.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A minimal view of an audit log entry needed for integrity checking.
///
/// This mirrors the fields used in [`custody_db::audit::CustodyDb::insert_audit_entry`]
/// to compute the hash chain.
#[derive(Debug, Clone)]
pub struct AuditEntryForVerification {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub operation_json: String,
    pub actor: String,
    /// Hash stored in the database for this entry (the running chain hash).
    pub stored_hash: Option<String>,
}

/// Result of a hash-chain integrity verification run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IntegrityResult {
    /// Whether all checked entries form a valid unbroken chain.
    pub valid: bool,
    /// Number of entries that were examined.
    pub entries_checked: usize,
    /// ID of the first entry where the chain breaks, if any.
    pub first_broken_at: Option<Uuid>,
    /// Stored hash of the last successfully verified entry.
    pub last_valid_hash: String,
    /// When this check was performed.
    pub checked_at: DateTime<Utc>,
}

/// Verifier for the tamper-evident audit log hash chain.
pub struct AuditIntegrityChecker;

impl AuditIntegrityChecker {
    /// Verify an ordered (oldest-first) slice of audit entries.
    ///
    /// The algorithm re-derives each entry's expected hash and checks that the
    /// _next_ entry's stored hash matches what was computed.
    ///
    /// Returns an [`IntegrityResult`] indicating whether the chain is intact and,
    /// if not, the UUID of the first broken link.
    pub fn verify_chain(entries: &[AuditEntryForVerification]) -> IntegrityResult {
        Self::verify_range(entries, None)
    }

    /// Verify a subset of entries, optionally providing the hash that the first
    /// entry in the slice should chain from (for incremental verification).
    ///
    /// Pass `expected_prev_hash = None` when starting from the genesis entry.
    pub fn verify_range(
        entries: &[AuditEntryForVerification],
        expected_prev_hash: Option<&str>,
    ) -> IntegrityResult {
        if entries.is_empty() {
            return IntegrityResult {
                valid: true,
                entries_checked: 0,
                first_broken_at: None,
                last_valid_hash: expected_prev_hash.unwrap_or("genesis").to_string(),
                checked_at: Utc::now(),
            };
        }

        // The prev_hash context fed into the first entry.
        let mut running_prev = expected_prev_hash.unwrap_or("genesis").to_string();

        let mut last_valid_hash = running_prev.clone();
        let mut entries_checked = 0;

        for entry in entries {
            // Re-derive what the stored hash for this entry should be.
            let expected_hash = derive_entry_hash(&running_prev, entry);

            // The stored hash on the entry IS the derived hash for that entry.
            // The next entry would chain off this hash.
            match &entry.stored_hash {
                Some(stored) if stored == &expected_hash => {
                    // Hash matches — chain intact so far.
                    last_valid_hash = stored.clone();
                    running_prev = stored.clone();
                    entries_checked += 1;
                }
                _ => {
                    // Chain broken at this entry.
                    return IntegrityResult {
                        valid: false,
                        entries_checked,
                        first_broken_at: Some(entry.id),
                        last_valid_hash,
                        checked_at: Utc::now(),
                    };
                }
            }
        }

        IntegrityResult {
            valid: true,
            entries_checked,
            first_broken_at: None,
            last_valid_hash,
            checked_at: Utc::now(),
        }
    }
}

/// Re-derive the stored hash for a single audit entry.
///
/// Must match the formula used in `CustodyDb::insert_audit_entry`:
/// ```text
/// hash_input = "{prev_hash}:{id}:{operation}:{actor}:{timestamp_rfc3339}"
/// stored_hash = hex(SHA-256(hash_input))
/// ```
fn derive_entry_hash(prev_hash: &str, entry: &AuditEntryForVerification) -> String {
    use sha2::Digest;
    let hash_input = format!(
        "{}:{}:{}:{}:{}",
        prev_hash,
        entry.id,
        entry.operation_json,
        entry.actor,
        entry.timestamp.to_rfc3339(),
    );
    hex::encode(sha2::Sha256::digest(hash_input.as_bytes()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_entry(id: Uuid, ts: DateTime<Utc>, op: &str, actor: &str, prev: &str) -> AuditEntryForVerification {
        let stored = derive_entry_hash(prev, &AuditEntryForVerification {
            id,
            timestamp: ts,
            operation_json: op.to_string(),
            actor: actor.to_string(),
            stored_hash: None,
        });
        AuditEntryForVerification {
            id,
            timestamp: ts,
            operation_json: op.to_string(),
            actor: actor.to_string(),
            stored_hash: Some(stored),
        }
    }

    #[test]
    fn empty_log_is_valid() {
        let result = AuditIntegrityChecker::verify_chain(&[]);
        assert!(result.valid);
        assert_eq!(result.entries_checked, 0);
        assert!(result.first_broken_at.is_none());
    }

    #[test]
    fn single_entry_valid_chain() {
        let ts = Utc::now();
        let id = Uuid::new_v4();
        let entry = make_entry(id, ts, "\"address_derivation\"", "system", "genesis");

        let result = AuditIntegrityChecker::verify_chain(&[entry]);
        assert!(result.valid);
        assert_eq!(result.entries_checked, 1);
    }

    #[test]
    fn multi_entry_valid_chain() {
        let ts1 = Utc::now();
        let ts2 = Utc::now();
        let ts3 = Utc::now();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        let e1 = make_entry(id1, ts1, "\"address_derivation\"", "user1", "genesis");
        let hash1 = e1.stored_hash.clone().unwrap();
        let e2 = make_entry(id2, ts2, "\"signing_request\"", "system", &hash1);
        let hash2 = e2.stored_hash.clone().unwrap();
        let e3 = make_entry(id3, ts3, "\"signing_completed\"", "system", &hash2);

        let result = AuditIntegrityChecker::verify_chain(&[e1, e2, e3]);
        assert!(result.valid);
        assert_eq!(result.entries_checked, 3);
    }

    #[test]
    fn tampered_entry_detected() {
        let ts1 = Utc::now();
        let ts2 = Utc::now();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let e1 = make_entry(id1, ts1, "\"address_derivation\"", "user1", "genesis");
        let hash1 = e1.stored_hash.clone().unwrap();
        let mut e2 = make_entry(id2, ts2, "\"signing_request\"", "system", &hash1);

        // Tamper: change the actor after computing the hash.
        e2.actor = "attacker".to_string();

        let result = AuditIntegrityChecker::verify_chain(&[e1, e2]);
        assert!(!result.valid);
        assert_eq!(result.entries_checked, 1); // first entry OK, second breaks
        assert_eq!(result.first_broken_at, Some(id2));
    }

    #[test]
    fn wrong_prev_hash_breaks_chain() {
        let ts = Utc::now();
        let id = Uuid::new_v4();
        let entry = make_entry(id, ts, "\"signing_request\"", "system", "genesis");

        // Verify against a different expected_prev_hash.
        let result = AuditIntegrityChecker::verify_range(&[entry], Some("wrong_prev_hash"));
        assert!(!result.valid);
        assert_eq!(result.first_broken_at, Some(id));
    }

    #[test]
    fn incremental_verification() {
        let ts1 = Utc::now();
        let ts2 = Utc::now();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let e1 = make_entry(id1, ts1, "\"address_derivation\"", "user1", "genesis");
        let hash1 = e1.stored_hash.clone().unwrap();
        let e2 = make_entry(id2, ts2, "\"signing_request\"", "system", &hash1);

        // First batch: verify e1 alone.
        let r1 = AuditIntegrityChecker::verify_chain(&[e1]);
        assert!(r1.valid);

        // Second batch: verify e2 starting from where e1 left off.
        let r2 = AuditIntegrityChecker::verify_range(&[e2], Some(&r1.last_valid_hash));
        assert!(r2.valid);
        assert_eq!(r2.entries_checked, 1);
    }
}
