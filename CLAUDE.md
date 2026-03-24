# MG Custody

## What
Self-hosted cryptocurrency custody service. Manages private keys, signs transactions, monitors deposits, processes withdrawals.
Standalone Rust project, communicates with the exchange (~/Projects/exchange/) via REST API + webhooks.
14 crates, 4 chains (Bitcoin, Ethereum, Tron, Solana).

## Structure
```
crates/
  common/              # Shared types: Chain, Asset, Deposit, Withdrawal, CustodyEvent
  db/                  # PostgreSQL persistence (sqlx): addresses, deposits, withdrawals, ledger, audit
  gateway/             # axum REST API (internal only, HMAC-SHA256 auth)
  scanner/             # ChainScanner trait + per-chain deposit detection (block polling)
  confirmations/       # Confirmation tracking + webhook dispatch to exchange
  address-factory/     # BIP32/BIP44 HD wallet key derivation + per-chain address encoding
  signing/             # SigningBackend trait (software keys → HSM → MPC)
  broadcaster/         # Per-chain transaction broadcast + stuck tx rebroadcast
  withdrawal-pipeline/ # State machine: Received → Validated → Signed → Broadcast → Confirmed
  wallet-manager/      # Hot/warm/cold tiering (5%/15%/80%) + sweep logic
  utxo/                # Bitcoin UTXO set management + coin selection (Branch and Bound)
  fee-estimator/       # Per-chain network fee queries
  reconciliation/      # On-chain vs internal ledger balance reconciliation
  server/              # Binary entry point, background task orchestration
```

## Commands
```bash
cargo build --release
cargo test
cargo test -p custody-common
cargo test -p custody-gateway
cargo check
cargo run --release --bin custody    # Start on :8443
```

## Code Style
- Rust 2024 edition, `anyhow::Result` for errors, `tracing` for logs
- **NEVER use f64 for money** — `rust_decimal` everywhere, no exceptions
- **`zeroize`** on ALL types holding key material — seeds, private keys, key shares
- All amounts transmitted as string-encoded Decimal (never float in JSON)
- Idempotency: every API endpoint must handle duplicate calls safely
- Double-entry: every fund movement has a corresponding debit and credit in the custody ledger

## Security Rules (CRITICAL)

This service holds private keys and signs transactions. A bug here is not a data leak — it is direct, irreversible loss of user funds.

### Key Material
- Private keys, seeds, and key shares MUST derive `Zeroize` and `ZeroizeOnDrop`
- Key material MUST NEVER appear in logs — filter before any `tracing::` call
- Key material MUST NEVER be serialized to JSON or stored in plaintext
- HSM-backed storage for production; software keys only for dev/test
- Every key derivation operation logged in audit trail (path, user_id, timestamp — NOT the key)

### Signing Service
- The signing service validates every transaction before signing:
  - Destination address matches the approved withdrawal record
  - Amount matches the approved withdrawal record
  - Chain matches the approved withdrawal record
- Signing requests carry HMAC from the approval layer — verify before signing
- The signing service has NO public network interface

### Network Isolation
- Zone 1 (DMZ): API gateway only
- Zone 2 (Internal): scanners, withdrawal pipeline, blockchain nodes
- Zone 3 (Air-gapped): signing service + HSM — no internet, no inbound except Zone 2

### Circuit Breaker
- Per-transaction limit: $500K
- Hourly outflow limit: $10M
- Daily outflow limit: $50M
- Circuit breaker state MUST be persisted (not in-memory only)
- Circuit breaker trip MUST fire an alert (PagerDuty/webhook), not just log

## Chain-Specific Rules

### Bitcoin (UTXO model)
- P2WPKH (Bech32) addresses for hot wallet
- UTXO selection: Branch and Bound, fallback to largest-first
- Change goes to fresh address (never reuse sender)
- RBF (BIP125): support fee bumping for stuck transactions
- 3 confirmations for deposits

### Ethereum (Account model)
- EIP-1559 transactions only (type 2)
- Nonce manager: single-threaded per address, re-read from chain on restart
- ERC-20 decimals: USDT=6, USDC=6, ETH=18 — unit tests mandatory for each
- 12 confirmations for deposits
- Gas price cap in config — reject/queue withdrawals when gas exceeds cap

### Tron
- secp256k1 (same as ETH), Base58Check address encoding
- TRC-20 USDT contract: TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t
- Monitor Energy balance — TRC-20 transfers fail without Energy
- 20 confirmations for deposits

### Solana
- Ed25519 keys (SLIP-0010 derivation, NOT standard BIP32)
- Create Associated Token Accounts (ATAs) eagerly at address generation
- Transactions expire after ~90 seconds (blockhash TTL) — retry with fresh blockhash
- Use "finalized" commitment level for deposit crediting
- SPL token decimals: USDT=6, USDC=6

## API Contract (Internal Only)

### Exchange → Custody
```
POST /v1/addresses          {user_id, asset, chain} → {address, chain, memo}
POST /v1/withdrawals        {withdrawal_id, asset, chain, address, amount} → 202 {custody_ref, status}
GET  /v1/withdrawals/:id    → {status, tx_hash, confirmations, error}
GET  /v1/wallets/:asset     → {hot_balance, warm_balance, total_managed}
GET  /health                → "ok"
GET  /health/chains         → per-chain node status
```

### Custody → Exchange (Webhooks)
```
deposit_confirmed     — user deposit reached required confirmations
withdrawal_broadcast  — signed tx submitted to network
withdrawal_confirmed  — tx confirmed on-chain
withdrawal_failed     — tx failed after retries
```

Auth: HMAC-SHA256 of (method + path + timestamp + body). Reject stale requests (>30s).

## Withdrawal Approval Tiers

| Tier | Amount | Approval | Delay |
|------|--------|----------|-------|
| 0 | < $1K + whitelisted | Auto | 1-5 min |
| 1 | $1K - $10K | Auto + alert | 10 min |
| 2 | $10K - $100K | 1 approver (FIDO2) | 30 min |
| 3 | $100K - $500K | 2 approvers | 2 hours |
| 4 | > $500K | 3 approvers, cold wallet | 24 hours |

## Key Design Decisions
- Provider trait pattern: `SigningBackend` trait allows swapping software → HSM → MPC
- `ChainScanner` trait: per-chain implementations behind a common interface
- All state in PostgreSQL — restart loses nothing
- Webhook delivery: at-least-once with exponential backoff (exchange must be idempotent)
- Amounts as strings in JSON, Decimal in Rust — never float anywhere

## Configuration
- `config/default.toml` — base config with chain endpoints, limits, wallet policies
- Environment overrides via `CUSTODY_` prefix (e.g., `CUSTODY_DATABASE__URL`)
- Secrets (HMAC keys, DB credentials) via env vars or HashiCorp Vault

## Related Projects
- `~/Projects/exchange/` — the exchange monolith this service integrates with
- `~/Projects/exchange/crates/custody/` — exchange-side CustodyProvider trait and SelfHostedCustodyProvider HTTP client
- `~/Projects/exchange/docs/roadmaps/self-hosted-custody-research.md` — full research document

## Testing
- `cargo test` — all unit tests
- Integration tests require `DATABASE_URL` env var (PostgreSQL)
- Chain integration tests require node RPC URLs (skip in CI without them)
- Every financial calculation needs explicit test with known values
- Never write tests against imaginary API methods — verify they exist first

## Documentation Requirements (MANDATORY)

Every session MUST maintain these documents. No work starts without reading them; no work ends without updating them.

### CHANGELOG.md
- Every change logged with date, category (Added/Changed/Fixed/Removed), and what was done
- Include reference links for any external source used (BIP, EIP, docs)
- Keep [Unreleased] section current; tag versions on milestones

### ROADMAP.md
- Phase-based roadmap with checkboxes
- Mark tasks as done immediately when completed
- Add new tasks discovered during implementation

### SPRINTS.md
- Sprint log with goals, completed tasks, metrics (tests, lines, crates)
- Each sprint records: what was planned, what was done, references used
- Track: test count, crate count, lines of code — quantify progress

### REFERENCES.md
- Every formula, threshold, mechanism must cite its source
- No financial logic ships without a reference
- Table format: Mechanism | Reference | Used In

### Session Workflow
1. **Start**: Read ROADMAP.md + SPRINTS.md to understand current state
2. **Plan**: Enter plan mode for non-trivial work (3+ steps)
3. **Implement**: Write code, run `cargo test`
4. **Document**: Update CHANGELOG.md, ROADMAP.md (check off tasks)
5. **End**: Update SPRINTS.md with what was done, metrics, references used

### Per-Crate Documentation
- Each crate's `lib.rs` has a module-level doc comment explaining purpose
- Each public type/function has a doc comment
- Chain-specific crates document which chain quirks they handle
- `REFERENCES.md` links per mechanism per crate

## Expert Agents (slash commands)
- `/architect` — system architecture review (chain adapters, signing pipeline, data flow)
- `/developer` — code implementation and review (Rust, blockchain libs)
- `/security` — key management, signing security, threat model review
- `/ops` — node infrastructure, monitoring, DR, deployment
- `/blockchain` — chain-specific logic review (UTXO, nonce, Energy, ATA)
- `/pm` — task breakdown, progress tracking, prioritization
