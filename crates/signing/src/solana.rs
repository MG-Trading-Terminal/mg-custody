//! Solana transaction signing.
//!
//! Signs Solana transactions with Ed25519 keys derived via SLIP-0010.
//! Key derivation path: m/44'/501'/{index}'/0'
//!
//! Handles:
//! - SOL native transfers
//! - SPL token transfers (via spl-token program)
//! - Blockhash management (transactions expire after ~90 seconds)

use anyhow::Result;

/// Parameters for a SOL native transfer.
#[derive(Debug, Clone)]
pub struct SolTransferParams {
    pub from_pubkey: [u8; 32],
    pub to_pubkey: [u8; 32],
    pub lamports: u64,
    pub recent_blockhash: [u8; 32],
}

/// Parameters for an SPL token transfer.
#[derive(Debug, Clone)]
pub struct SplTransferParams {
    pub from_token_account: [u8; 32],
    pub to_token_account: [u8; 32],
    pub authority: [u8; 32],
    pub amount: u64, // in token base units
    pub decimals: u8,
    pub recent_blockhash: [u8; 32],
}

/// Sign a SOL native transfer.
pub fn sign_sol_transfer(
    _seed: &[u8; 64],
    _index: u32,
    _params: &SolTransferParams,
) -> Result<Vec<u8>> {
    // TODO: Implement using solana-sdk
    // 1. Derive Ed25519 keypair from seed at m/44'/501'/{index}'/0' (SLIP-0010)
    // 2. Create SystemInstruction::transfer
    // 3. Build and sign Transaction with recent_blockhash
    // 4. Serialize to wire format
    anyhow::bail!("Solana signing not yet implemented")
}

/// Sign an SPL token transfer.
pub fn sign_spl_transfer(
    _seed: &[u8; 64],
    _index: u32,
    _params: &SplTransferParams,
) -> Result<Vec<u8>> {
    // TODO: Implement using spl-token
    // 1. Derive Ed25519 keypair
    // 2. Create spl_token::instruction::transfer_checked
    // 3. Build and sign Transaction
    anyhow::bail!("SPL token signing not yet implemented")
}

/// Derive the Associated Token Account (ATA) address for a wallet + mint.
///
/// ATA = PDA of [wallet_pubkey, TOKEN_PROGRAM_ID, mint_pubkey]
/// using the ASSOCIATED_TOKEN_PROGRAM_ID as the program.
pub fn derive_ata_address(wallet: &[u8; 32], mint: &[u8; 32]) -> [u8; 32] {
    // TODO: Use solana_sdk::pubkey::find_program_address
    // For now, return a placeholder
    let mut ata = [0u8; 32];
    // Simple XOR placeholder (NOT correct - replace with real PDA derivation)
    for i in 0..32 {
        ata[i] = wallet[i] ^ mint[i];
    }
    ata
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ata_derivation_deterministic() {
        let wallet = [1u8; 32];
        let mint = [2u8; 32];

        let ata1 = derive_ata_address(&wallet, &mint);
        let ata2 = derive_ata_address(&wallet, &mint);
        assert_eq!(ata1, ata2);

        // Different mint → different ATA
        let mint2 = [3u8; 32];
        let ata3 = derive_ata_address(&wallet, &mint2);
        assert_ne!(ata1, ata3);
    }
}
