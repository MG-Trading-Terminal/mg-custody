//! Custody service entry point.
//!
//! Starts the internal API server and background tasks
//! (deposit scanners, confirmation tracker, reconciliation).

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    tracing::info!("Starting custody service");

    // Build API router
    let app = custody_gateway::routes::router();

    // Start HTTP server
    let bind_addr = "0.0.0.0:8443";
    tracing::info!(addr = bind_addr, "Listening");
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
