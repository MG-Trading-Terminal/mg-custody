# Testing Guide

## Running Tests

```bash
# All tests (recommended)
cargo test

# Specific crate
cargo test -p custody-signing

# Specific test file
cargo test -p custody-server --test api_integration

# With output
cargo test -- --nocapture
```

## Test Categories

### Unit Tests (always run, no deps)
- **custody-common**: 1,177+ tests — types, crypto, validation, token database
- **custody-signing**: 78 tests — ETH/BTC/TRX/SOL signing, Shamir, encryption
- **custody-scanner**: 37 tests — uint256 conversion, TRC-20 parsing, SPL parsing
- **custody-policy**: 50 tests — rules, approvals, RBAC
- **custody-mpc**: 90 tests — CGGMP21, FROST, PKCS#11 HSM

### Integration Tests (need DATABASE_URL for DB tests)
- **api_integration**: 13 tests — full HTTP stack via tower::oneshot
- **e2e_testnet**: 9 tests — live Sepolia/Solana/Tron + offline signing
- **withdrawal_security**: 17 tests — pipeline + signing + HMAC
- **disaster_recovery**: 6 tests — Shamir backup, encrypted seed, key rotation

### Live Network Tests (soft-fail if RPC unavailable)
- Sepolia (Ethereum testnet)
- Tron Nile (Tron testnet)
- Solana Devnet

## Test Coverage

### Critical Paths (must maintain >90% coverage)

| Path | Tests | Coverage |
|------|-------|----------|
| Signing (ETH/BTC/TRX/SOL) | 78 | High |
| Scanner (deposit detection) | 37 | High |
| Withdrawal pipeline (state machine) | 21 | High |
| Circuit breaker | 10+ | High |
| HMAC auth | 8+ | High |
| Shamir secret sharing | 15 | 100% (all 10 combos verified) |
| Address derivation | 41 | High |

### Generating Coverage Report

```bash
# Install (one-time)
cargo install cargo-tarpaulin

# Generate HTML report
cargo tarpaulin --out html --output-dir coverage/
open coverage/tarpaulin-report.html

# Or with llvm-cov (requires nightly + llvm-tools)
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov
cargo llvm-cov --html --output-dir coverage/
```

## Writing Tests

### Rules
1. Every financial calculation needs a test with known values
2. Every signing function needs a determinism test (same input → same output)
3. Every chain scanner needs a decimal conversion test
4. Never test against imaginary API methods — verify they exist first
5. Use `dec!()` macro for Decimal literals, never `Decimal::from_f64`

### Patterns

```rust
// Soft-fail for tests needing external services
let Some(db) = get_db().await else {
    eprintln!("Skipping: DATABASE_URL not set");
    return;
};

// Live network tests
if !can_connect().await {
    eprintln!("Skipping: RPC unavailable");
    return;
}
```

## CI Pipeline

```yaml
# Required checks:
- cargo fmt --check
- cargo check --all-targets
- cargo test
- cargo clippy --all-targets
- cargo audit
- cargo deny check advisories bans licenses
```
