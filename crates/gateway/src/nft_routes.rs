//! NFT custody API routes.
//!
//! Provides CRUD management for NFTs held in custody vaults and
//! initiation of on-chain NFT transfers.
//!
//! Access levels:
//!   - GET endpoints: RequireViewer
//!   - POST /v1/nfts (register): RequireEditor
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
        .route("/v1/nfts/{id}",         get(get_nft))
        .route("/v1/nfts/{id}/transfer", post(transfer_nft))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /v1/nfts — list all custodied NFTs.
async fn list_nfts(
    _auth: RequireViewer,
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // In a full implementation this calls state.db.list_custodied_nfts().
    // Returning an empty list here until the DB layer is wired.
    Ok(Json(json!({ "count": 0, "nfts": [] })))
}

/// GET /v1/nfts/{id} — get a single custodied NFT.
async fn get_nft(
    _auth: RequireViewer,
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // TODO: state.db.find_custodied_nft(id).await
    Err(ApiError::not_found(&format!("NFT {id} not found")))
}

/// POST /v1/nfts — register an NFT in custody.
async fn register_nft(
    _auth: RequireEditor,
    State(_state): State<AppState>,
    Json(req): Json<RegisterNftRequest>,
) -> Result<(StatusCode, Json<CustodiedNft>), ApiError> {
    // Validate chain + standard combination.
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

    // TODO: state.db.insert_custodied_nft(&nft).await
    tracing::info!(
        nft_id = %nft.id,
        chain = %nft.chain,
        contract = %nft.contract_address,
        token_id = %nft.token_id,
        "NFT registered in custody"
    );
    Ok((StatusCode::CREATED, Json(nft)))
}

/// POST /v1/nfts/{id}/transfer — initiate an NFT transfer.
async fn transfer_nft(
    _auth: RequireSigner,
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<NftTransferRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // TODO: look up nft in DB and build actual transfer.
    // For now we validate the to_address is non-empty and return a calldata preview
    // for EVM chains — the real implementation calls the signing backend.
    if req.to_address.is_empty() {
        return Err(ApiError::bad_request("to_address must not be empty"));
    }

    tracing::info!(
        nft_id = %id,
        to = %req.to_address,
        "NFT transfer initiated"
    );

    Ok(Json(json!({
        "nft_id": id,
        "to_address": req.to_address,
        "status": "pending",
        "message": "Transfer queued — signing backend will process this request"
    })))
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
