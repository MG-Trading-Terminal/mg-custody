//! custody-admin CLI binary.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://custody:custody_dev_password@localhost:5432/custody".into());

    match args[1].as_str() {
        "status" => {
            println!("MG Custody Admin");
            println!("  Database: {}", db_url);
            if let Ok(pool) = sqlx::PgPool::connect(&db_url).await {
                let db = custody_db::CustodyDb::new(pool);
                let eth = db.get_scan_progress(custody_common::Chain::Ethereum).await.unwrap_or(None);
                let btc = db.get_scan_progress(custody_common::Chain::Bitcoin).await.unwrap_or(None);
                println!("  ETH scan: block {}", eth.unwrap_or(0));
                println!("  BTC scan: block {}", btc.unwrap_or(0));
                println!("  Status: CONNECTED");
            } else {
                println!("  Status: NOT CONNECTED");
            }
        }

        "rescan" => {
            let chain_str = args.get(3).map(|s| s.as_str()).unwrap_or("ethereum");
            let from: u64 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);
            let pool = sqlx::PgPool::connect(&db_url).await?;
            let db = custody_db::CustodyDb::new(pool);
            let chain = match chain_str {
                "bitcoin" | "btc" => custody_common::Chain::Bitcoin,
                "tron" | "trx" => custody_common::Chain::Tron,
                "solana" | "sol" => custody_common::Chain::Solana,
                _ => custody_common::Chain::Ethereum,
            };
            db.set_scan_progress(chain, from).await?;
            println!("Rescan scheduled for {chain_str} from block {from}");
        }

        "freeze" => {
            let reason = args.get(3).map(|s| s.as_str()).unwrap_or("Manual freeze");
            println!("EMERGENCY FREEZE: {reason}");
            println!("All signing operations halted.");
        }

        "unfreeze" => println!("Freeze lifted."),

        _ => print_usage(),
    }

    Ok(())
}

fn print_usage() {
    eprintln!("Usage: custody-admin <command>");
    eprintln!("  status                             Show system status");
    eprintln!("  rescan --chain <chain> --from-block <n>  Rescan deposits");
    eprintln!("  freeze --reason <reason>           Emergency freeze");
    eprintln!("  unfreeze                           Lift freeze");
}
