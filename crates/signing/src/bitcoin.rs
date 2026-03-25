//! Bitcoin P2WPKH transaction signing.
//!
//! Constructs and signs SegWit (P2WPKH) transactions from UTXOs.
//! Derives the signing key from the HD seed at m/84'/0'/0'/0/{index}.
//!
//! Key features:
//! - P2WPKH (native SegWit) only
//! - Change goes to a fresh derived address
//! - RBF signaling (BIP-125): nSequence = 0xFFFFFFFD
//! - Private key zeroized immediately after signing

use anyhow::Result;

/// Bitcoin transaction input (from a UTXO).
#[derive(Debug, Clone)]
pub struct BtcTxInput {
    pub txid: String,
    pub vout: u32,
    pub amount_sats: u64,
    /// Derivation index for the key that controls this UTXO.
    pub derivation_index: u32,
}

/// Bitcoin transaction output.
#[derive(Debug, Clone)]
pub struct BtcTxOutput {
    pub address: String,
    pub amount_sats: u64,
}

/// Parameters for a Bitcoin transaction.
#[derive(Debug, Clone)]
pub struct BtcTxParams {
    pub inputs: Vec<BtcTxInput>,
    pub outputs: Vec<BtcTxOutput>,
    /// Enable RBF (BIP-125) for fee bumping.
    pub rbf: bool,
}

/// Sign a Bitcoin P2WPKH transaction.
///
/// Returns the serialized signed transaction ready for broadcast.
pub fn sign_bitcoin_tx(
    _seed: &[u8; 64],
    _params: &BtcTxParams,
) -> Result<Vec<u8>> {
    // TODO: Implement using the `bitcoin` crate:
    // 1. Create Transaction with inputs (nSequence = 0xFFFFFFFD for RBF)
    // 2. For each input, derive the private key at m/84'/0'/0'/0/{index}
    // 3. Create the P2WPKH witness program
    // 4. Sign each input using BIP-143 (SegWit signing)
    // 5. Serialize the transaction
    // 6. Zeroize all private keys
    anyhow::bail!("Bitcoin signing not yet implemented — requires bitcoin crate integration")
}

/// Construct the P2WPKH scriptPubKey for a public key hash.
///
/// OP_0 <20-byte-pubkey-hash>
pub fn p2wpkh_script_pubkey(pubkey_hash: &[u8; 20]) -> Vec<u8> {
    let mut script = Vec::with_capacity(22);
    script.push(0x00); // OP_0 (witness version 0)
    script.push(0x14); // Push 20 bytes
    script.extend_from_slice(pubkey_hash);
    script
}

/// RBF sequence number (BIP-125): signals replaceability.
pub const RBF_SEQUENCE: u32 = 0xFFFF_FFFD;

/// Final sequence number: no RBF.
pub const FINAL_SEQUENCE: u32 = 0xFFFF_FFFF;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p2wpkh_script_format() {
        let pubkey_hash = [0xAB; 20];
        let script = p2wpkh_script_pubkey(&pubkey_hash);

        assert_eq!(script.len(), 22);
        assert_eq!(script[0], 0x00); // OP_0
        assert_eq!(script[1], 0x14); // Push 20 bytes
        assert_eq!(&script[2..], &pubkey_hash);
    }

    #[test]
    fn rbf_sequence_values() {
        // BIP-125: nSequence < 0xFFFFFFFE signals RBF
        assert!(RBF_SEQUENCE < 0xFFFF_FFFE);
        assert_eq!(FINAL_SEQUENCE, 0xFFFF_FFFF);
    }
}
