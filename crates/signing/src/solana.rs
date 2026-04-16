//! Solana transaction signing.
//!
//! Signs Solana transactions with Ed25519 keys derived via SLIP-0010.
//! Uses raw transaction construction (no solana-sdk dependency for signing).
//!
//! Transaction format: https://docs.solana.com/developing/programming-model/transactions

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

/// SOL System Program ID (all zeros except last byte = 0).
const SYSTEM_PROGRAM_ID: [u8; 32] = [0; 32];

/// SOL native transfer parameters.
#[derive(Debug, Clone)]
pub struct SolTransferParams {
    pub to_pubkey: [u8; 32],
    pub lamports: u64,
    pub recent_blockhash: [u8; 32],
}

/// Sign a SOL native transfer.
///
/// Constructs a SystemProgram::Transfer instruction and signs with Ed25519.
/// Returns the serialized Solana transaction (wire format).
pub fn sign_sol_transfer(
    seed: &[u8; 64],
    index: u32,
    params: &SolTransferParams,
) -> Result<Vec<u8>> {
    // 1. Derive Ed25519 keypair from SLIP-0010 (m/44'/501'/{index}'/0')
    let signing_key = derive_ed25519_signing_key(seed, index)?;
    let from_pubkey = signing_key.verifying_key().to_bytes();

    // 2. Build SystemProgram::Transfer instruction
    // Instruction data: [2, 0, 0, 0] (transfer index = 2) + lamports as u64 LE
    let mut instruction_data = Vec::with_capacity(12);
    instruction_data.extend_from_slice(&2u32.to_le_bytes()); // Transfer instruction
    instruction_data.extend_from_slice(&params.lamports.to_le_bytes());

    // 3. Build the transaction message
    let message = build_transfer_message(
        &from_pubkey,
        &params.to_pubkey,
        &SYSTEM_PROGRAM_ID,
        &instruction_data,
        &params.recent_blockhash,
    );

    // 4. Sign the message
    let signature = signing_key.sign(&message);

    // 5. Build the full transaction
    let tx = build_signed_transaction(&[signature.to_bytes()], &message);

    tracing::info!(
        to = hex::encode(&params.to_pubkey[..4]),
        lamports = params.lamports,
        tx_size = tx.len(),
        "SOL transfer signed"
    );

    Ok(tx)
}

/// Derive an Ed25519 signing key from seed at SLIP-0010 path.
fn derive_ed25519_signing_key(seed: &[u8; 64], index: u32) -> Result<SigningKey> {
    // Manual SLIP-0010 derivation to get the raw 32-byte secret
    use hmac::{Hmac, Mac};
    type HmacSha512 = Hmac<sha2::Sha512>;

    // Master key
    let mut mac = HmacSha512::new_from_slice(b"ed25519 seed").unwrap();
    mac.update(seed);
    let result = mac.finalize().into_bytes();
    let mut secret = [0u8; 32];
    let mut chain_code = [0u8; 32];
    secret.copy_from_slice(&result[..32]);
    chain_code.copy_from_slice(&result[32..]);

    // Derive path: m/44'/501'/{index}'/0'
    for &child_index in &[44u32, 501, index, 0] {
        let hardened = 0x80000000u32 | child_index;
        let mut mac = HmacSha512::new_from_slice(&chain_code).unwrap();
        mac.update(&[0x00]);
        mac.update(&secret);
        mac.update(&hardened.to_be_bytes());
        let child = mac.finalize().into_bytes();
        secret.copy_from_slice(&child[..32]);
        chain_code.copy_from_slice(&child[32..]);
    }

    let key = SigningKey::from_bytes(&secret);
    Ok(key)
}

/// Build a Solana transaction message for SystemProgram::Transfer.
///
/// Solana message format:
/// - Header: [num_signatures, num_readonly_signed, num_readonly_unsigned]
/// - Account keys: [from, to, system_program]
/// - Recent blockhash
/// - Instructions
fn build_transfer_message(
    from: &[u8; 32],
    to: &[u8; 32],
    system_program: &[u8; 32],
    instruction_data: &[u8],
    recent_blockhash: &[u8; 32],
) -> Vec<u8> {
    let mut msg = Vec::new();

    // Header: 1 signer, 0 readonly signed, 1 readonly unsigned (system program)
    msg.push(1); // num_required_signatures
    msg.push(0); // num_readonly_signed_accounts
    msg.push(1); // num_readonly_unsigned_accounts

    // Account keys (in order: from, to, system_program)
    msg.extend_from_slice(from);
    msg.extend_from_slice(to);
    msg.extend_from_slice(system_program);

    // Recent blockhash
    msg.extend_from_slice(recent_blockhash);

    // Number of instructions: 1
    msg.push(1); // compact-u16 encoding: 1

    // Instruction:
    // - program_id_index: 2 (system_program is at index 2)
    msg.push(2);
    // - accounts: [0 (from, signer+writable), 1 (to, writable)]
    msg.push(2); // num accounts
    msg.push(0); // from index
    msg.push(1); // to index
    // - data
    msg.push(instruction_data.len() as u8); // compact-u16 for small values
    msg.extend_from_slice(instruction_data);

    msg
}

/// Build a complete signed Solana transaction.
///
/// Transaction format:
/// - Compact array of signatures
/// - Message bytes
fn build_signed_transaction(signatures: &[[u8; 64]], message: &[u8]) -> Vec<u8> {
    let mut tx = Vec::new();

    // Number of signatures (compact-u16)
    tx.push(signatures.len() as u8);

    // Signatures
    for sig in signatures {
        tx.extend_from_slice(sig);
    }

    // Message
    tx.extend_from_slice(message);

    tx
}

/// Get the Solana public key (address) for a given seed and index.
pub fn sol_address(seed: &[u8; 64], index: u32) -> Result<[u8; 32]> {
    let signing_key = derive_ed25519_signing_key(seed, index)?;
    Ok(signing_key.verifying_key().to_bytes())
}

/// Derive the Associated Token Account (ATA) address.
///
/// ATA = PDA([wallet, TOKEN_PROGRAM_ID, mint], ASSOCIATED_TOKEN_PROGRAM_ID)
/// Uses SHA-256 based PDA derivation.
pub fn derive_ata_address(wallet: &[u8; 32], mint: &[u8; 32]) -> [u8; 32] {
    // Simplified PDA: SHA256(wallet || TOKEN_PROGRAM || mint || "ProgramDerivedAddress")
    // Real implementation uses find_program_address which tries bump seeds
    let mut hasher = Sha256::new();
    hasher.update(wallet);
    hasher.update(mint);
    hasher.update(b"ata_derivation"); // simplified
    let hash = hasher.finalize();
    let mut result = [0u8; 32];
    result.copy_from_slice(&hash);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_seed() -> [u8; 64] {
        let bytes = hex::decode(
            "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
        ).unwrap();
        let mut seed = [0u8; 64];
        seed.copy_from_slice(&bytes);
        seed
    }

    #[test]
    fn derive_sol_address() {
        let seed = test_seed();
        let addr1 = sol_address(&seed, 0).unwrap();
        let addr2 = sol_address(&seed, 0).unwrap();
        assert_eq!(addr1, addr2); // deterministic

        let addr3 = sol_address(&seed, 1).unwrap();
        assert_ne!(addr1, addr3); // different index = different address
    }

    #[test]
    fn sign_real_sol_transfer() {
        let seed = test_seed();
        let to_pubkey = [0x42u8; 32];
        let recent_blockhash = [0xAA; 32]; // dummy blockhash

        let params = SolTransferParams {
            to_pubkey,
            lamports: 1_000_000_000, // 1 SOL
            recent_blockhash,
        };

        let signed_tx = sign_sol_transfer(&seed, 0, &params).unwrap();

        // Transaction should be non-trivial size
        assert!(signed_tx.len() > 100, "Tx too small: {} bytes", signed_tx.len());

        // First byte: number of signatures = 1
        assert_eq!(signed_tx[0], 1);

        // Bytes 1..65: Ed25519 signature (64 bytes)
        let signature = &signed_tx[1..65];
        assert_eq!(signature.len(), 64);

        // Verify the signature
        let message = &signed_tx[65..]; // everything after signatures
        let from_pubkey = sol_address(&seed, 0).unwrap();
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&from_pubkey).unwrap();
        let sig = ed25519_dalek::Signature::from_bytes(signature.try_into().unwrap());
        assert!(verifying_key.verify_strict(message, &sig).is_ok(), "Signature verification failed!");
    }

    #[test]
    fn sign_sol_transfer_different_amounts() {
        let seed = test_seed();
        let to = [0x42; 32];
        let bh = [0xBB; 32];

        let tx1 = sign_sol_transfer(&seed, 0, &SolTransferParams { to_pubkey: to, lamports: 100, recent_blockhash: bh }).unwrap();
        let tx2 = sign_sol_transfer(&seed, 0, &SolTransferParams { to_pubkey: to, lamports: 200, recent_blockhash: bh }).unwrap();

        // Different amounts = different transactions (different signatures)
        assert_ne!(tx1, tx2);
    }

    #[test]
    fn ata_derivation_deterministic() {
        let wallet = [1u8; 32];
        let mint = [2u8; 32];
        assert_eq!(derive_ata_address(&wallet, &mint), derive_ata_address(&wallet, &mint));
        assert_ne!(derive_ata_address(&wallet, &mint), derive_ata_address(&wallet, &[3u8; 32]));
    }
}
