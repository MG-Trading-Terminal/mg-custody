//! custody-seed CLI — generate, verify, and backup HD wallet seeds.

mod seed_tool;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: custody-seed <command>");
        eprintln!("  generate --output <file> --password <pw>   Create new encrypted seed");
        eprintln!("  verify --input <file> --password <pw>      Verify seed file");
        eprintln!("  backup --input <file> --password <pw>      Create Shamir 3-of-5 backup");
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
            seed_tool::generate_seed(&output, &password)?;
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
        other => eprintln!("Unknown command: {other}"),
    }

    Ok(())
}
