# Contributing to MG Custody

## How the Code is Organized

```
crates/
  common/         ← Types, config, traits (start here to understand the domain)
  db/             ← PostgreSQL layer (real SQL queries)
  address-factory ← HD wallet derivation (BIP-32/44/84, SLIP-0010)
  signing/        ← Transaction signing (ETH, BTC, SOL, TRX — all REAL)
  mpc/            ← Threshold signing (FROST + CGGMP21)
  scanner/        ← Blockchain deposit detection
  fee-estimator/  ← Gas/fee estimation per chain
  broadcaster/    ← Transaction broadcast
  confirmations/  ← Confirmation tracking + webhooks
  withdrawal-pipeline/ ← State machine (Received → Finalized)
  policy/         ← Rules engine, RBAC, whitelisting
  compliance/     ← AML, OFAC, Travel Rule
  wallet-manager/ ← Hot/warm/cold tiering
  utxo/           ← Bitcoin UTXO management
  reconciliation/ ← Proof of Reserves
  gateway/        ← HTTP API (axum)
  server/         ← Binary + background tasks
```

## Running Tests

```bash
# Unit tests (no external deps)
cargo test

# With PostgreSQL (start with docker compose up -d postgres)
DATABASE_URL=postgres://custody:custody_dev_password@localhost:5432/custody cargo test

# Live Sepolia tests (connects to real Ethereum testnet)
cargo test -p custody-server --test sepolia_live
cargo test -p custody-server --test sepolia_scanner
```

## Adding a New Chain

1. Add address derivation in `crates/address-factory/src/{chain}.rs`
2. Add signing in `crates/signing/src/{chain}.rs` — MUST include sign+verify test
3. Add scanner in `crates/scanner/src/{chain}.rs`
4. Add fee estimator in `crates/fee-estimator/src/{chain}.rs`
5. Add broadcaster in `crates/broadcaster/src/{chain}.rs`
6. Update `config/default.toml` with chain config
7. Add to `common/src/types.rs` Chain enum

## Security

**DO NOT open a public GitHub issue for security vulnerabilities.**
See [SECURITY.md](SECURITY.md) for reporting instructions.

Before submitting code that touches signing, keys, or crypto:
- Read [THREAT_MODEL.md](THREAT_MODEL.md)
- Run `cargo test -p custody-common security_tests`
- Run `cargo deny check`

## Architecture Docs

- [SECURITY.md](SECURITY.md) — Trust boundaries, key flow, deployment prerequisites
- [THREAT_MODEL.md](THREAT_MODEL.md) — STRIDE analysis, attack vectors
- [docs/DEPLOYMENT_MODES.md](docs/DEPLOYMENT_MODES.md) — 4 deployment modes with honest security properties

## Crates (19 total)

In addition to the crates above:
- `signer-service/` — Isolated signing binary with Unix socket IPC (Phase 17B)

## Rules

- **NEVER use f64 for money** — `rust_decimal::Decimal` everywhere
- **NEVER log key material** — custom Debug impls that print `***`
- **All key types must Zeroize** — `#[derive(Zeroize, ZeroizeOnDrop)]`
- **Every signing function must have a verify test** — sign → verify roundtrip
- **Amounts as strings in JSON** — never float in serialization
- **Idempotent API** — duplicate calls must be safe
- **Run `cargo deny check`** before submitting — license + advisory compliance
