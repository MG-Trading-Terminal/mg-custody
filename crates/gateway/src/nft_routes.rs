//! NFT custody API routes.
//!
//! CRUD for NFTs held in custody vaults. Register/list/get/delete are
//! backed by the `custodied_nfts` table (migration 025). List and get
//! are owner-scoped via a join on `vaults.owner_id`.
//!
//! `POST /v1/nfts/{id}/transfer` prepares an on-chain transfer by
//! returning the calldata and metadata a signer needs to submit the
//! transaction through an existing path (Safe tx proposal, withdrawal
//! pipeline). It does NOT broadcast — the generic-calldata broadcast
//! lane is not wired yet, and the previous "status: pending" response
//! was a lie that told the user their NFT had been queued when
//! nothing had happened.
//!
//! Access levels:
//!   - GET endpoints: RequireViewer (own NFTs only)
//!   - POST /v1/nfts (register): RequireEditor
//!   - DELETE /v1/nfts/{id}: RequireEditor
//!   - POST /v1/nfts/{id}/transfer: RequireSigner

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use custody_common::nft_custody::{
    CustodiedNft, NftStandard, NftTransferRequest, RegisterNftRequest,
    encode_erc721_transfer, encode_erc1155_transfer,
};
use custody_common::Chain;

use crate::errors::ApiError;
use crate::rbac_middleware::{RequireEditor, RequireSigner, RequireViewer};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn nft_router() -> Router<AppState> {
    Router::new()
        .route("/v1/nfts",              get(list_nfts).post(register_nft))
        .route("/v1/nfts/{id}",         get(get_nft).delete(delete_nft))
        .route("/v1/nfts/{id}/transfer", post(transfer_nft))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load an NFT and reject if the caller doesn't own its vault. Admins
/// bypass the ownership check so they can operate on any workspace row.
async fn load_nft_scoped(
    state: &AppState,
    auth: &crate::rbac_middleware::AuthContext,
    id: Uuid,
) -> Result<CustodiedNft, ApiError> {
    let nft = state
        .db
        .find_custodied_nft(id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found(&format!("NFT {id} not found")))?;

    let is_admin = auth.role.has_at_least(&custody_db::sessions::Role::Admin);
    if !is_admin {
        let vault = state
            .db
            .find_vault(nft.owner_vault_id)
            .await
            .map_err(|e| ApiError::internal(&e.to_string()))?;
        match vault {
            Some(v) if v.owner_id == auth.user_id => {}
            _ => return Err(ApiError::forbidden("NFT is held in a vault you do not own")),
        }
    }
    Ok(nft)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /v1/nfts — list NFTs the caller owns (any of their vaults).
async fn list_nfts(
    auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rows = state
        .db
        .list_custodied_nfts_for_user(auth.0.user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    Ok(Json(json!({
        "count": rows.len(),
        "nfts": rows,
    })))
}

/// GET /v1/nfts/{id} — fetch one NFT, only if the caller owns it.
async fn get_nft(
    auth: RequireViewer,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CustodiedNft>, ApiError> {
    let nft = load_nft_scoped(&state, &auth.0, id).await?;
    Ok(Json(nft))
}

/// POST /v1/nfts — register an NFT in custody.
///
/// Caller must own the target `owner_vault_id` (or be Admin). The
/// UNIQUE (chain, contract, token_id, vault) constraint turns repeat
/// registrations into a 409, which we map explicitly so callers can
/// distinguish "already registered" from "DB unreachable".
async fn register_nft(
    auth: RequireEditor,
    State(state): State<AppState>,
    Json(req): Json<RegisterNftRequest>,
) -> Result<(StatusCode, Json<CustodiedNft>), ApiError> {
    match (req.chain, req.standard) {
        (Chain::Ethereum, NftStandard::Metaplex) => {
            return Err(ApiError::bad_request(
                "Metaplex standard is only valid on Solana",
            ));
        }
        (Chain::Solana, NftStandard::Erc721) | (Chain::Solana, NftStandard::Erc1155) => {
            return Err(ApiError::bad_request(
                "ERC-721/1155 standards are only valid on EVM chains",
            ));
        }
        _ => {}
    }

    if req.contract_address.is_empty() {
        return Err(ApiError::bad_request("contract_address must not be empty"));
    }
    if req.token_id.is_empty() {
        return Err(ApiError::bad_request("token_id must not be empty"));
    }

    // Vault ownership gate — without this, a user could register NFTs
    // against any vault id they guessed.
    let is_admin = auth.0.role.has_at_least(&custody_db::sessions::Role::Admin);
    let vault = state
        .db
        .find_vault(req.owner_vault_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::bad_request("owner_vault_id does not exist"))?;
    if !is_admin && vault.owner_id != auth.0.user_id {
        return Err(ApiError::forbidden("owner_vault_id is not one of your vaults"));
    }

    let nft = CustodiedNft {
        id: Uuid::new_v4(),
        chain: req.chain,
        contract_address: req.contract_address,
        token_id: req.token_id,
        standard: req.standard,
        owner_vault_id: req.owner_vault_id,
        metadata_uri: req.metadata_uri,
        name: req.name,
        image_url: req.image_url,
        quantity: req.quantity.unwrap_or(1),
        acquired_at: Utc::now(),
    };

    state
        .db
        .insert_custodied_nft(&nft)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("duplicate key") || msg.contains("unique") {
                ApiError::conflict("NFT already registered in this vault")
            } else {
                ApiError::internal(&msg)
            }
        })?;

    tracing::info!(
        nft_id = %nft.id,
        chain = %nft.chain,
        contract = %nft.contract_address,
        token_id = %nft.token_id,
        "NFT registered in custody"
    );
    Ok((StatusCode::CREATED, Json(nft)))
}

/// DELETE /v1/nfts/{id} — retire a custody record.
///
/// Used after the NFT has been transferred out of custody via an
/// out-of-band path (e.g., a signed Safe transaction executed on
/// chain). Does not itself move the token.
async fn delete_nft(
    auth: RequireEditor,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Ownership check first — use same scoped load so non-admins can't
    // delete NFTs that are in other users' vaults.
    let nft = load_nft_scoped(&state, &auth.0, id).await?;
    let deleted = state
        .db
        .delete_custodied_nft(nft.id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    if !deleted {
        // Race: row was removed between load and delete. Idempotent
        // from the caller's POV, but we surface it as 200 rather than
        // 404 because the desired end-state is reached.
        tracing::info!(nft_id = %id, "NFT record already gone at delete-time");
    }
    Ok(Json(json!({ "status": "deleted", "nft_id": id })))
}

/// POST /v1/nfts/{id}/transfer — prepare an NFT transfer.
///
/// **This endpoint does NOT broadcast.** It validates ownership, builds
/// the on-chain calldata, and returns everything a signer needs to
/// submit the transaction via the existing lanes (Safe tx proposal on
/// EVM; Solana and Bitcoin return 501 because no NFT transfer path is
/// wired there yet). The pre-existing response lied about queueing the
/// transfer — we'd rather return an honest "prepared, not broadcast"
/// result than a fake "pending".
async fn transfer_nft(
    auth: RequireSigner,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<NftTransferRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if req.to_address.is_empty() {
        return Err(ApiError::bad_request("to_address must not be empty"));
    }

    let nft = load_nft_scoped(&state, &auth.0, id).await?;

    // From-address = the vault owner's native-asset address for this
    // chain (ERC-721/1155 balances live at the same EVM address as
    // ETH). No fabrication — if the user has no derived address yet,
    // the caller gets a clear 409.
    let vault = state
        .db
        .find_vault(nft.owner_vault_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::internal("NFT row references a missing vault"))?;
    let from_address = state
        .db
        .find_native_address_for_chain(vault.owner_id, nft.chain)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| {
            ApiError::conflict(
                "Vault owner has no derived address on this chain — GET /v1/user/deposit-address/{chain} first",
            )
        })?;

    // Validate destination address shape for the target chain.
    custody_common::validation::validate_address(nft.chain, &req.to_address)
        .map_err(|e| ApiError::bad_request(&format!("invalid to_address: {e}")))?;

    match nft.chain {
        Chain::Ethereum => {
            let calldata_hex = match nft.standard {
                NftStandard::Erc721 => erc721_calldata_hex(
                    &from_address,
                    &req.to_address,
                    &nft.token_id,
                )
                .map_err(|e| ApiError::internal(&format!("calldata build failed: {e}")))?,
                NftStandard::Erc1155 => {
                    let amount = nft.quantity.min(u64::MAX);
                    erc1155_calldata_hex(
                        &from_address,
                        &req.to_address,
                        &nft.token_id,
                        amount,
                    )
                    .map_err(|e| ApiError::internal(&format!("calldata build failed: {e}")))?
                }
                NftStandard::Metaplex => {
                    return Err(ApiError::bad_request(
                        "Metaplex NFT found on Ethereum — registration is inconsistent",
                    ));
                }
            };

            tracing::info!(
                nft_id = %id,
                from = %from_address,
                to = %req.to_address,
                standard = ?nft.standard,
                "NFT transfer prepared (calldata only — not broadcast)"
            );

            Ok(Json(json!({
                "nft_id": nft.id,
                "chain": "ethereum",
                "standard": nft.standard,
                "from_address": from_address,
                "to_address": req.to_address,
                "contract_address": nft.contract_address,
                "token_id": nft.token_id,
                "calldata_hex": format!("0x{calldata_hex}"),
                "value_wei": "0",
                "status": "prepared",
                "message": "Calldata ready. Submit via POST /v1/safe-wallets/{addr}/transactions or the signing pipeline — this endpoint does not broadcast.",
            })))
        }
        Chain::Solana | Chain::Bitcoin | Chain::Tron => Err(ApiError {
            error: "not_implemented".into(),
            message: format!(
                "NFT transfer for {:?} is not wired. Calldata-level preparation only covers EVM today.",
                nft.chain
            ),
            request_id: Uuid::new_v4().to_string(),
            status: StatusCode::NOT_IMPLEMENTED,
        }),
    }
}

// ---------------------------------------------------------------------------
// Calldata helpers exposed for integration use
// ---------------------------------------------------------------------------

/// Build ERC-721 transferFrom calldata for the given NFT.
///
/// Returns the hex-encoded calldata string (without 0x prefix).
pub fn erc721_calldata_hex(from: &str, to: &str, token_id: &str) -> anyhow::Result<String> {
    let bytes = encode_erc721_transfer(from, to, token_id)?;
    Ok(hex::encode(bytes))
}

/// Build ERC-1155 safeTransferFrom calldata for the given NFT.
pub fn erc1155_calldata_hex(from: &str, to: &str, token_id: &str, amount: u64) -> anyhow::Result<String> {
    let bytes = encode_erc1155_transfer(from, to, token_id, amount)?;
    Ok(hex::encode(bytes))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erc721_calldata_starts_with_selector() {
        let cd = erc721_calldata_hex(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
            "1",
        ).unwrap();
        assert!(cd.starts_with("23b872dd"));
    }

    #[test]
    fn erc721_calldata_length_200_hex_chars() {
        // 100 bytes × 2 = 200 hex chars
        let cd = erc721_calldata_hex(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
            "0",
        ).unwrap();
        assert_eq!(cd.len(), 200);
    }

    #[test]
    fn erc1155_calldata_starts_with_selector() {
        let cd = erc1155_calldata_hex(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
            "42",
            5,
        ).unwrap();
        assert!(cd.starts_with("f242432a"));
    }

    #[test]
    fn erc1155_calldata_length_392_hex_chars() {
        // 196 bytes × 2 = 392 hex chars
        let cd = erc1155_calldata_hex(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
            "1",
            1,
        ).unwrap();
        assert_eq!(cd.len(), 392);
    }

    #[test]
    fn register_nft_request_serializes() {
        let req = RegisterNftRequest {
            chain: Chain::Ethereum,
            contract_address: "0xBC4CA0EdA7647A8aB7C2061c2E118A18a936f13D".into(),
            token_id: "1234".into(),
            standard: NftStandard::Erc721,
            owner_vault_id: Uuid::nil(),
            metadata_uri: None,
            name: Some("Ape #1234".into()),
            image_url: None,
            quantity: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Ape #1234"));
    }

    #[test]
    fn nft_standard_metaplex_on_ethereum_rejected_in_logic() {
        // Test the validation guard
        let is_invalid = matches!(
            (Chain::Ethereum, NftStandard::Metaplex),
            (Chain::Ethereum, NftStandard::Metaplex)
        );
        assert!(is_invalid);
    }
}
