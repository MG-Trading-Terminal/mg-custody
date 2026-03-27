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
- [x] **Network transport between MPC parties (authenticated channels)** — `MpcTransport` trait + InMemoryTransport + HMAC auth
- [x] **Share persistence across process restarts** — `ShareStore` trait + FileShareStore + integrity verification
- [x] **Multi-process party coordination** — `SessionCoordinator` state machine with round management

### 4B — FROST threshold EdDSA for Ed25519 chains ✅ crypto done
- [x] FROST trusted dealer keygen (frost-ed25519 v2.2, RFC 9591)
- [x] FROST 2-round threshold signing (commitments → shares → aggregate)
- [x] Sign + verify roundtrip tested for all 3 combinations of 2-of-3
- [x] Wrong message verification fails (security test)
- [x] Distributed DKG types + config (round-based ready)
- [x] **Network transport between FROST parties** — shared `MpcTransport` trait
- [x] **Distributed DKG implementation** — `run_simulated_dkg()` with transport, round management, group key derivation

### 4C — HSM Integration (production key storage) ✅ FILE-BACKED OPERATIONAL
> **Status**: `HsmSigningBackend` stores seed encrypted (AES-256-GCM + PBKDF2 600K), loads on demand per signing operation, zeroizes immediately. Seed NOT held in memory between operations. File-backed storage operational; real PKCS#11 (SoftHSM2/Thales) requires `cryptoki` integration.
- [x] HsmAdapter trait (store/load/delete/list)
- [x] SoftwareHsm (in-memory, dev/test)
- [x] KeyStore wrapper with encrypted storage
- [x] PKCS#11 integration types (SoftHSM2 for dev, Thales/Utimaco for prod)
- [x] AWS CloudHSM adapter types
- [x] Azure Dedicated HSM adapter types (via CloudHSM wrapper)
- [x] Hardware-enforced rate limiting on signing operations
- [x] **HsmSigningBackend: encrypted seed, on-demand decrypt→sign→zeroize cycle**
- [ ] **Live PKCS#11 session management (cryptoki crate, actual C_Sign calls)**
- [ ] **CloudHSM API integration (actual AWS SDK calls)**
- [x] **Runtime guard: panic if HSM mode selected but not wired** — `assert_hsm_operational()` + health check

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
- [x] Update README: add "dev mode" warning on quick start, fix test count badge
- [x] Document what the system protects against and what it does NOT

### 17B — Signer Process Isolation (P0, makes Zone 3 real)
> **Current state**: Single `custody` binary runs gateway + scanner + signer in same process. API compromise = signer compromise. Zone 3 is an aspiration, not a constraint.
- [x] Extract signing into separate binary (`custody-signer`) — new crate `signer-service`
- [x] IPC transport: Unix domain socket with HMAC-authenticated request/response
- [x] `custody-signer` has no network listener, no DB connection, no HTTP
- [x] IPC protocol: SignRequest/SignResponse with HMAC auth + constant-time verification
- [x] Wire `custody` main binary to use IPC client instead of direct signing — `SigningMode` in pipeline
- [x] Integration test: IPC mode without signer socket fails gracefully (connect error, not panic)
- [x] Document: signer can run on separate host, only IPC socket crosses boundary

### 17C — Circuit Breaker Persistence (P0, fund safety)
> **Current state**: `check_circuit_breaker` is a pure function. Counters reset to zero on process restart. After $9M outflow, restart = another $10M can go out.
- [x] DB table: `circuit_breaker_state` (chain, window_start, hourly_usd, daily_usd, tripped_at)
- [x] Load counters on startup from DB
- [x] Persist counter increment atomically with withdrawal record (same DB transaction)
- [x] Trip event writes to DB AND fires alert webhook before returning error
- [x] Test: restart process mid-hour, verify counter survives

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
- [x] Test: `SoftwareSigningBackend` zeroize clears seed (explicit test)
- [x] Test: Debug output from signing backends contains no key material

### 17E — Supply Chain & Reproducibility (P1)
- [x] Pin Dockerfile `FROM` lines to versioned tag (not `:latest`)
- [x] Add `cargo-deny` config (`deny.toml`) with license allow-list + RUSTSEC advisory DB
- [x] Generate SBOM on release build (CycloneDX in CI)
- [x] CI step: `cargo audit` on every PR
- [x] Document `Cargo.lock` as intentionally committed and version-pinned

### 17F — Deployment Modes Document (P1)
> **Why**: An operator deploying this needs to know what security properties hold in each configuration.
- [x] **Mode 1: Development** — single process, software keys, no HSM. What it is, what it is NOT
- [x] **Mode 2: Production minimal** — split signer binary, encrypted storage, separate host
- [x] **Mode 3: Production HSM** — Mode 2 + SoftHSM2 or Thales/Utimaco PKCS#11
- [x] **Mode 4: Production MPC** — Mode 3 + distributed key ceremony across 3+ hosts
- [x] For each mode: security properties that HOLD and those that DO NOT

### 17G — Go-to-Market Foundation (P2)
- [x] Positioning doc: "Open-source custody engine (self-hosted Fireblocks alternative)" (docs/POSITIONING.md)
- [x] Identify first 2 user archetypes: crypto startup + trading desk
- [x] Quickstart guide: "secure deployment in 30 minutes" (docs/QUICKSTART.md)
- [x] GitHub packaging: description, topics, social preview, CONTRIBUTING.md
- [x] Onboarding walkthrough for first 2 users (hands-on)

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

## Phase 18 — Closing Competitive Gaps (P0/P1)
> **Why**: Phase 17 established trust infrastructure. Now close remaining gaps vs Fireblocks/BitGo/Copper identified in gap analysis.

### 18A — HSM Signing Backend ✅ DONE
- [x] `HsmSigningBackend`: encrypted seed storage, on-demand decrypt→sign→zeroize
- [x] Seed NOT held in memory between operations (security improvement over SoftwareSigningBackend)
- [x] Signature parity test: HSM and Software backends produce identical signatures
- [x] Operation counter for audit trail
- [ ] Real PKCS#11 via `cryptoki` crate (SoftHSM2/Thales/Utimaco)

### 18B — Generalized EVM Scanner ✅ DONE
- [x] `EvmScanner` with native + ERC-20 Transfer event scanning
- [x] `from_chain_def()` factory — create scanner from EvmChainRegistry entry
- [x] `create_scanners_from_registry()` — bulk create for all enabled chains
- [x] Configurable native_token, native_decimals, chain_id per chain
- [x] Server wired: Arbitrum, Optimism, Base, Polygon L2 scanners

### 18C — Multi-Chain Fee Estimation ✅ ALREADY DONE
- [x] Bitcoin: `estimatesmartfee` RPC, BTC/kB → sat/vB conversion
- [x] Tron: Energy/bandwidth model, chain parameter query
- [x] Solana: `getRecentPrioritizationFees` with median calculation

### 18D — MPC HTTP Network Transport ✅ DONE
- [x] `HttpTransport` implementing `MpcTransport` trait
- [x] HTTP POST between party endpoints with HMAC-SHA256 on every message
- [x] `deliver()` with authentication + size validation
- [x] `deliver_to_inbox()` helper for HTTP handler integration
- [x] Polling `receive()` with configurable timeout
- [x] 8 tests: creation, delivery, HMAC reject, tamper detect, multi-party, filtering

---

## ~~Feature Freeze Notice~~ LIFTED (2026-03-27)

> Phase 17 complete. Feature freeze lifted. Competitive gap closure in progress.

---

## Phase 19 — RBAC Enforcement & Session Management (P0, 3-4 weeks)
> **Why**: Without authorization middleware, ANY authenticated API caller has god-mode access. Types exist (`crates/policy/src/rbac.rs`, `crates/common/src/governance.rs`) but nothing enforces them. Cannot ship a dashboard or onboard users without this.
> **Tech**: axum `tower::Layer` middleware, sessions in PostgreSQL (no Redis — "all state in PostgreSQL" per CLAUDE.md). `webauthn-rs` for FIDO2 comes in Phase 20.

### 19A — Session Infrastructure ✅ DONE
- [x] DB migration 011: `sessions` table + `password_hash` column on workspace_users
- [x] Session creation endpoint: `POST /v1/auth/sessions` → returns opaque Bearer token
- [x] Session revocation: `DELETE /v1/auth/sessions/{token}`
- [x] Session listing: `GET /v1/auth/sessions`
- [x] Session DB layer: create/load/touch/revoke/cleanup, token hashing (SHA-256)
- [ ] Background task: expire stale sessions (hourly cleanup)

### 19B — RBAC Middleware ✅ DONE
- [x] `rbac_auth` middleware in `crates/gateway/src/rbac_middleware.rs` — Bearer token → session → AuthContext
- [x] `RequireViewer/Editor/Signer/Admin/Owner` axum extractors — returns 403 if denied
- [x] Default-deny: no AuthContext = rejected
- [x] Permission guards wired on core endpoints (create_address, submit/get/approve withdrawal, wallet balance)
- [x] Session binding: IP match validation
- [x] Absolute 8h + idle 30m timeout validation
- [x] HMAC-authed service requests get Admin role (backward compatible)
- [x] 14 new tests: sessions, RBAC extractors, password verification
- [x] Apply guards to ALL 125+ endpoints across 11 route files (vault, ops, connectivity, platform, governance, operations, embedded, exchange, admin, settlement, staking)
- [ ] Permission denials logged to audit trail

---

## Phase 20 — FIDO2/WebAuthn Verification (P0, 2-3 weeks)
> **Why**: Tier 2+ withdrawal approvals ($10K+) require hardware key verification. The types already exist in `crates/common/src/fido2.rs` but nothing actually verifies cryptographic assertions. Hand-rolling WebAuthn verification = security disaster.
> **Tech**: `webauthn-rs = "0.5"` — maintained by ISRG/Let's Encrypt contributors. The only production-grade Rust WebAuthn library. Handles CBOR attestation parsing, origin checking, and credential storage patterns.
> **Depends on**: Phase 19 (user/session infrastructure)

- [x] DB migration 012: `fido2_credentials` + `fido2_challenges` tables
- [x] FIDO2 DB layer (`crates/db/src/fido2.rs`): credential CRUD, challenge create/consume, sign count update, cleanup
- [x] Registration flow: `POST /v1/auth/fido2/register/begin` → `POST /v1/auth/fido2/register/complete`
- [x] Authentication flow: `POST /v1/auth/fido2/authenticate/begin` → `POST /v1/auth/fido2/authenticate/complete`
- [x] Credential management: `GET /v1/auth/fido2/credentials`, `DELETE /v1/auth/fido2/credentials/{id}`
- [x] Configurable `rp_id` via `CUSTODY_FIDO2_RP_ID` env var (default: "custody.local")
- [x] Security: server-side challenges (SHA-256 hashed, single-use, 5min expiry), sign count monotonic (cloned key detection)
- [x] 7 new tests (challenge validity, consumed rejection, serialization)
- [ ] Wire FIDO2 verification into Tier 2+ withdrawal approval flow
- [ ] Full WebAuthn assertion verification (CBOR attestation parsing — needs `webauthn-rs` stable release or manual COSE key verification)

---

## Phase 21 — Web Dashboard (P0, 6-8 weeks)
> **Why**: Zero operators will adopt a custody system with no UI. This is the **#1 adoption blocker**. Operators need a console for approvals, policy management, wallet monitoring.
> **Depends on**: Phase 19 (RBAC/sessions), Phase 20 (FIDO2 for approval flows)

### Technology Decision: SvelteKit + `adapter-static`

| Alternative | Why rejected |
|-------------|-------------|
| React/Next.js | Next.js SSR requires Node.js process in production — violates "single binary" principle. CRA deprecated. |
| Plain HTML+JS | No component model, no TypeScript, unmaintainable for 9+ pages with real-time state |
| SolidJS | Smaller ecosystem, fewer battle-tested table/chart components |

**Why SvelteKit**: `@sveltejs/adapter-static` compiles to pure HTML/CSS/JS (~40KB bundle). Zero Node.js in production. Output goes into `dashboard/build/` and axum serves it via `tower_http::services::ServeDir`. TypeScript for type-safe API calls. Built-in routing.

### Security Hardening
- CSP: `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' wss:; img-src 'self' data:`
- `X-Frame-Options: DENY` (prevent clickjacking of approval flows)
- `X-Content-Type-Options: nosniff`
- `Strict-Transport-Security: max-age=31536000; includeSubDomains`
- `Referrer-Policy: same-origin`
- All fonts/icons bundled — **air-gap compatible, zero CDN dependencies**
- Auth: `HttpOnly` + `Secure` + `SameSite=Strict` cookies
- No inline JavaScript anywhere

### Deployment
- Docker multi-stage: `FROM node:22-alpine AS dashboard` → `npm ci && npm run build` → copy `build/` into Rust image
- `npm ci --offline` works with cached `node_modules` (air-gap support)
- axum serves dashboard at `/` with fallback to `index.html` for client-side routing

### 21A — Dashboard Infrastructure (2 weeks)
- [ ] Create `dashboard/` directory with SvelteKit scaffold + `adapter-static`
- [ ] TypeScript API client generated from endpoint catalog
- [ ] axum `ServeDir` fallback route in `crates/gateway/src/routes.rs`
- [ ] Security headers middleware in `crates/gateway/src/middleware.rs` (CSP, HSTS, X-Frame-Options)
- [ ] Login page with FIDO2 hardware key authentication
- [ ] Session management (cookie → Bearer token for API calls)
- [ ] WebSocket client wrapper with auto-reconnection (connects to existing `/v1/ws`)
- [ ] Shared layout: sidebar navigation, header with user info, notification badge
- [ ] Dark/light theme (CSS variables only, no JS theme library)
- [ ] Dockerfile multi-stage build (Node → Rust)

### 21B — Core Pages (3 weeks)
- [ ] **Dashboard**: total AUM, hot/warm/cold balances, recent transactions, chain health from `/health/chains`, active alerts
- [ ] **Wallets**: vault list from `/v1/vaults`, per-vault accounts, balances, tier visualization (5%/15%/80%)
- [ ] **Transactions**: unified list from `/v1/transactions`, filter by chain/status/amount/date, detail panel with on-chain explorer link
- [ ] **Addresses**: address book, whitelist management, internal vs external categorization, add/remove with policy enforcement
- [ ] **Audit Log**: tamper-evident log viewer, filter by operation/user/time, hash chain verification indicator

### 21C — Operational Pages (2 weeks)
- [ ] **Approvals**: pending queue, approve/reject with FIDO2 challenge, history, tier indicator, time-delay countdown
- [ ] **Policies**: rule list with priority ordering, rule editor (conditions + actions), simulation mode toggle, version history
- [ ] **Users**: user list with role badges, invite flow, FIDO2 credential management, session listing, disable/enable
- [ ] **Settings**: chain config, circuit breaker limits, webhook endpoints, API key management, gas station status

### 21D — Polish (1 week)
- [ ] Responsive design (desktop + tablet minimum)
- [ ] Keyboard shortcuts (Approve: Ctrl+Enter, Reject: Ctrl+Backspace)
- [ ] Error boundary with structured error display
- [ ] Loading/empty/error states for all pages
- [ ] E2E tests with Playwright (headless)

---

## Phase 22 — AML/KYT Real Vendor Integration (P1, 3-4 weeks)
> **Why**: Regulatory requirement for any custody service handling real money. `ScreeningProvider` trait exists in `crates/compliance/src/screening.rs` but is a stub — zero actual HTTP calls. Operators in regulated jurisdictions cannot go live without this.
> **Tech**: `reqwest` (already in workspace) for Chainalysis KYT v2 and Elliptic Lens APIs.
> **Depends on**: Phase 19 (admin permissions for provider config)

- [ ] `ChainalysisProvider`: implement HTTP calls to `https://api.chainalysis.com/api/kyt/v2/` — register transfer, get risk assessment
- [ ] `EllipticProvider`: implement `ScreeningProvider` trait against Elliptic Lens API
- [ ] DB migration 013: `screening_cache` table (address, chain, risk_level, risk_score, provider, screened_at, expires_at)
- [ ] Cache layer: check DB before vendor API call, TTL configurable (default 24h)
- [ ] Fallback mode config: `compliance.on_provider_failure = "block" | "allow_and_flag"` — operator chooses behavior when provider is down
- [ ] Wire screening into deposit processing: post-deposit screen of source address
- [ ] Wire screening into withdrawal pipeline: pre-sign screen of destination address
- [ ] Admin endpoints: `POST /v1/admin/compliance/providers`, `GET /v1/admin/compliance/screenings`
- [ ] Security: API keys encrypted (AES-256-GCM, same pattern as seed storage), no PII in logs
- [ ] Dashboard integration: screening widget with risk distribution, blocked count, flagged-for-review count
- [ ] Tests: mock provider responses, cache hit/miss, fallback behavior, high-risk blocking

---

## Phase 23 — UTXO Consolidation Execution (P1, 2-3 weeks)
> **Why**: Bitcoin operational cost. UTXO set grows with small deposits, increasing fees over time. Detection exists (`crates/utxo/`) and sweep recommendations exist (`crates/wallet-manager/`), but no actual consolidation transactions are built/broadcast.
> **Tech**: Reuse existing `select_coins()` from `crates/utxo/` and `BitcoinBroadcaster` from `crates/broadcaster/`.
> **Depends on**: None (uses existing Bitcoin signing pipeline)

- [ ] `ConsolidationPolicy` config: min_utxo_count_trigger (default: 50), max_utxo_per_consolidation (default: 20), max_fee_rate_sat_per_vb (default: 10), schedule (default: daily 03:00 UTC)
- [ ] `ConsolidationTransaction` builder: select dust + small UTXOs → single-output to internal hot wallet address
- [ ] Wire through signing pipeline (same flow: Received → Validated → Signed → Broadcast → Confirmed)
- [ ] Mark consolidation transactions as `tx_type = "consolidation"` — circuit breaker MUST skip these (internal movements, not outflow)
- [ ] `ConsolidationScheduler` background task: check UTXO count + fee environment → submit if conditions met
- [ ] Admin manual trigger: `POST /v1/admin/utxo/consolidate`
- [ ] Security: destination MUST be internal hot wallet address (never external), through full signing pipeline (no shortcuts)
- [ ] Metrics: `utxo_count` gauge, `consolidation_tx_count` counter, `consolidation_savings_sats` counter
- [ ] Tests: selection logic, fee gate, circuit breaker bypass, internal-address-only validation

---

## Phase 24 — Swap Execution Engine (P2, 4-6 weeks)
> **Why**: DeFi access is a competitive differentiator. Calldata builders exist (`dex_calldata.rs` for Uniswap V3, `jupiter.rs` for Jupiter) but nothing fetches quotes, approves tokens, or broadcasts swap transactions.
> **Tech**: Uniswap V3 SwapRouter02 via `alloy`, Jupiter v6 API for Solana. Direct DEX contract calls — NOT WalletConnect (institutional custody calls contracts directly).
> **Depends on**: Phase 19 (permission control), Phase 22 (screen swap contracts)

### 24A — Swap Infrastructure
- [ ] `SwapExecutor` trait: `quote()`, `approve_if_needed()`, `execute()`, `status()`
- [ ] `UniswapV3Executor`: fetch pool state for fee tier → calculate expected output → encode approve calldata → encode swap via existing `encode_uniswap_v3_swap()` → submit through withdrawal pipeline
- [ ] `JupiterExecutor`: call Jupiter v6 quote API → call swap API for serialized tx → sign via Solana signer → broadcast
- [ ] Price validation: on-chain TWAP or Chainlink (ETH) / Pyth (SOL) check before execution

### 24B — Swap API & Controls
- [ ] `POST /v1/swaps` — create swap request (from_token, to_token, amount, max_slippage_bps)
- [ ] `GET /v1/swaps/{id}` — swap status
- [ ] Approval integration: swaps above $10K require approval (reuse existing ApprovalRequest)
- [ ] Security: exact token approvals only (NEVER `type(uint256).max`), 200bps max slippage enforced, MEV protection (Flashbots Protect on Ethereum, Jito on Solana), swap destination MUST be internal custody address
- [ ] Dashboard integration: swap page with quote preview, slippage visualization, execution history
- [ ] Tests: quote fetching (mocked), calldata verification, slippage enforcement, approval flow

---

## Phase 25 — OpenAPI Spec Generation (P2, 1-2 weeks)
> **Why**: Current spec is hand-maintained in `crates/common/src/developer.rs`. Drifts from actual handlers. Auto-generation enables reliable SDK generation and documentation.
> **Tech**: `utoipa = "5"` with `utoipa-axum` — derives OpenAPI from handler function signatures. Most mature Rust OpenAPI generator, works with axum extractors. Alternative `aide` is less mature.
> **Depends on**: Phase 19 (auth in spec), Phase 21 (serve spec on dashboard)

- [ ] Add `utoipa` + `utoipa-axum` to workspace dependencies
- [ ] Annotate request/response types with `#[derive(utoipa::ToSchema)]`
- [ ] Annotate route handlers with `#[utoipa::path(...)]` attributes
- [ ] Serve auto-generated spec at `/v1/openapi.json` (replaces hand-maintained version)
- [ ] Optional Swagger UI at `/docs` (gated behind config flag, disabled by default in production)
- [ ] Remove hand-maintained `all_endpoints()` and `generate_openapi_spec()` from `developer.rs`
- [ ] CI check: compile-time OpenAPI spec validation
- [ ] Tests: spec is valid OpenAPI 3.1, all endpoints present, auth requirements documented

---

## Dependency Graph (Phases 19-25)

```
Phase 19 (RBAC) ──┬──> Phase 20 (FIDO2) ──> Phase 21 (Dashboard)
                   ├──> Phase 22 (AML)
                   └──> Phase 24 (Swaps — also needs Phase 22)

Phase 23 (UTXO) ─────> independent, can start anytime

Phase 19 + 21 ────────> Phase 25 (OpenAPI)
```

**Recommended execution**: 19 → 20 → 21A → [22 + 23 in parallel] → 21B/C/D → 24 → 25

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
| MPC CGGMP21 (secp256k1) | Phase 4A | ✅ Crypto + HTTP transport + coordinator |
| FROST Ed25519 | Phase 4B | ✅ Crypto + distributed DKG + HTTP transport |
| HSM adapters | Phase 4C | ✅ HsmSigningBackend operational (encrypted seed, on-demand decrypt) — real PKCS#11 needs cryptoki |

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

### Trust Infrastructure (Phase 17 ✅ COMPLETE)
| Feature | Phase | Status |
|---------|-------|--------|
| Threat model document | Phase 17A | ✅ THREAT_MODEL.md |
| Security architecture doc | Phase 17A | ✅ SECURITY.md |
| Signer process isolation | Phase 17B | ✅ Binary + IPC + pipeline wired |
| Circuit breaker persistence | Phase 17C | ✅ DB-persisted (migration 009) |
| Attack simulation tests | Phase 17D | ✅ 33 security tests |
| Supply chain (pinning, deny.toml) | Phase 17E | ✅ cargo-deny + Dockerfile pinned |
| Deployment modes guide | Phase 17F | ✅ docs/DEPLOYMENT_MODES.md |
| Security certifications | Phase 16 | ❌ Blocked — needs SOC 2 auditors |

### Product Gaps (Phases 19-25 — NEXT)
> Identified 2026-03-27 via honest assessment: strong backend, but missing critical product features for adoption.

| Feature | Phase | Status | Priority |
|---------|-------|--------|----------|
| RBAC enforcement middleware | Phase 19 | ✅ Done — middleware + extractors + guards on ALL 125+ endpoints | P0 |
| FIDO2/WebAuthn verification | Phase 20 | ✅ Done — registration + authentication flows, challenge management, credential CRUD | P0 |
| **Web Dashboard (SvelteKit)** | Phase 21 | ❌ Not started — zero frontend code | P0 |
| AML/KYT real vendor integration | Phase 22 | ❌ Not started — trait stubs only | P1 |
| UTXO consolidation execution | Phase 23 | ❌ Not started — detection only | P1 |
| Swap execution engine | Phase 24 | ❌ Not started — calldata builders only | P2 |
| OpenAPI auto-generation | Phase 25 | ❌ Not started — hand-maintained spec | P2 |
