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

/// Associated Token Program ID (ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL).
pub const ASSOCIATED_TOKEN_PROGRAM_ID: [u8; 32] = [
    140, 151, 37, 143, 78, 36, 137, 241, 187, 61, 16, 41, 20, 142, 13, 131,
    11, 90, 19, 153, 218, 255, 16, 132, 4, 142, 123, 216, 219, 233, 248, 89,
];

/// Derive the Associated Token Account (ATA) address.
///
/// ATA = PDA([wallet, TOKEN_PROGRAM_ID, mint], ASSOCIATED_TOKEN_PROGRAM_ID)
/// Uses real `find_program_address` algorithm: tries bump seeds 255→0.
pub fn derive_ata_address(wallet: &[u8; 32], mint: &[u8; 32]) -> [u8; 32] {
    // Seeds: [wallet_pubkey, token_program_id, mint_pubkey]
    let seeds: &[&[u8]] = &[wallet, &TOKEN_PROGRAM_ID, mint];
    find_program_address(seeds, &ASSOCIATED_TOKEN_PROGRAM_ID)
}

/// Solana `find_program_address` — iterates bump from 255 down to 0.
///
/// PDA = SHA256(seeds... + bump + program_id + "ProgramDerivedAddress")
/// Must NOT be on the Ed25519 curve (that's the whole point of PDAs).
pub fn find_program_address(seeds: &[&[u8]], program_id: &[u8; 32]) -> [u8; 32] {
    for bump in (0..=255u8).rev() {
        let candidate = create_program_address(seeds, &[bump], program_id);
        if let Some(addr) = candidate {
            return addr;
        }
    }
    // Should never happen — at least one bump will produce an off-curve point
    panic!("Could not find valid PDA bump seed");
}

/// Try to create a program address with given seeds + bump.
/// Returns None if the result is on the Ed25519 curve (invalid PDA).
fn create_program_address(seeds: &[&[u8]], bump: &[u8], program_id: &[u8; 32]) -> Option<[u8; 32]> {
    let mut hasher = Sha256::new();
    for seed in seeds {
        hasher.update(seed);
    }
    hasher.update(bump);
    hasher.update(program_id);
    hasher.update(b"ProgramDerivedAddress");
    let hash = hasher.finalize();

    let mut result = [0u8; 32];
    result.copy_from_slice(&hash);

    // Check if the point is on the Ed25519 curve — PDAs must NOT be valid public keys.
    // We use ed25519-dalek's VerifyingKey::from_bytes which returns Err for off-curve points.
    if ed25519_dalek::VerifyingKey::from_bytes(&result).is_ok() {
        return None; // On curve — not a valid PDA, try next bump
    }

    Some(result)
}

/// Solana Token Program ID.
pub const TOKEN_PROGRAM_ID: [u8; 32] = [
    6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172,
    28, 180, 133, 237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0, 169,
];

/// Encode an SPL Token transfer instruction.
///
/// Instruction data: [3] (Transfer) + [amount as u64 LE]
/// Accounts: [source_ata, dest_ata, owner] with signer flag on owner.
///
/// Returns the instruction bytes for inclusion in a Solana transaction message.
pub fn encode_spl_transfer_instruction(
    source_ata: &[u8; 32],
    dest_ata: &[u8; 32],
    owner: &[u8; 32],
    amount: u64,
) -> Vec<u8> {
    // Instruction data: tag=3 (Transfer), amount as u64 LE
    let mut data = Vec::with_capacity(9);
    data.push(3); // Transfer instruction tag
    data.extend_from_slice(&amount.to_le_bytes());

    // Full instruction encoding:
    // program_id_index(1) + accounts_len(1) + account_metas(3*each) + data_len(1) + data(9)
    // For now, return just the instruction data — the message builder handles account layout.
    data
}

/// Parameters for an SPL token transfer.
#[derive(Debug, Clone)]
pub struct SplTransferParams {
    pub source_ata: [u8; 32],
    pub dest_ata: [u8; 32],
    pub owner: [u8; 32],
    pub amount: u64,        // in base units (e.g., 1_000_000 = 1 USDT)
    pub decimals: u8,
    pub mint: [u8; 32],
}

/// Sign an SPL token transfer.
///
/// Uses TransferChecked (instruction tag 12) which includes decimals for safety.
/// Returns the signed transaction bytes.
pub fn sign_spl_transfer(
    seed: &[u8; 64],
    index: u32,
    params: &SplTransferParams,
    recent_blockhash: &[u8; 32],
) -> Result<Vec<u8>> {
    let signing_key = derive_ed25519_signing_key(seed, index)?;
    let pubkey = signing_key.verifying_key().to_bytes();

    // TransferChecked instruction data: [12, amount(u64 LE), decimals(u8)]
    let mut instruction_data = Vec::with_capacity(10);
    instruction_data.push(12); // TransferChecked
    instruction_data.extend_from_slice(&params.amount.to_le_bytes());
    instruction_data.push(params.decimals);

    // Build a compact message with:
    // Accounts: [owner(signer), source_ata, mint, dest_ata, token_program]
    // This is a simplified encoding — production would use proper CompactU16 and full message format.
    let mut message = Vec::new();

    // Message header: num_required_signatures=1, num_readonly_signed=0, num_readonly_unsigned=2
    message.push(1); // num_required_signatures
    message.push(0); // num_readonly_signed_accounts
    message.push(2); // num_readonly_unsigned (mint + token_program)

    // Account keys (5 accounts)
    message.extend_from_slice(&pubkey);           // 0: owner (signer, writable)
    message.extend_from_slice(&params.source_ata); // 1: source ATA (writable)
    message.extend_from_slice(&params.dest_ata);   // 2: dest ATA (writable)
    message.extend_from_slice(&params.mint);       // 3: mint (read-only)
    message.extend_from_slice(&TOKEN_PROGRAM_ID);  // 4: token program (read-only)

    // Recent blockhash
    message.extend_from_slice(recent_blockhash);

    // Instructions: 1 instruction
    message.push(1); // num_instructions
    // Instruction: program_id_index=4, accounts=[1,2,0,3], data=instruction_data
    message.push(4); // program_id_index (token_program)
    message.push(4); // num_accounts
    message.extend_from_slice(&[1, 2, 0, 3]); // account indices: source, dest, owner, mint
    // data_len as compact-u16
    message.push(instruction_data.len() as u8);
    message.extend_from_slice(&instruction_data);

    // Sign the message
    let sig: ed25519_dalek::Signature = signing_key.sign(&message);

    // Build transaction: num_signatures(1) + signature(64) + message
    let mut tx = Vec::new();
    tx.push(1); // num_signatures
    tx.extend_from_slice(&sig.to_bytes());
    tx.extend_from_slice(&message);

    tracing::info!(
        amount = params.amount,
        decimals = params.decimals,
        "SPL token transfer signed"
    );

    Ok(tx)
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

    #[test]
    fn spl_transfer_instruction_encoding() {
        let data = encode_spl_transfer_instruction(
            &[1u8; 32], &[2u8; 32], &[3u8; 32],
            1_000_000, // 1 USDT
        );
        assert_eq!(data.len(), 9); // 1 (tag) + 8 (u64 LE)
        assert_eq!(data[0], 3); // Transfer tag
        assert_eq!(u64::from_le_bytes(data[1..9].try_into().unwrap()), 1_000_000);
    }

    #[test]
    fn sign_spl_usdt_transfer() {
        let seed = test_seed();
        let owner = sol_address(&seed, 0).unwrap();
        let source_ata = derive_ata_address(&owner, &[0xAA; 32]);
        let dest_ata = [0xBB; 32];
        let blockhash = [0xCC; 32];

        let params = SplTransferParams {
            source_ata,
            dest_ata,
            owner,
            amount: 100_000_000, // 100 USDT (6 decimals)
            decimals: 6,
            mint: [0xAA; 32],
        };

        let signed = sign_spl_transfer(&seed, 0, &params, &blockhash).unwrap();

        // Should start with num_signatures=1
        assert_eq!(signed[0], 1);
        // Signature is 64 bytes starting at index 1
        assert!(signed.len() > 65);
        // Message follows signature
        let message_start = 1 + 64;
        // Header: num_required_signatures=1
        assert_eq!(signed[message_start], 1);
    }

    #[test]
    fn sign_spl_different_amounts_different_sigs() {
        let seed = test_seed();
        let owner = sol_address(&seed, 0).unwrap();
        let blockhash = [0xDD; 32];

        let p1 = SplTransferParams {
            source_ata: [1u8; 32], dest_ata: [2u8; 32], owner,
            amount: 1_000_000, decimals: 6, mint: [3u8; 32],
        };
        let p2 = SplTransferParams {
            amount: 2_000_000, ..p1.clone()
        };

        let tx1 = sign_spl_transfer(&seed, 0, &p1, &blockhash).unwrap();
        let tx2 = sign_spl_transfer(&seed, 0, &p2, &blockhash).unwrap();
        assert_ne!(tx1, tx2, "Different amounts must produce different signed txs");
    }
}
