//! custody-signer — Isolated signing service binary.
//!
//! Phase 17B: This binary runs SEPARATELY from the main custody process.
//! It has:
//!   - NO network listener (no HTTP, no TCP)
//!   - NO database connection
//!   - NO direct internet access
//!   - ONLY a Unix domain socket for IPC with the main custody process
//!
//! Security boundary: even if the API gateway is compromised, the attacker
//! cannot access private keys because they are in a different process.

use std::path::PathBuf;
use anyhow::Result;
use tracing_subscriber::EnvFilter;

mod ipc;
mod protocol;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("custody-signer starting — isolated signing service");
    tracing::info!("NO network listener, NO database, NO HTTP");

    // Load configuration
    let socket_path = std::env::var("SIGNER_SOCKET")
        .unwrap_or_else(|_| "/tmp/custody-signer.sock".into());
    let seed_file = std::env::var("CUSTODY_SEED_FILE")
        .unwrap_or_else(|_| "seed.enc".into());
    let seed_password = std::env::var("CUSTODY_SEED_PASSWORD")
        .unwrap_or_else(|_| "dev-password".into());
    let ipc_secret = std::env::var("SIGNER_IPC_SECRET")
        .unwrap_or_else(|_| "dev-ipc-secret".into());

    // Load or create seed
    let seed = load_seed(&seed_file, &seed_password)?;
    tracing::info!("Seed loaded successfully");

    // Start IPC server
    let server = ipc::IpcServer::new(
        PathBuf::from(&socket_path),
        seed,
        ipc_secret.as_bytes().to_vec(),
    );

    tracing::info!(socket = %socket_path, "Listening on Unix domain socket");

    // Run until SIGTERM
    server.run().await?;

    tracing::info!("custody-signer shutting down");
    Ok(())
}

fn load_seed(path: &str, password: &str) -> Result<[u8; 64]> {
    // Try to load encrypted seed from file
    match std::fs::read(path) {
        Ok(encrypted) => {
            let pwd = custody_signing::Password(password.as_bytes().to_vec());
            let seed = custody_signing::decrypt_seed(&encrypted, &pwd)?;
            tracing::info!("Seed decrypted from {path}");
            Ok(seed)
        }
        Err(_) => {
            // Dev mode: use zeroed seed
            tracing::warn!("No seed file found at {path} — using zeroed dev seed");
            tracing::warn!("DO NOT use this in production!");
            Ok([0u8; 64])
        }
    }
}
