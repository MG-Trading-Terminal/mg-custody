//! custody-seed CLI — generate, verify, backup, and restore HD wallet seeds.
//!
//! Commands:
//!   generate   — Create new seed with 24-word BIP-39 mnemonic
//!   verify     — Verify encrypted seed file can be decrypted
//!   backup     — Create Shamir 3-of-5 backup shares
//!   restore    — Restore seed from mnemonic or Shamir shares
//!   show       — Show mnemonic for existing seed file

mod seed_tool;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    let get_flag = |flag: &str| -> Option<String> {
        args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
    };

    match args[1].as_str() {
        "generate" => {
            let output = get_flag("--output").unwrap_or("seed.enc".into());
            let password = get_flag("--password")
                .expect("--password required for generate");

            // Generate BIP-39 mnemonic
            let (phrase, seed) = custody_signing::mnemonic::generate_mnemonic()?;

            // Show mnemonic to user
            println!("════════════════════════════════════════════════════");
            println!("  YOUR 24-WORD RECOVERY PHRASE");
            println!("  Write these words down and store in a safe place!");
            println!("════════════════════════════════════════════════════");
            println!();
            println!("{}", custody_signing::mnemonic::format_mnemonic_for_display(&phrase));
            println!("════════════════════════════════════════════════════");
            println!("  WARNING: This phrase will NOT be shown again!");
            println!("  Anyone with these words can access ALL your funds.");
            println!("════════════════════════════════════════════════════");

            // Encrypt and save
            let pw = custody_signing::Password::from_str(&password);
            let encrypted = custody_signing::encrypt_seed(&seed, &pw)?;
            std::fs::write(&output, &encrypted)?;
            println!("\nEncrypted seed saved to: {output}");

            // Show first addresses for verification
            let eth_addr = custody_signing::ethereum::eth_address(&seed, 0)?;
            println!("\nFirst addresses (for verification):");
            println!("  ETH: {eth_addr:#x}");
        }
        "verify" => {
            let input = get_flag("--input").unwrap_or("seed.enc".into());
            let password = get_flag("--password")
                .expect("--password required for verify");
            seed_tool::verify_seed(&input, &password)?;
        }
        "backup" => {
            let input = get_flag("--input").unwrap_or("seed.enc".into());
            let password = get_flag("--password")
                .expect("--password required for backup");
            seed_tool::create_backup(&input, &password, 5, 3)?;
        }
        "restore" => {
            let output = get_flag("--output").unwrap_or("seed.enc".into());
            let password = get_flag("--password")
                .expect("--password required for restore");

            if let Some(mnemonic) = get_flag("--mnemonic") {
                // Restore from mnemonic phrase
                println!("Restoring from mnemonic...");
                let seed = custody_signing::mnemonic::mnemonic_to_seed(&mnemonic, "")?;

                let pw = custody_signing::Password::from_str(&password);
                let encrypted = custody_signing::encrypt_seed(&seed, &pw)?;
                std::fs::write(&output, &encrypted)?;

                let eth_addr = custody_signing::ethereum::eth_address(&seed, 0)?;
                println!("Seed restored and encrypted to: {output}");
                println!("  ETH address: {eth_addr:#x}");
                println!("Verify this matches your original address!");
            } else if let Some(shares_dir) = get_flag("--shares-dir") {
                // Restore from Shamir shares
                println!("Restoring from Shamir shares in {shares_dir}...");
                let mut shares = Vec::new();

                for entry in std::fs::read_dir(&shares_dir)? {
                    let entry = entry?;
                    if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                        let data = std::fs::read_to_string(entry.path())?;
                        let share: custody_signing::shamir::Share = serde_json::from_str(&data)?;
                        shares.push(share);
                    }
                }

                println!("Found {} shares", shares.len());
                anyhow::ensure!(shares.len() >= 3, "Need at least 3 shares (have {})", shares.len());

                let seed_bytes = custody_signing::shamir::reconstruct(&shares[..3])?;
                anyhow::ensure!(seed_bytes.len() == 64, "Reconstructed seed wrong size");
                let mut seed = [0u8; 64];
                seed.copy_from_slice(&seed_bytes);

                let pw = custody_signing::Password::from_str(&password);
                let encrypted = custody_signing::encrypt_seed(&seed, &pw)?;
                std::fs::write(&output, &encrypted)?;

                let eth_addr = custody_signing::ethereum::eth_address(&seed, 0)?;
                println!("Seed restored from {} shares and encrypted to: {output}", shares.len());
                println!("  ETH address: {eth_addr:#x}");
                println!("Verify this matches your original address!");
            } else {
                eprintln!("Error: restore requires --mnemonic \"word1 word2 ...\" or --shares-dir <dir>");
            }
        }
        "show-mnemonic" => {
            // This is a DANGEROUS operation — shows the mnemonic for an existing seed
            let input = get_flag("--input").unwrap_or("seed.enc".into());
            let password = get_flag("--password")
                .expect("--password required");

            eprintln!("WARNING: Displaying recovery phrase. Ensure no one is watching your screen.");
            eprintln!("Press Enter to continue...");
            let mut _buf = String::new();
            std::io::stdin().read_line(&mut _buf).ok();

            let encrypted = std::fs::read(&input)?;
            let pw = custody_signing::Password::from_str(&password);
            let seed = custody_signing::decrypt_seed(&encrypted, &pw)?;

            // Convert first 32 bytes of seed to mnemonic entropy
            let mut entropy = [0u8; 32];
            entropy.copy_from_slice(&seed[..32]);
            let phrase = custody_signing::mnemonic::entropy_to_mnemonic(&entropy)?;

            println!();
            println!("{}", custody_signing::mnemonic::format_mnemonic_for_display(&phrase));
            println!();
            eprintln!("NOTE: This mnemonic is derived from the first 32 bytes of your seed.");
            eprintln!("For full seed recovery, use Shamir backup instead.");
        }
        other => {
            eprintln!("Unknown command: {other}");
            print_usage();
        }
    }

    Ok(())
}

fn print_usage() {
    eprintln!("Usage: custody-seed <command>");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  generate --output <file> --password <pw>               Create new seed + show 24-word mnemonic");
    eprintln!("  verify --input <file> --password <pw>                  Verify encrypted seed file");
    eprintln!("  backup --input <file> --password <pw>                  Create Shamir 3-of-5 backup shares");
    eprintln!("  restore --mnemonic \"word1 word2 ...\" --password <pw>   Restore from 24-word phrase");
    eprintln!("  restore --shares-dir <dir> --password <pw>             Restore from Shamir shares");
    eprintln!("  show-mnemonic --input <file> --password <pw>           Display recovery phrase (DANGEROUS)");
}
