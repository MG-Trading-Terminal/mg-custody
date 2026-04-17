//! Swap execution persistence.
//!
//! Backs `POST /v1/swaps` + `GET /v1/swaps/{id}`. Writes on execute,
//! reads on status. Ownership-scoped reads are the non-negotiable bit:
//! one user must not be able to read another's swap records by id.

use chrono::{DateTime, Utc};
use custody_common::swap_executor::{SwapExecution, SwapExecutionStatus, SwapProtocol};
use custody_common::Chain;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    /// Insert a fresh swap execution. Idempotent on `id` — if the same
    /// swap id shows up again (retries after a partial failure), the
    /// status/tx_hash/error fields are refreshed without creating a
    /// duplicate row. Called by the gateway immediately after
    /// building the unsigned tx.
    pub async fn upsert_swap_execution(
        &self,
        user_id: Uuid,
        swap: &SwapExecution,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO swap_executions
                 (id, quote_id, user_id, chain, protocol,
                  from_token, to_token, amount_in, expected_out, minimum_out,
                  slippage_bps, status, tx_hash, calldata, encoded_tx, error_message,
                  created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)
             ON CONFLICT (id) DO UPDATE
             SET status        = EXCLUDED.status,
                 tx_hash       = EXCLUDED.tx_hash,
                 calldata      = EXCLUDED.calldata,
                 encoded_tx    = EXCLUDED.encoded_tx,
                 error_message = EXCLUDED.error_message,
                 updated_at    = NOW()",
        )
        .bind(swap.id)
        .bind(swap.quote_id)
        .bind(user_id)
        .bind(chain_to_str(swap.chain))
        .bind(protocol_to_str(swap.protocol))
        .bind(&swap.from_token)
        .bind(&swap.to_token)
        .bind(swap.amount_in)
        .bind(swap.expected_out)
        .bind(swap.minimum_out)
        .bind(swap.slippage_bps as i32)
        .bind(status_to_str(swap.status))
        .bind(&swap.tx_hash)
        .bind(&swap.calldata)
        .bind(&swap.encoded_tx)
        .bind(&swap.error_message)
        .bind(swap.created_at)
        .bind(swap.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Fetch a swap owned by `user_id`. Returns `None` for both
    /// "row does not exist" and "row exists but belongs to someone
    /// else" — the handler maps either to a generic 404 so attackers
    /// can't tell which swap IDs exist.
    pub async fn find_swap_for_user(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> anyhow::Result<Option<SwapExecution>> {
        let row: Option<SwapRow> = sqlx::query_as(
            "SELECT id, quote_id, chain, protocol,
                    from_token, to_token, amount_in, expected_out, minimum_out,
                    slippage_bps, status, tx_hash, calldata, encoded_tx, error_message,
                    created_at, updated_at
             FROM swap_executions
             WHERE id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_swap).transpose()
    }

    /// Fetch a swap by id without owner-scope. Admin-only use at the
    /// handler layer — never expose this to Viewer/Editor/Signer.
    pub async fn find_swap_any(&self, id: Uuid) -> anyhow::Result<Option<SwapExecution>> {
        let row: Option<SwapRow> = sqlx::query_as(
            "SELECT id, quote_id, chain, protocol,
                    from_token, to_token, amount_in, expected_out, minimum_out,
                    slippage_bps, status, tx_hash, calldata, encoded_tx, error_message,
                    created_at, updated_at
             FROM swap_executions
             WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_swap).transpose()
    }
}

// ---------------------------------------------------------------------------
// Row mapping
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct SwapRow {
    id: Uuid,
    quote_id: Uuid,
    chain: String,
    protocol: String,
    from_token: String,
    to_token: String,
    amount_in: Decimal,
    expected_out: Decimal,
    minimum_out: Decimal,
    slippage_bps: i32,
    status: String,
    tx_hash: Option<String>,
    calldata: Option<String>,
    encoded_tx: Option<String>,
    error_message: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

fn row_to_swap(r: SwapRow) -> anyhow::Result<SwapExecution> {
    Ok(SwapExecution {
        id: r.id,
        quote_id: r.quote_id,
        chain: str_to_chain(&r.chain)?,
        protocol: str_to_protocol(&r.protocol)?,
        from_token: r.from_token,
        to_token: r.to_token,
        amount_in: r.amount_in,
        expected_out: r.expected_out,
        minimum_out: r.minimum_out,
        slippage_bps: r.slippage_bps.max(0) as u32,
        status: str_to_status(&r.status)?,
        tx_hash: r.tx_hash,
        calldata: r.calldata,
        encoded_tx: r.encoded_tx,
        error_message: r.error_message,
        created_at: r.created_at,
        updated_at: r.updated_at,
    })
}

fn chain_to_str(c: Chain) -> &'static str {
    match c {
        Chain::Bitcoin => "bitcoin",
        Chain::Ethereum => "ethereum",
        Chain::Tron => "tron",
        Chain::Solana => "solana",
    }
}

fn str_to_chain(s: &str) -> anyhow::Result<Chain> {
    match s {
        "bitcoin" => Ok(Chain::Bitcoin),
        "ethereum" => Ok(Chain::Ethereum),
        "tron" => Ok(Chain::Tron),
        "solana" => Ok(Chain::Solana),
        other => Err(anyhow::anyhow!("unknown chain in swap_executions: {other}")),
    }
}

fn protocol_to_str(p: SwapProtocol) -> &'static str {
    match p {
        SwapProtocol::UniswapV3 => "uniswap_v3",
        SwapProtocol::Jupiter => "jupiter",
    }
}

fn str_to_protocol(s: &str) -> anyhow::Result<SwapProtocol> {
    match s {
        "uniswap_v3" => Ok(SwapProtocol::UniswapV3),
        "jupiter" => Ok(SwapProtocol::Jupiter),
        other => Err(anyhow::anyhow!("unknown swap protocol: {other}")),
    }
}

fn status_to_str(s: SwapExecutionStatus) -> &'static str {
    match s {
        SwapExecutionStatus::Pending => "pending",
        SwapExecutionStatus::ReadyToSign => "ready_to_sign",
        SwapExecutionStatus::Broadcast => "broadcast",
        SwapExecutionStatus::Confirmed => "confirmed",
        SwapExecutionStatus::Failed => "failed",
    }
}

fn str_to_status(s: &str) -> anyhow::Result<SwapExecutionStatus> {
    match s {
        "pending" => Ok(SwapExecutionStatus::Pending),
        "ready_to_sign" => Ok(SwapExecutionStatus::ReadyToSign),
        "broadcast" => Ok(SwapExecutionStatus::Broadcast),
        "confirmed" => Ok(SwapExecutionStatus::Confirmed),
        "failed" => Ok(SwapExecutionStatus::Failed),
        other => Err(anyhow::anyhow!("unknown swap status: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_roundtrip() {
        for c in [Chain::Bitcoin, Chain::Ethereum, Chain::Tron, Chain::Solana] {
            assert_eq!(str_to_chain(chain_to_str(c)).unwrap(), c);
        }
    }

    #[test]
    fn protocol_roundtrip() {
        for p in [SwapProtocol::UniswapV3, SwapProtocol::Jupiter] {
            assert_eq!(str_to_protocol(protocol_to_str(p)).unwrap(), p);
        }
    }

    #[test]
    fn status_roundtrip() {
        for s in [
            SwapExecutionStatus::Pending,
            SwapExecutionStatus::ReadyToSign,
            SwapExecutionStatus::Broadcast,
            SwapExecutionStatus::Confirmed,
            SwapExecutionStatus::Failed,
        ] {
            assert_eq!(str_to_status(status_to_str(s)).unwrap(), s);
        }
    }

    #[test]
    fn unknown_string_rejected() {
        assert!(str_to_protocol("1inch").is_err());
        assert!(str_to_status("pending_broadcast").is_err());
    }
}
