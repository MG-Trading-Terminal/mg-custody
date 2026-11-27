# MG Custody

**Open-source cryptocurrency custody engine.**

Self-hosted alternative to Fireblocks/BitGo. Manages private keys, signs transactions, monitors deposits, processes withdrawals across 4 blockchains.

[![Tests](https://img.shields.io/badge/tests-1031-brightgreen)]() [![Rust](https://img.shields.io/badge/rust-2024-orange)]() [![Crates](https://img.shields.io/badge/crates-19-blue)]() [![License](https://img.shields.io/badge/license-MIT-blue)]()

> **WARNING: Development mode only.** The default configuration runs all components
> in a single process with software keys. This is suitable for development and testing.
> For production deployment with real funds, see [docs/DEPLOYMENT_MODES.md](docs/DEPLOYMENT_MODES.md).
> Read [THREAT_MODEL.md](THREAT_MODEL.md) and [SECURITY.md](SECURITY.md) before deploying.

## Why This Exists

The crypto industry is opaque. People don't understand how exchanges custody funds, how keys are managed, or how withdrawals actually work. This project makes everything transparent — every algorithm, every security decision, every trade-off is open and documented.

**This is not a toy.** Real BIP-143 Bitcoin signing. Real EIP-1559 Ethereum transactions. Real FROST threshold signatures audited by NCC Group. Real CGGMP21 MPC keygen audited by Kudelski Security. 451 tests proving it works.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Exchange (REST API)                       │
│                   POST /v1/addresses                             │
│                   POST /v1/withdrawals                           │
│                   GET  /v1/withdrawals/:id                       │
└─────────────┬───────────────────────────────────┬───────────────┘
              │                                   │
    ┌─────────▼─────────┐             ┌───────────▼──────────┐
    │  Deposit Scanner   │             │ Withdrawal Pipeline  │
    │  (per chain)       │             │ Received → Validated │
    │  ETH: alloy 1.x   │             │ → Signed → Broadcast │
    │  BTC: Core RPC     │             │ → Confirmed          │
    │  TRX: HTTP API     │             │                      │
    │  SOL: RPC          │             │ Policy Engine check  │
    └─────────┬──────────┘             │ OFAC sanctions check │
              │                        │ Fee estimation       │
    ┌─────────▼──────────┐             │ Nonce management     │
    │ Confirmation        │             └──────────┬───────────┘
    │ Tracker             │                        │
    │ ETH: 12 blocks      │             ┌──────────▼───────────┐
    │ BTC: 3 blocks       │             │    Signing Service    │
    │ TRX: 20 blocks      │             │  ┌─────────────────┐ │
    │ SOL: finalized      │             │  │ Single-key (dev) │ │
    └─────────┬──────────┘             │  │ MPC FROST (SOL)  │ │
              │                        │  │ MPC CGGMP (BTC/  │ │
    ┌─────────▼──────────┐             │  │      ETH/TRX)    │ │
    │   Webhook Sender    │             │  └─────────────────┘ │
    │ HMAC-SHA256 signed  │             └──────────────────────┘
    │ Exponential backoff │
    └─────────────────────┘
```

## Quick Start

```bash
# Clone
git clone https://github.com/example/mg-custody
cd mg-custody

# Start PostgreSQL
docker compose up -d postgres

# Run migrations
DATABASE_URL=postgres://custody:custody_dev_password@localhost:5432/custody \
  psql $DATABASE_URL -f crates/db/migrations/001_initial_schema.sql

# Build and run
cargo build --release
DATABASE_URL=postgres://custody:custody_dev_password@localhost:5432/custody \
  ./target/release/custody

# Test
curl http://localhost:8443/health
# → "ok"

curl -X POST http://localhost:8443/v1/addresses \
  -H "Content-Type: application/json" \
  -d '{"user_id":"550e8400-e29b-41d4-a716-446655440000","asset":"ETH","chain":"ethereum"}'
# → {"address":"0x95b1C733EE4ad41C726A25AFa57264dEB58c24CA","chain":"ethereum","memo":null}
```

## 21 Analytics (TRUST)

Set `CUSTODY_TRAVELRULE__PROVIDER=21travel` to enable automatic TRUST dispatch. Point `CUSTODY_21_TRAVEL__BASE_URL` to the HTTPS URL of a TRUST-enabled 21 Analytics deployment running version 8.4.0, and set `CUSTODY_21_TRAVEL__CLIENT_ID` and `CUSTODY_21_TRAVEL__CLIENT_SECRET`. On Enterprise, grant the service account `trustd.vasps.read`, `travel.configuration.read`, `travel.transactions.read`, and `travel.transactions.write`. [API setup](https://docs.21analytics.ch/8.4.0/api/getting-started.html) · [Service account permissions](https://docs.21analytics.ch/8.4.0/deployment/keycloak/authorisation.html)

For sends, use a dedicated TRUST instance. Set `directoryUrl` to `file:///` and `syncAddressDirectory` to `false` in the instance's `trpd.updateConfiguration` settings, preserving the other inputs. Restart the instance and leave these settings in effect. In 8.4.0 the server's HTTP client rejects file URLs, which disables directory routing. MG Custody checks both settings before sending. [Outgoing API workflow](https://docs.21analytics.ch/8.4.0/api/outgoing_integration.html)

Submit the identity data to `POST /v1/travel-rule` with the MG Custody withdrawal ID. The 21 Analytics Address Book entry must already exist for the asset. MG Custody supplies the destination, exact base-unit amount, asset and transaction ID directly from the withdrawal.

```json
{
  "withdrawal_id": "<MG Custody withdrawal ID>",
  "originator": { "name": "John Example" },
  "beneficiary": { "name": "Jane Example" },
  "trust": {
    "originator": {
      "id": "<21 Analytics Address Book entry ID>",
      "address": { "addressLine": "1 Example Street", "postalCode": "8001", "town": "Zurich", "country": "CH" },
      "customerIdentification": "example-customer-1"
    },
    "beneficiaryName": { "natural": { "firstName": "Jane", "lastName": "Example" } }
  }
}
```

The server holds withdrawals subject to the existing Travel Rule gate until TRUST data is queued and the counterparty and routing checks pass. After broadcast, the worker claims the notification and sends it. `GET /v1/travel-rule/{withdrawal_id}` reports `required`, `dispatching`, `submitted` with `trust_record_id`, or `failed`. Resubmit a `failed` record to retry a failure before sending. A send with an unconfirmed outcome stays `dispatching` and is checked against 21 Analytics without resending. If the status persists, verify delivery with the counterparty before intervening.

The built-in asset mapping covers BTC, ETH, TRX, SOL, and USDC/USDT on Ethereum and Tron. Use `custody-admin trust lookup ADDRESS [CHAIN ASSET]` to look up a counterparty and `custody-admin trust receive RECORD_ID` to retrieve the beneficiary data. For a standalone Bitcoin transaction, `custody-admin trust send ADDRESS` reads the 21 Analytics details with a string satoshi `amount` and on-chain `txId` from standard input.

## What's Inside (17 Crates)

| Crate | Purpose | Lines |
|-------|---------|-------|
| **custody-common** | Core types, config, chain adapters, metrics | ~8K |
| **custody-db** | PostgreSQL persistence (14 tables, real SQL) | ~1K |
| **custody-address-factory** | HD wallet derivation (BIP-32/44/84, SLIP-0010) | ~1.5K |
| **custody-signing** | Transaction signing (ETH, BTC, SOL, TRX) | ~2K |
| **custody-mpc** | Threshold signing (FROST + CGGMP21) | ~2K |
| **custody-scanner** | Block scanning + deposit detection | ~1.5K |
| **custody-fee-estimator** | Gas/fee estimation per chain | ~500 |
| **custody-broadcaster** | Transaction broadcast + fee bumping | ~500 |
| **custody-confirmations** | Confirmation tracking + webhooks | ~700 |
| **custody-withdrawal-pipeline** | State machine + nonce manager | ~1K |
| **custody-policy** | TAP-like rule engine + RBAC + whitelisting | ~2K |
| **custody-compliance** | AML/KYT + OFAC + Travel Rule | ~1.5K |
| **custody-wallet-manager** | Hot/warm/cold tiering + gas station | ~1K |
| **custody-utxo** | Bitcoin UTXO management + coin selection | ~500 |
| **custody-reconciliation** | Proof of Reserves (Merkle tree) | ~500 |
| **custody-gateway** | axum REST API + rate limiting | ~800 |
| **custody-server** | Binary + background tasks | ~500 |

## Signing — How It Works

### Ethereum (EIP-1559)
```
Seed → BIP-32 at m/44'/60'/0'/0/{i} → secp256k1 private key
→ Construct TxEip1559 (chain_id, nonce, gas, to, value)
→ Sign with k256 → RLP encode → 0x02 prefix (type 2)
→ eth_sendRawTransaction
```

### Bitcoin (P2WPKH)
```
Seed → BIP-32 at m/84'/0'/0'/0/{i} → secp256k1 private key
→ Build Transaction (inputs from UTXOs, outputs)
→ BIP-143 sighash (SegWit digest algorithm)
→ Sign each input → DER encode → witness = [sig, pubkey]
→ sendrawtransaction
```

### Solana (Ed25519)
```
Seed → SLIP-0010 at m/44'/501'/{i}'/0' → Ed25519 private key
→ Build message (header, accounts, blockhash, instructions)
→ Sign with ed25519-dalek → 64-byte signature
→ sendTransaction
```

### Tron (secp256k1 + SHA256)
```
Seed → BIP-32 at m/44'/195'/0'/0/{i} → secp256k1 private key
→ Build raw transaction (protobuf-like)
→ SHA256(raw_data) → sign with ECDSA → r(32) + s(32) + v(1)
→ broadcasttransaction
```

## MPC Threshold Signing

**Why**: A single seed = single point of failure. If compromised, all funds lost.

**Solution**: Split the key into shares. Any 2-of-3 shares can sign, but no single share reveals the key.

| Chain | Protocol | Library | Audit |
|-------|----------|---------|-------|
| BTC/ETH/TRX | CGGMP21 (ECDSA) | cggmp21 0.6 | Kudelski Security |
| SOL | FROST (EdDSA) | frost-ed25519 2.2 | NCC Group |

## Security Model

```
Zone 1 (DMZ):       API Gateway only
Zone 2 (Internal):  Scanners, pipeline, blockchain nodes
Zone 3 (Air-gap):   Signing service + HSM — no internet
```

- All key material: `Zeroize + ZeroizeOnDrop` (cleared from memory on drop)
- Key material NEVER logged (custom Debug impls print `***`)
- AES-256-GCM encrypted seed storage (PBKDF2 600K iterations)
- Shamir 3-of-5 backup (any 3 shares reconstruct — all 10 combos tested)
- HMAC-SHA256 on all API requests and webhooks
- OFAC SDN sanctions screening (known Tornado Cash addresses blocked)
- Policy engine: default DENY (must explicitly allow)

## API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check → "ok" |
| `/health/chains` | GET | Per-chain node status |
| `/metrics` | GET | Prometheus metrics |
| `/v1/addresses` | POST | Create deposit address |
| `/v1/withdrawals` | POST | Submit withdrawal |
| `/v1/withdrawals/:id` | GET | Check withdrawal status |
| `/v1/withdrawals/:id/approve` | POST | Approve/reject withdrawal |
| `/v1/wallets/:asset` | GET | Wallet balance |

Auth: `HMAC-SHA256(method + path + timestamp + body)` via `X-Custody-Signature` header.

## Tests

```bash
# Unit + crypto tests (no external deps)
cargo test

# With real PostgreSQL
DATABASE_URL=postgres://custody:custody_dev_password@localhost:5432/custody cargo test

# With real Sepolia testnet
cargo test -p custody-server --test sepolia_live
```

**1031 tests** covering: cryptographic signing (4 chains), MPC threshold signing, HD derivation, PostgreSQL CRUD, HTTP API E2E, webhook delivery, policy engine, UTXO selection, security attack simulations, DeFi calldata encoding, and live Sepolia blockchain interaction.

## Configuration

```toml
# config/default.toml
[api]
bind = "0.0.0.0:8443"

[chains.ethereum]
enabled = true
rpc_url = "http://localhost:8545"
required_confirmations = 12
max_gas_price_gwei = 200

[limits]
max_single_withdrawal_usd = 500000
max_hourly_outflow_usd = 10000000
max_daily_outflow_usd = 50000000
```

Environment overrides: `CUSTODY_DATABASE__URL`, `CUSTODY_CHAINS__ETHEREUM__RPC_URL`, etc.

## Roadmap

See [ROADMAP.md](ROADMAP.md) for full details. Current focus: Phase 17 (Trust Infrastructure) — threat model, signer isolation, circuit breaker persistence, supply chain security.

## Related Projects

- **mg-exchange** — Order matching engine, trading API
- Together they form a complete open-source crypto exchange stack

## License

MIT — use it, learn from it, build on it.
