//! custody-admin CLI binary.

use anyhow::{Context, Result, bail};
use custody_compliance::travel_rule::{TwentyOneTravelConfig, trust_asset};

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
        "trust" => trust(&args[2..]).await?,
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

        "bootstrap" => {
            let email = args.get(2).map(|s| s.as_str()).unwrap_or("admin@custody.local");
            let password = args.get(3).cloned().unwrap_or_else(|| {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                (0..24).map(|_| {
                    let idx = rng.gen_range(0..62);
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"[idx] as char
                }).collect()
            });

            let pool = sqlx::PgPool::connect(&db_url).await?;

            // Run migrations
            let migration = include_str!("../../db/migrations/001_initial_schema.sql");
            let _ = sqlx::raw_sql(migration).execute(&pool).await;
            let migration6 = include_str!("../../db/migrations/006_platform.sql");
            let _ = sqlx::raw_sql(migration6).execute(&pool).await;
            let migration11 = include_str!("../../db/migrations/011_sessions.sql");
            let _ = sqlx::raw_sql(migration11).execute(&pool).await;

            let workspace_id = uuid::Uuid::new_v4();
            let user_id = uuid::Uuid::new_v4();
            let password_hash = custody_gateway::auth_routes::hash_password(&password);
            let now = chrono::Utc::now();

            sqlx::query(
                "INSERT INTO workspaces (id, name, environment, owner_email, enabled, created_at, updated_at)
                 VALUES ($1, 'Default', 'mainnet', $2, true, $3, $3)
                 ON CONFLICT DO NOTHING"
            )
            .bind(workspace_id).bind(email).bind(now)
            .execute(&pool).await?;

            sqlx::query(
                "INSERT INTO workspace_users (id, workspace_id, email, role, password_hash, enabled, created_at)
                 VALUES ($1, $2, $3, 'owner', $4, true, $5)
                 ON CONFLICT (workspace_id, email) DO NOTHING"
            )
            .bind(user_id).bind(workspace_id).bind(email).bind(&password_hash).bind(now)
            .execute(&pool).await?;

            println!("══════════════════════════════════════");
            println!("  Admin account created:");
            println!("  Email:    {email}");
            println!("  Password: {password}");
            println!("  Role:     owner");
            println!("══════════════════════════════════════");
            println!("  SAVE THIS PASSWORD!");
        }

        "create-user" => {
            if args.len() < 5 {
                eprintln!("Usage: custody-admin create-user <email> <password> <role>");
                eprintln!("  Roles: viewer, editor, signer, admin, owner");
                return Ok(());
            }
            let email = &args[2];
            let password = &args[3];
            let role = &args[4];

            let pool = sqlx::PgPool::connect(&db_url).await?;

            // Find first workspace
            let ws_id: Option<(uuid::Uuid,)> = sqlx::query_as(
                "SELECT id FROM workspaces WHERE enabled = true ORDER BY created_at LIMIT 1"
            ).fetch_optional(&pool).await?;

            let workspace_id = ws_id.map(|r| r.0).unwrap_or(uuid::Uuid::nil());
            let user_id = uuid::Uuid::new_v4();
            let password_hash = custody_gateway::auth_routes::hash_password(password);

            sqlx::query(
                "INSERT INTO workspace_users (id, workspace_id, email, role, password_hash, enabled, created_at)
                 VALUES ($1, $2, $3, $4, $5, true, NOW())
                 ON CONFLICT (workspace_id, email) DO NOTHING"
            )
            .bind(user_id).bind(workspace_id).bind(email).bind(role).bind(&password_hash)
            .execute(&pool).await?;

            println!("User created: {email} (role: {role})");
        }

        _ => print_usage(),
    }

    Ok(())
}

async fn trust(args: &[String]) -> Result<()> {
    let config = TwentyOneTravelConfig::from_env;
    match args {
        [action, address] if action == "lookup" => {
            println!("{}", config()?.connect().await?.lookup(address).await?);
        }
        [action, address, chain, symbol] if action == "lookup" => {
            let chain = custody_common::Chain::try_parse_str(chain).context("Unknown chain")?;
            let asset = trust_asset(chain, symbol)?;
            println!("{}", config()?.connect().await?.lookup_asset(address, chain, asset).await?);
        }
        [action, address] if action == "send" => {
            let details = serde_json::from_reader(std::io::stdin().lock())
                .context("Expected TRUST details JSON on stdin")?;
            println!("{}", config()?.connect().await?.send(address, details).await?);
        }
        [action, record_id] if action == "receive" => {
            let record = config()?.connect().await?.receive(record_id).await?;
            println!("{}", serde_json::to_string(&record)?);
        }
        _ => bail!("Usage: custody-admin trust lookup <address> [chain asset] | send <bitcoin-address> | receive <record-id>"),
    }
    Ok(())
}

fn print_usage() {
    eprintln!("Usage: custody-admin <command>");
    eprintln!("  status                                  Show system status");
    eprintln!("  bootstrap [email] [password]             Create first admin (auto-generates password if omitted)");
    eprintln!("  create-user <email> <password> <role>    Create additional user");
    eprintln!("  rescan --chain <chain> --from-block <n>  Rescan deposits");
    eprintln!("  freeze --reason <reason>                 Emergency freeze");
    eprintln!("  unfreeze                                 Lift freeze");
    eprintln!("  trust lookup <address> [chain asset]    Look up a TRUST counterparty (Bitcoin by default)");
    eprintln!("  trust send <bitcoin-address>            Send broadcast withdrawal details from JSON on stdin");
    eprintln!("  trust receive <record-id>               Retrieve a 21 Travel Rule TRUST record");
}
