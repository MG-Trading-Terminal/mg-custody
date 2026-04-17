//! Bitcoin UTXO consolidation job persistence.
//!
//! Schema: see migration 033. Status transitions mirror the user-
//! withdrawal state machine (Received → Signed → Broadcast →
//! Confirmed | Failed) so the scheduler and worker can use the same
//! mental model as the rest of the service.
//!
//! `inputs` is stored as JSONB so the worker can replay the exact
//! UTXO set after a restart instead of re-selecting and potentially
//! racing with the scanner.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::CustodyDb;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsolidationStatus {
    Received,
    Signed,
    Broadcast,
    Confirmed,
    Failed,
}

impl ConsolidationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Signed => "signed",
            Self::Broadcast => "broadcast",
            Self::Confirmed => "confirmed",
            Self::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "received" => Some(Self::Received),
            "signed" => Some(Self::Signed),
            "broadcast" => Some(Self::Broadcast),
            "confirmed" => Some(Self::Confirmed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// Serialisable UTXO input for a consolidation job. Matches the
/// JSONB shape stored in `consolidation_jobs.inputs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationInput {
    pub txid: String,
    pub vout: u32,
    pub amount_sats: u64,
    pub address: String,
}

#[derive(Debug, Clone)]
pub struct ConsolidationJob {
    pub id: Uuid,
    pub status: ConsolidationStatus,
    pub destination_address: String,
    pub inputs: Vec<ConsolidationInput>,
    pub total_input_sats: u64,
    pub output_sats: u64,
    pub fee_sats: u64,
    pub fee_rate_sat_per_vb: u64,
    pub signed_tx_bytes: Option<Vec<u8>>,
    pub tx_hash: Option<String>,
    pub block_height: Option<u64>,
    pub confirmations: u64,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub signed_at: Option<DateTime<Utc>>,
    pub broadcast_at: Option<DateTime<Utc>>,
    pub confirmed_at: Option<DateTime<Utc>>,
}

impl CustodyDb {
    /// Persist a freshly-planned job. Worker will later pick it up.
    pub async fn insert_consolidation_job(&self, job: &ConsolidationJob) -> anyhow::Result<()> {
        let inputs_json = serde_json::to_value(&job.inputs)?;
        sqlx::query(
            "INSERT INTO consolidation_jobs
                 (id, status, destination_address, inputs,
                  total_input_sats, output_sats, fee_sats, fee_rate_sat_per_vb,
                  signed_tx_bytes, tx_hash, block_height, confirmations,
                  error_message, created_at, signed_at, broadcast_at, confirmed_at)
             VALUES ($1, $2, $3, $4,
                     $5, $6, $7, $8,
                     $9, $10, $11, $12,
                     $13, $14, $15, $16, $17)",
        )
        .bind(job.id)
        .bind(job.status.as_str())
        .bind(&job.destination_address)
        .bind(&inputs_json)
        .bind(job.total_input_sats as i64)
        .bind(job.output_sats as i64)
        .bind(job.fee_sats as i64)
        .bind(job.fee_rate_sat_per_vb as i64)
        .bind(job.signed_tx_bytes.as_deref())
        .bind(&job.tx_hash)
        .bind(job.block_height.map(|h| h as i64))
        .bind(job.confirmations as i64)
        .bind(&job.error_message)
        .bind(job.created_at)
        .bind(job.signed_at)
        .bind(job.broadcast_at)
        .bind(job.confirmed_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Fetch jobs in a specific status, oldest first. `limit` caps
    /// the worker batch size so one burst of small UTXOs doesn't
    /// monopolise a single tick.
    pub async fn list_consolidation_jobs_by_status(
        &self,
        status: ConsolidationStatus,
        limit: i64,
    ) -> anyhow::Result<Vec<ConsolidationJob>> {
        let rows: Vec<ConsolidationJobRow> = sqlx::query_as(
            "SELECT id, status, destination_address, inputs,
                    total_input_sats, output_sats, fee_sats, fee_rate_sat_per_vb,
                    signed_tx_bytes, tx_hash, block_height, confirmations,
                    error_message, created_at, signed_at, broadcast_at, confirmed_at
             FROM consolidation_jobs
             WHERE status = $1
             ORDER BY created_at ASC
             LIMIT $2",
        )
        .bind(status.as_str())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_job).collect()
    }

    /// Record the signed-tx bytes + advance to Signed.
    pub async fn mark_consolidation_signed(
        &self,
        id: Uuid,
        signed_tx: &[u8],
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE consolidation_jobs
             SET status = 'signed',
                 signed_tx_bytes = $2,
                 signed_at = NOW()
             WHERE id = $1",
        )
        .bind(id)
        .bind(signed_tx)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Record the broadcast tx_hash + advance to Broadcast.
    pub async fn mark_consolidation_broadcast(
        &self,
        id: Uuid,
        tx_hash: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE consolidation_jobs
             SET status = 'broadcast',
                 tx_hash = $2,
                 broadcast_at = NOW()
             WHERE id = $1",
        )
        .bind(id)
        .bind(tx_hash)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Record confirmation tally + advance to Confirmed when enough.
    pub async fn update_consolidation_confirmations(
        &self,
        id: Uuid,
        block_height: u64,
        confirmations: u64,
        threshold: u64,
    ) -> anyhow::Result<()> {
        let status = if confirmations >= threshold {
            "confirmed"
        } else {
            "broadcast"
        };
        sqlx::query(
            "UPDATE consolidation_jobs
             SET status = $2,
                 block_height = $3,
                 confirmations = $4,
                 confirmed_at = CASE WHEN $2 = 'confirmed' THEN NOW() ELSE confirmed_at END
             WHERE id = $1",
        )
        .bind(id)
        .bind(status)
        .bind(block_height as i64)
        .bind(confirmations as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark the job failed with a diagnostic message. Called when
    /// signing or broadcast hits an unrecoverable error so a human
    /// operator can investigate without the worker retrying
    /// indefinitely on a malformed input set.
    pub async fn mark_consolidation_failed(
        &self,
        id: Uuid,
        error_message: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE consolidation_jobs
             SET status = 'failed',
                 error_message = $2
             WHERE id = $1",
        )
        .bind(id)
        .bind(error_message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct ConsolidationJobRow {
    id: Uuid,
    status: String,
    destination_address: String,
    inputs: serde_json::Value,
    total_input_sats: i64,
    output_sats: i64,
    fee_sats: i64,
    fee_rate_sat_per_vb: i64,
    signed_tx_bytes: Option<Vec<u8>>,
    tx_hash: Option<String>,
    block_height: Option<i64>,
    confirmations: i64,
    error_message: Option<String>,
    created_at: DateTime<Utc>,
    signed_at: Option<DateTime<Utc>>,
    broadcast_at: Option<DateTime<Utc>>,
    confirmed_at: Option<DateTime<Utc>>,
}

fn row_to_job(r: ConsolidationJobRow) -> anyhow::Result<ConsolidationJob> {
    let status = ConsolidationStatus::parse(&r.status)
        .ok_or_else(|| anyhow::anyhow!("unknown consolidation_jobs.status: {}", r.status))?;
    let inputs: Vec<ConsolidationInput> = serde_json::from_value(r.inputs)
        .map_err(|e| anyhow::anyhow!("consolidation_jobs.inputs JSON: {e}"))?;
    Ok(ConsolidationJob {
        id: r.id,
        status,
        destination_address: r.destination_address,
        inputs,
        total_input_sats: r.total_input_sats.max(0) as u64,
        output_sats: r.output_sats.max(0) as u64,
        fee_sats: r.fee_sats.max(0) as u64,
        fee_rate_sat_per_vb: r.fee_rate_sat_per_vb.max(0) as u64,
        signed_tx_bytes: r.signed_tx_bytes,
        tx_hash: r.tx_hash,
        block_height: r.block_height.map(|h| h.max(0) as u64),
        confirmations: r.confirmations.max(0) as u64,
        error_message: r.error_message,
        created_at: r.created_at,
        signed_at: r.signed_at,
        broadcast_at: r.broadcast_at,
        confirmed_at: r.confirmed_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrip() {
        for s in [
            ConsolidationStatus::Received,
            ConsolidationStatus::Signed,
            ConsolidationStatus::Broadcast,
            ConsolidationStatus::Confirmed,
            ConsolidationStatus::Failed,
        ] {
            assert_eq!(ConsolidationStatus::parse(s.as_str()), Some(s));
        }
    }

    #[test]
    fn unknown_status_rejected() {
        assert_eq!(ConsolidationStatus::parse("queued"), None);
    }

    #[test]
    fn consolidation_input_roundtrip() {
        let inp = ConsolidationInput {
            txid: "abc".into(),
            vout: 1,
            amount_sats: 12345,
            address: "bc1qx".into(),
        };
        let json = serde_json::to_value(&inp).unwrap();
        let back: ConsolidationInput = serde_json::from_value(json).unwrap();
        assert_eq!(back.txid, "abc");
        assert_eq!(back.vout, 1);
        assert_eq!(back.amount_sats, 12345);
    }
}
