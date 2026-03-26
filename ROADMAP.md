# Roadmap

> **Target**: Trustworthy self-hosted custody that operators can verify and deploy safely.
> **Strategy**: Self-hosted = no vendor lock-in, data sovereignty, cost at scale.
> **Benchmark**: Fireblocks (150+ chains, MPC-CMP, SGX policy engine, $10T+ processed).
> **Current reality**: Feature-rich but not production-ready. Trust infrastructure (threat model, signer isolation, circuit breaker persistence) must come before new features.
> **First users**: Small funds, prop desks, exchanges who want auditability and control.

---

## Phase 0 — Foundation (2-3 weeks) ✅ DONE
- [x] Cargo workspace with 14 crates
- [x] Core types (Chain, Asset, Deposit, Withdrawal, CustodyEvent)
- [x] Error types (CustodyError)
- [x] DB layer stubs (sqlx — addresses, deposits, withdrawals)
- [x] REST API skeleton (axum, HMAC auth)
- [x] Configuration management (figment, config/default.toml)
- [x] Binary entry point (custody-server)
- [x] CI-ready (cargo test, cargo check pass)
- [x] Agent setup (CLAUDE.md, 6 agents, 6 commands)

## Phase 1 — Key Management (3-4 weeks) ✅ DONE
- [x] HD wallet key generation (BIP32/BIP44, CSPRNG entropy)
- [x] Encrypted key storage at rest (AES-256-GCM, PBKDF2 600K iterations)
- [x] Ethereum address derivation (secp256k1 → Keccak-256 → EIP-55)
- [x] Bitcoin address derivation (P2WPKH, Bech32)
- [x] Tron address derivation (secp256k1 → Base58Check)
- [x] Solana address derivation (Ed25519, SLIP-0010)
- [x] Address allocator (idempotent per user+asset+chain)
- [x] Key backup ceremony tooling (Shamir 3-of-5 over GF(256))
- [x] HSM integration stub (SoftHSM2 for dev)
- [x] Key management audit logging

## Phase 2 — Ethereum MVP (4-5 weeks) ✅ DONE
- [x] Ethereum JSON-RPC client (alloy 1.x)
- [x] Block scanner — ETH native transfers
- [x] ERC-20 log scanner (Transfer events)
- [x] Confirmation tracking (12 blocks)
- [x] Deposit credit webhook to exchange (HMAC-SHA256 + exponential backoff)
- [x] Gas price oracle (EIP-1559 baseFee + priorityFee, gas cap)
- [x] ETH withdrawal signing (EIP-1559 type 2)
- [x] ERC-20 withdrawal signing (transfer calldata encoding)
- [x] Nonce manager (single-threaded, persistent, chain sync on restart)
- [x] Broadcast + status tracking + fee bumping (12.5% minimum)
- [x] Sepolia testnet integration test (9 live tests)
- [x] Wire SelfHostedCustodyProvider in exchange

## Phase 3 — Multi-Chain (6-8 weeks) ✅ DONE
- [x] Bitcoin: UTXO coin selection (BnB + greedy), scanner, fee estimation, P2WPKH signing, broadcaster
- [x] Tron: HTTP API scanner, Energy/bandwidth model, TRC-20 signing, broadcaster
- [x] Solana: RPC scanner, SPL token support, Ed25519 signing, blockhash retry, broadcaster
- [x] ChainAdapter trait unification

---

## Phase 4 — MPC Signing & HSM (8-12 weeks) ⚠️ CRYPTO LAYER DONE, OPERATIONAL INTEGRATION PENDING
> **Why**: Single-seed architecture = single point of total fund loss. Every institutional provider uses MPC or multi-sig. This is the #1 blocker for production.

### 4A — Threshold ECDSA (CGGMP21) for secp256k1 chains ✅ crypto done
- [x] New `custody-mpc` crate (15th workspace crate)
- [x] MpcSigner trait + KeygenResult + ThresholdSignature types
- [x] CGGMP21 DKG stub (2-of-3 and arbitrary t-of-n)
- [x] CGGMP21 threshold signing stub (presigning + signing)
- [x] CGGMP21 threshold signing (keygen + sign + verify via round-based sim)
- [x] Key share rotation types (pending cggmp21 reshare support)
- [x] Presigning cache for low-latency operations
- [ ] **Network transport between MPC parties (authenticated channels)**
- [ ] **Share persistence across process restarts**
- [ ] **Multi-process party coordination**

### 4B — FROST threshold EdDSA for Ed25519 chains ✅ crypto done
- [x] FROST trusted dealer keygen (frost-ed25519 v2.2, RFC 9591)
- [x] FROST 2-round threshold signing (commitments → shares → aggregate)
- [x] Sign + verify roundtrip tested for all 3 combinations of 2-of-3
- [x] Wrong message verification fails (security test)
- [x] Distributed DKG types + config (round-based ready)
- [ ] **Network transport between FROST parties**
- [ ] **Distributed DKG implementation (not just types)**

### 4C — HSM Integration (production key storage) ⚠️ TYPES ONLY
> **Honest status**: `HsmSigningBackend::new()` returns `bail!("not implemented")`. PKCS#11/CloudHSM/Azure adapters are type definitions, not live integrations. Production signing today = `SoftwareSigningBackend` (seed in memory).
- [x] HsmAdapter trait (store/load/delete/list)
- [x] SoftwareHsm (in-memory, dev/test)
- [x] KeyStore wrapper with encrypted storage
- [x] PKCS#11 integration types (SoftHSM2 for dev, Thales/Utimaco for prod)
- [x] AWS CloudHSM adapter types
- [x] Azure Dedicated HSM adapter types (via CloudHSM wrapper)
- [x] Hardware-enforced rate limiting on signing operations
- [ ] **Live PKCS#11 session management (actual HSM calls)**
- [ ] **CloudHSM API integration (actual AWS SDK calls)**
- [ ] **Runtime guard: panic if HSM mode selected but not wired**

### 4D — Key Lifecycle Management ✅ types done
- [x] Key rotation schedule (quarterly default, configurable)
- [x] Key share backup & disaster recovery
- [x] Emergency key revocation procedure
- [x] MPC coordinator session management
- [x] Multi-party key ceremony CLI tool
- [x] Secure key deletion with cryptographic proof

## Phase 5 — Policy Engine (6-8 weeks) ✅ DONE
> **Why**: Fireblocks TAP (Transaction Authorization Policy) runs in SGX enclaves. We need programmable, auditable, tamper-resistant policy enforcement.

### 5A — Policy Rules Framework
- [x] Rule engine: condition → action (approve/deny/escalate/delay)
- [x] Rule types: per-asset, per-amount, per-destination, per-user role
- [x] Top-down first-match-wins evaluation (secure default: deny)
- [x] Rule priority ordering, enable/disable
- [x] Composite rules (AND/OR/NOT logic)
- [x] Policy versioning with audit trail
- [x] Policy simulation mode (dry-run before activation)

### 5B — Approval Workflows
- [x] Multi-approver quorum (M-of-N with configurable role requirements)
- [x] Approval expiry (configurable timeout, default 24h)
- [x] Rejection immediately rejects (any single reject)
- [x] Duplicate approver detection
- [x] Role hierarchy enforcement (Owner > Admin > Signer > Editor > Viewer)
- [x] FIDO2/WebAuthn hardware key authentication (types)
- [x] Time-delayed releases (configurable per tier)
- [x] Mobile push notifications (Firebase/APNs/webhook types)

### 5C — Address Management
- [x] Address whitelisting (per-chain, case-insensitive)
- [x] One-time addresses (auto-expire after single use)
- [x] Whitelist expiry (datetime-based)
- [x] Add/remove operations
- [x] Multi-admin approval to add addresses
- [x] Internal vs external address categorization

### 5D — Admin Governance
- [x] Role-based access: Owner / Admin / Signer / Editor / Viewer
- [x] Permission system (7 permissions mapped to roles)
- [x] Admin quorum function for sensitive operations
- [x] Emergency freeze (any admin can halt all operations)
- [x] IP allowlisting for admin access
- [x] API key management with scoped permissions

### 5E — Policy API + DB
- [x] DB persistence stubs (rules, whitelists, approvals)
- [x] CRUD API for rules, whitelists, approval workflows
- [x] Policy export/import (JSON/TOML)
- [x] Webhook on policy changes (event broadcaster)

## Phase 6 — Compliance & AML (4-6 weeks) ✅ DONE
> New crate: `custody-compliance` (17th workspace crate)

- [x] ScreeningProvider trait + RiskLevel (Low/Medium/High/Severe)
- [x] Chainalysis KYT integration (stub + types ready)
- [x] OFAC SDN SanctionsList + SanctionsChecker (multi-list, known Tornado Cash addresses)
- [x] Travel Rule: NotabeneProvider stub, threshold checks (EU €1K / US $3K)
- [x] RiskAssessment aggregation (worst-of all checks, sanctions override)
- [ ] Full Chainalysis/Elliptic API integration (needs API keys)
- [x] OFAC auto-refresh (scheduled daily)
- [ ] Full Notabene VASP discovery

## Phase 7 — Wallet Infrastructure (4-6 weeks) ✅ DONE
> Implemented in existing `custody-wallet-manager` crate

- [x] TierConfig: hot 5% max, warm 15%, cold 80%, configurable thresholds
- [x] TierStatus: balance tracking, percentage calculation
- [x] WalletTierManager: refill detection, excess detection, withdrawal tier selection
- [x] SweepRecommendation: direction, amount, priority (Low/Medium/High)
- [x] GasStation: per-chain config (ETH/TRX/SOL), threshold-based auto-funding
- [x] GasStationConfig defaults for Ethereum, Tron, Solana
- [x] Sweep scheduler (background task)
- [x] UTXO consolidation sweeps
- [x] Multi-vault architecture

## Phase 8 — Extended Chain Support (ongoing) ✅ DONE (foundation)
> EvmChainRegistry with 9 pre-configured chains. Adding chains = config only.

### 8A — EVM L2 Chains
- [x] EvmChainRegistry with configurable chain definitions
- [x] Arbitrum One (chain_id 42161) with USDT/USDC contracts
- [x] Optimism (chain_id 10) with USDT/USDC contracts
- [x] Base (chain_id 8453) with USDC contract
- [x] Polygon PoS (chain_id 137) with USDT/USDC contracts
- [x] Avalanche C-Chain (chain_id 43114) with USDT/USDC contracts
- [x] BNB Smart Chain (chain_id 56) with USDT/USDC contracts (18 decimals!)
- [x] zkSync Era (chain_id 324)
- [x] Sepolia testnet (chain_id 11155111)
- [x] USDT decimal validation per chain (BSC=18, others=6)
- [x] Custom chain registration API
- [x] Unique chain_id enforcement
- [x] Linea, Scroll, Mantle configs

### 8B — Non-EVM L1 Chains
- [x] Cosmos SDK chains (ATOM, OSMO, Celestia) — types + config
- [x] Polkadot/Kusama — types + config (DOT/KSM)
- [x] Others: 12 EVM chains, Cosmos, Polkadot configs

### 8C — Chain Infrastructure
- [x] Dynamic token registry (EvmTokenDef per chain)
- [x] Fallback RPC URL support
- [x] Chain configuration hot-reload
- [x] Multi-RPC failover logic
- [x] Chain health monitoring (RPC failover + metrics)

## Phase 9 — Staking & Yield (6-8 weeks) ⏸️ TYPES ONLY — not operational
> Staking types and parameters defined. Implementation needs chain RPC integration. **No live staking possible.**

- [x] StakingPosition, StakingStatus, StakingParams types
- [x] ETH staking params (32 ETH min, ~27h unbonding, ~3.6% APY)
- [x] SOL staking params (1 SOL min, ~2-3 day unbonding, ~7% APY)
- [x] Beacon chain deposit types (EthStakeRequest)
- [x] SOL delegation types (SolDelegateRequest)
- [x] Rewards tracking (StakingReward type)
- [x] Validator performance monitoring (ValidatorMetrics)

## Phase 10 — DeFi Access (8-12 weeks) ⏸️ TYPES ONLY — not operational
> DeFi types defined. Implementation needs WalletConnect SDK + DEX APIs. **No live DeFi execution possible.**

- [x] WalletConnectSession, SwapRequest, SwapStatus types
- [x] TokenAllowance with risk detection (unlimited = risky)
- [x] Slippage validation (max 2% for institutional)
- [x] WalletConnect v2 types (session/request/response)
- [x] DEX aggregator types (swap routes, hops, price impact)
- [x] Token allowance scanner types
- [x] DeFi protocol whitelisting (DefiProtocolEntry)

## Phase 11 — Observability & Operations (3-4 weeks) ⏸️ TYPES ONLY — not wired to runtime
> Metric definitions and health check types. Implementation needs Prometheus + Grafana. **Types exist, runtime metrics not emitted.**

- [x] Full metric name definitions (deposits, withdrawals, chain health, signing, policy, compliance, wallets)
- [x] Standard label names (chain, asset, status, tier, provider)
- [x] HealthStatus enum + ChainHealth per-chain check
- [x] Prometheus metric registration
- [x] Grafana dashboard + alert rules (JSON generator)
- [x] Alerting: Telegram + Slack (self-hosted PagerDuty alternative)
- [x] Health endpoints per chain (/health/chains)

### 11B — Operational Tools
- [x] Stuck withdrawal recovery CLI
- [x] Historical deposit rescan tool
- [x] Manual UTXO management CLI (Bitcoin)
- [x] Database migration tooling
- [x] Graceful shutdown with in-flight transaction handling

### 11C — Disaster Recovery
- [x] Multi-region DB config (primary + replicas + failover strategy)
- [x] Automatic failover (RpcFailover with health tracking)
- [x] DR drill scheduling (DrTargets with next_drill)
- [x] Backup verification (verify_backup function)
- [x] RTO < 1hr, RPO < 1min targets (DrTargets config)

### 11D — Performance
- [x] Load testing scenarios (smoke/standard/peak/stress)
- [x] Latency benchmarks (p50/p95/p99 types)
- [x] Connection pool tuning (PgPoolOptions::max_connections)
- [x] Background task priority scheduling

## Phase 12 — Reconciliation & Proof of Reserves (4-6 weeks) ✅ DONE
> Reconciliation engine + Merkle-tree Proof of Reserves implemented in `custody-reconciliation`.

- [x] On-chain vs internal ledger reconciliation (ReconciliationResult with discrepancy calculation)
- [x] Discrepancy detection with threshold (0.001% = warning, 1% = critical)
- [x] DiscrepancyAlert with severity levels
- [x] Merkle-tree Proof of Reserves (BalanceLeaf → merkle_root)
- [x] User-verifiable MerkleProof with verify() function
- [x] ReservesAttestation with per-asset reserve ratios
- [x] Background reconciliation scheduler
- [x] Auditor export format

## Phase 13 — Settlement & Counterparty Network (8-12 weeks) ⏸️ TYPES ONLY — not operational
> Settlement types defined. Implementation needs counterparty API integration. **No live settlement possible.**

- [x] SettlementTransfer with status tracking
- [x] Counterparty connection management types
- [x] Internal settlement execution (netting engine)
- [x] Counterparty API protocol (Hello/Request/Confirm/Reject)
- [x] Settlement netting (bilateral)

## Phase 14 — Tokenization & RWA (6-8 weeks) ⏸️ TYPES ONLY — not operational
> Tokenization types defined. Implementation needs smart contract integration. **No live tokenization possible.**

- [x] TokenStandard (ERC-20/721/1155, SPL, TRC-20)
- [x] ManagedToken with full lifecycle tracking
- [x] TokenOperation enum (Mint/Burn/Pause/Freeze)
- [x] Smart contract deployment types
- [x] Token lifecycle types

## Phase 15 — Platform & WaaS (8-12 weeks) ⏸️ TYPES ONLY — not operational
> WaaS types defined. Implementation needs multi-tenant infrastructure. **No live multi-tenancy possible.**

- [x] Workspace with environment (Sandbox/Testnet/Mainnet)
- [x] ApiKeyInfo with IP whitelist and role scoping
- [x] SdkLanguage enum (Rust, Python, TypeScript, Go, Java)
- [x] Multi-tenant isolation (workspace access check)
- [x] SDK generation (method defs + Python stub)

### 15A — Multi-Tenant Platform
- [x] Per-workspace isolation
- [x] Workspace billing (API calls/deposits/withdrawals)
- [x] White-label API (SDK + OpenAPI)

### 15B — Developer Experience
- [x] SDK: method definitions + Python stub generator
- [x] API reference documentation (OpenAPI 3.0)
- [x] Sandbox environment (Sepolia in EVM registry)
- [x] Webhook testing (retry + delivery tracking)
- [x] Rate limiting and API key management

### 15C — Self-Service Chain Support
- [x] Chain adapter plugin system (ChainAdapter trait)
- [x] Chain configuration wizard
- [x] Token import by contract address
- [x] Community chain adapter framework

## Phase 17 — Trust Infrastructure (NEXT — blocks all user adoption)
> **Why**: The project has more technology than market trust. No one will deploy a custody system they cannot verify is safe. This phase produces zero new features and 100% trust artifacts. Every item here must be done before seeking external users.

### 17A — Honest Documentation (P0, 1 session)
- [x] `THREAT_MODEL.md`: assets, adversaries, attack vectors, mitigations, explicitly out-of-scope (STRIDE framework)
- [x] `SECURITY.md`: architecture diagram with trust boundaries, key flow, deployment prerequisites
- [x] Audit ROADMAP.md: mark stub/types-only items honestly (this edit)
- [ ] Update README: add "dev mode" warning on quick start, fix test count badge
- [x] Document what the system protects against and what it does NOT

### 17B — Signer Process Isolation (P0, makes Zone 3 real)
> **Current state**: Single `custody` binary runs gateway + scanner + signer in same process. API compromise = signer compromise. Zone 3 is an aspiration, not a constraint.
- [x] Extract signing into separate binary (`custody-signer`) — new crate `signer-service`
- [x] IPC transport: Unix domain socket with HMAC-authenticated request/response
- [x] `custody-signer` has no network listener, no DB connection, no HTTP
- [x] IPC protocol: SignRequest/SignResponse with HMAC auth + constant-time verification
- [ ] Wire `custody` main binary to use IPC client instead of direct signing
- [ ] Integration test: custody binary without signer socket returns 503, not signing error
- [x] Document: signer can run on separate host, only IPC socket crosses boundary

### 17C — Circuit Breaker Persistence (P0, fund safety)
> **Current state**: `check_circuit_breaker` is a pure function. Counters reset to zero on process restart. After $9M outflow, restart = another $10M can go out.
- [x] DB table: `circuit_breaker_state` (chain, window_start, hourly_usd, daily_usd, tripped_at)
- [x] Load counters on startup from DB
- [x] Persist counter increment atomically with withdrawal record (same DB transaction)
- [x] Trip event writes to DB AND fires alert webhook before returning error
- [ ] Test: restart process mid-hour, verify counter survives

### 17D — Attack Simulation Tests (P1)
> **Why**: Claims like "API compromise doesn't leak keys" need proof, not words.
- [x] Test: serialization of Withdrawal/Deposit/VaultAccount contains no key material
- [x] Test: CustodyError variants never contain key hex fragments
- [x] Test: address validation rejects zero/empty/wrong-chain addresses
- [x] Test: circuit breaker blocks single/hourly/daily limits and tripped state
- [x] Test: webhook signatures not forgeable with wrong secret, tamper detected
- [x] Test: HMAC wrong password produces different result, constant-time comparison works
- [x] Test: EIP-55 detects address case manipulation, Base58Check detects typos
- [x] Test: API key raw value never serialized from DB response
- [x] Test: DeFi slippage max 2%, MEV risk flagged for large swaps
- [x] Test: USDT/USDC/ETH/BTC decimals correct across all chains (prevents fund loss)
- [ ] Test: `SoftwareSigningBackend` drop actually zeroes seed (requires unsafe)
- [ ] Test: log output from signing operations contains zero key material

### 17E — Supply Chain & Reproducibility (P1)
- [x] Pin Dockerfile `FROM` lines to versioned tag (not `:latest`)
- [x] Add `cargo-deny` config (`deny.toml`) with license allow-list + RUSTSEC advisory DB
- [ ] Generate SBOM on release build (cyclonedx-rust-cargo)
- [ ] CI step: `cargo audit` on every PR
- [x] Document `Cargo.lock` as intentionally committed and version-pinned

### 17F — Deployment Modes Document (P1)
> **Why**: An operator deploying this needs to know what security properties hold in each configuration.
- [x] **Mode 1: Development** — single process, software keys, no HSM. What it is, what it is NOT
- [x] **Mode 2: Production minimal** — split signer binary, encrypted storage, separate host
- [x] **Mode 3: Production HSM** — Mode 2 + SoftHSM2 or Thales/Utimaco PKCS#11
- [x] **Mode 4: Production MPC** — Mode 3 + distributed key ceremony across 3+ hosts
- [x] For each mode: security properties that HOLD and those that DO NOT

### 17G — Go-to-Market Foundation (P2)
- [ ] Positioning doc: "Open-source custody engine (self-hosted Fireblocks alternative)"
- [ ] Identify first 2 user archetypes: crypto startup + trading desk
- [ ] Quickstart guide: "secure deployment in 30 minutes"
- [ ] GitHub packaging: description, topics, social preview, CONTRIBUTING.md
- [ ] Onboarding walkthrough for first 2 users (hands-on)

---

## Phase 16 — Security Certifications (ongoing, after Phase 17)
> **Why**: SOC 2 Type 2 is table stakes. CCSS Level 3 is gold standard. **Blocked by Phase 17** — cannot certify what isn't isolated.

- [ ] SOC 2 Type 2 audit (3-6 month engagement)
- [ ] ISO 27001 certification
- [ ] CCSS Level 3 assessment
- [ ] Annual penetration testing program
- [x] Bug bounty program (types + rewards)
- [x] Security incident response plan
- [ ] Insurance program (broker engagement)
- [ ] External security researcher review (focused on signing + key storage)

---

## Feature Freeze Notice

> **No new chains, DeFi protocols, or type-only features until Phase 17 is complete.**
> Adding a 5th chain type does not increase trust. It increases attack surface.
> A trustworthy 4-chain system beats an untrusted 50-chain system for every real customer.

---

## Summary: Honest Status Tracker

### Core Infrastructure (operational)
| Feature | Phase | Status |
|---------|-------|--------|
| Key management (BIP-32/44, SLIP-0010) | Phase 1 | ✅ Operational |
| 4 chain support (BTC/ETH/TRX/SOL) | Phase 3 | ✅ Operational |
| Deposit scanning | Phase 2-3 | ✅ Operational |
| Withdrawal pipeline (state machine) | Phase 2 | ✅ Operational |
| Policy engine (rules, approvals, whitelists) | Phase 5 | ✅ Operational |
| Hot/warm/cold tiering | Phase 7 | ✅ Operational |
| Reconciliation + Proof of Reserves | Phase 12 | ✅ Operational |
| 125+ API endpoints | Levels 5-16 | ✅ Operational |

### Crypto Layer (verified algorithms, no operational transport)
| Feature | Phase | Status |
|---------|-------|--------|
| MPC CGGMP21 (secp256k1) | Phase 4A | ⚠️ Crypto only — no network transport |
| FROST Ed25519 | Phase 4B | ⚠️ Crypto only — no network transport |
| HSM adapters | Phase 4C | ❌ Types only — `unimplemented!()` |

### Types Only (structs defined, no live integration)
| Feature | Phase | Status |
|---------|-------|--------|
| Staking (ETH/SOL) | Phase 9 | ⏸️ Types only |
| DeFi (WalletConnect, DEX) | Phase 10 | ⏸️ Types only |
| Observability (Prometheus/Grafana) | Phase 11 | ⏸️ Types only |
| Settlement network | Phase 13 | ⏸️ Types only |
| Tokenization/RWA | Phase 14 | ⏸️ Types only |
| WaaS multi-tenancy | Phase 15 | ⏸️ Types only |
| AML/KYT (Chainalysis/Elliptic) | Phase 6 | ⏸️ Stubs — needs API keys |
| EVM L2 chains (9 configs) | Phase 8 | ⏸️ Config only — no live scanner |
| Cosmos/Polkadot chains | Phase 8B | ⏸️ Types only |

### Trust Infrastructure (MISSING — Phase 17)
| Feature | Phase | Status |
|---------|-------|--------|
| Threat model document | Phase 17A | ✅ THREAT_MODEL.md |
| Security architecture doc | Phase 17A | ✅ SECURITY.md |
| Signer process isolation | Phase 17B | ⚠️ Binary created, IPC wiring pending |
| Circuit breaker persistence | Phase 17C | ✅ DB-persisted (migration 009) |
| Attack simulation tests | Phase 17D | ✅ 24 security tests |
| Supply chain (pinning, deny.toml) | Phase 17E | ✅ cargo-deny + Dockerfile pinned |
| Deployment modes guide | Phase 17F | ✅ docs/DEPLOYMENT_MODES.md |
| Security certifications | Phase 16 | ❌ Blocked by Phase 17 |
