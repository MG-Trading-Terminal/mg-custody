//! Full binary startup smoke.
//!
//! Spawns the actual `custody` binary against a real PostgreSQL
//! (DATABASE_URL). The test curls three endpoints and asserts they
//! respond with the expected shape — proving the whole boot chain
//! works end-to-end: config load, migration apply (including the
//! 036/037/038 readiness tables), background task spawn, router
//! bind, Prometheus registry init.
//!
//! Unit tests cover each piece individually; this test covers the
//! integration between them.
//!
//! Gated on `DATABASE_URL` and `CUSTODY_BINARY_SMOKE=1`. Without
//! the opt-in flag it stays silent — starting the full binary
//! takes a few seconds and a developer's test run should not pay
//! that cost for every `cargo test`.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn opted_in() -> bool {
    std::env::var("CUSTODY_BINARY_SMOKE").ok().as_deref() == Some("1")
        && std::env::var("DATABASE_URL").is_ok()
}

/// Wait for `GET http://.../health` to return 200, or give up.
async fn wait_for_health(port: u16, deadline: Instant) -> bool {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .unwrap();
    while Instant::now() < deadline {
        match client
            .get(format!("http://127.0.0.1:{port}/health"))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => return true,
            _ => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }
    false
}

#[tokio::test]
async fn binary_boots_migrations_apply_health_features_responds() {
    if !opted_in() {
        eprintln!(
            "Skipping binary smoke — set CUSTODY_BINARY_SMOKE=1 and DATABASE_URL to opt in"
        );
        return;
    }
    // Bind a random port so parallel runs don't collide. :0 lets
    // the OS pick; the custody binary listens on CUSTODY_API__BIND
    // which we set below.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let p = l.local_addr().unwrap().port();
        drop(l);
        p
    };

    let binary = env!("CARGO_BIN_EXE_custody");
    let mut child = Command::new(binary)
        .env("CUSTODY_API__BIND", format!("127.0.0.1:{port}"))
        // Disable external-probe noise during the smoke — we're
        // testing the boot + HTTP layer, not vendor reachability.
        .env_remove("CUSTODY_CHAINALYSIS_API_KEY")
        .env_remove("CUSTODY_ELLIPTIC_API_KEY")
        .env_remove("CUSTODY_FCM__SERVICE_ACCOUNT_JSON")
        .env_remove("CUSTODY_APNS__KEY_P8")
        .env_remove("CUSTODY_VAPID__PRIVATE_KEY_PEM")
        .env_remove("CUSTODY_MPC__ENABLED")
        .env_remove("HSM_LIB_PATH")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn custody binary");

    // Wait up to 60s for health. Migrations against a cold PG can
    // take several seconds on first run.
    let booted = wait_for_health(port, Instant::now() + Duration::from_secs(60)).await;
    if !booted {
        let _ = child.kill();
        let out = child.wait_with_output().ok();
        panic!(
            "custody did not boot within 60s. stderr:\n{}",
            out.map(|o| String::from_utf8_lossy(&o.stderr).to_string())
                .unwrap_or_default()
        );
    }

    // Clean-up guard — always kill before we return, even on panic.
    struct Guard(std::process::Child);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let _guard = Guard(child);

    let http = reqwest::Client::new();

    // /v1/health/features — shape the dashboard expects.
    let resp = http
        .get(format!("http://127.0.0.1:{port}/v1/health/features"))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "features endpoint must 200");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["summary"]["total"].as_i64().unwrap() > 0, "some features should register");
    assert!(body["features"].is_array());

    // /metrics — Prometheus registry populated with readiness gauges.
    let resp = http
        .get(format!("http://127.0.0.1:{port}/metrics"))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let text = resp.text().await.unwrap();
    assert!(
        text.contains("custody_readiness_feature_state"),
        "readiness gauges must appear in /metrics after first refresh tick: {text:.200}"
    );
    assert!(
        text.contains("custody_readiness_last_refresh_seconds"),
        "refresh timestamp gauge must appear"
    );
}
