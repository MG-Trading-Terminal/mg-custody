//! Dashboard compile-time check.
//!
//! Shells out to `npm run build` in the dashboard/ directory and
//! asserts it exits 0. Catches Svelte template / TypeScript errors
//! the Rust compiler cannot see — our readiness work touched
//! /readiness/+page.svelte and /readiness/[feature]/+page.svelte
//! multiple times; each round, nothing in `cargo test` caught a
//! broken Svelte compile.
//!
//! Gated on `CUSTODY_DASHBOARD_BUILD=1` — npm install+build adds
//! ~60s to a cold test run, which nobody wants on every `cargo
//! test`. CI sets the flag.

use std::process::Command;

fn opted_in() -> bool {
    std::env::var("CUSTODY_DASHBOARD_BUILD").ok().as_deref() == Some("1")
}

/// Locate the dashboard directory relative to this test file.
fn dashboard_dir() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR = crates/server, go up two levels then into dashboard.
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .join("dashboard")
}

#[test]
fn dashboard_prod_build_succeeds() {
    if !opted_in() {
        eprintln!("Skipping dashboard build — set CUSTODY_DASHBOARD_BUILD=1 to opt in");
        return;
    }
    let dir = dashboard_dir();
    assert!(dir.join("package.json").exists(), "dashboard/ not at {dir:?}");

    // If node_modules is missing, install first. We don't use
    // npm ci here because it erases the cache; in a dev loop the
    // existing node_modules is fine.
    if !dir.join("node_modules").exists() {
        let status = Command::new("npm")
            .arg("install")
            .current_dir(&dir)
            .status()
            .expect("npm install spawn");
        assert!(status.success(), "npm install failed");
    }

    let status = Command::new("npm")
        .args(["run", "build"])
        .current_dir(&dir)
        .status()
        .expect("npm run build spawn");
    assert!(
        status.success(),
        "Dashboard production build failed — Svelte / TypeScript compile error"
    );
}
