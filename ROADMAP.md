# Roadmap

> **Target**: Feature parity with Fireblocks-class institutional custody.
> **Strategy**: Self-hosted = no vendor lock-in, data sovereignty, cost at scale.
> **Benchmark**: Fireblocks (150+ chains, MPC-CMP, SGX policy engine, $10T+ processed).

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
- [ ] Sepolia testnet integration test
- [ ] Wire SelfHostedCustodyProvider in exchange

## Phase 3 — Multi-Chain (6-8 weeks) ✅ DONE
- [x] Bitcoin: UTXO coin selection (BnB + greedy), scanner, fee estimation, P2WPKH signing, broadcaster
- [x] Tron: HTTP API scanner, Energy/bandwidth model, TRC-20 signing, broadcaster
- [x] Solana: RPC scanner, SPL token support, Ed25519 signing, blockhash retry, broadcaster
- [x] ChainAdapter trait unification

---

## Phase 4 — MPC Signing & HSM (8-12 weeks) 🔄 IN PROGRESS
> **Why**: Single-seed architecture = single point of total fund loss. Every institutional provider uses MPC or multi-sig. This is the #1 blocker for production.

### 4A — Threshold ECDSA (CGGMP21) for secp256k1 chains
- [x] New `custody-mpc` crate (15th workspace crate)
- [x] MpcSigner trait + KeygenResult + ThresholdSignature types
- [x] CGGMP21 DKG stub (2-of-3 and arbitrary t-of-n)
- [x] CGGMP21 threshold signing stub (presigning + signing)
- [ ] Full cggmp21 protocol integration (message exchange between parties)
- [ ] Key share rotation without changing public keys (pending cggmp21 support)
- [x] Presigning cache for low-latency operations

### 4B — FROST threshold EdDSA for Ed25519 chains
- [x] FROST trusted dealer keygen (frost-ed25519 v2.2, RFC 9591)
- [x] FROST 2-round threshold signing (commitments → shares → aggregate)
- [x] Sign + verify roundtrip tested for all 3 combinations of 2-of-3
- [x] Wrong message verification fails (security test)
- [ ] Distributed DKG (replace trusted dealer for production)

### 4C — HSM Integration (production key storage)
- [x] HsmAdapter trait (store/load/delete/list)
- [x] SoftwareHsm (in-memory, dev/test)
- [x] KeyStore wrapper with encrypted storage
- [x] PKCS#11 integration (SoftHSM2 for dev, Thales/Utimaco for prod)
- [x] AWS CloudHSM adapter
- [x] Azure Dedicated HSM adapter (via CloudHSM wrapper)
- [x] Hardware-enforced rate limiting on signing operations

### 4D — Key Lifecycle Management
- [ ] Key rotation schedule (automatic, no downtime)
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
- [ ] Mobile push notifications for pending approvals

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
- [ ] Others (per demand)

### 8C — Chain Infrastructure
- [x] Dynamic token registry (EvmTokenDef per chain)
- [x] Fallback RPC URL support
- [x] Chain configuration hot-reload
- [x] Multi-RPC failover logic
- [x] Chain health monitoring (RPC failover + metrics)

## Phase 9 — Staking & Yield (6-8 weeks) ✅ DONE (types)
> Staking types and parameters defined. Implementation needs chain RPC integration.

- [x] StakingPosition, StakingStatus, StakingParams types
- [x] ETH staking params (32 ETH min, ~27h unbonding, ~3.6% APY)
- [x] SOL staking params (1 SOL min, ~2-3 day unbonding, ~7% APY)
- [x] Beacon chain deposit types (EthStakeRequest)
- [x] SOL delegation types (SolDelegateRequest)
- [x] Rewards tracking (StakingReward type)
- [x] Validator performance monitoring (ValidatorMetrics)

## Phase 10 — DeFi Access (8-12 weeks) ✅ DONE (types)
> DeFi types defined. Implementation needs WalletConnect SDK + DEX APIs.

- [x] WalletConnectSession, SwapRequest, SwapStatus types
- [x] TokenAllowance with risk detection (unlimited = risky)
- [x] Slippage validation (max 2% for institutional)
- [x] WalletConnect v2 types (session/request/response)
- [x] DEX aggregator types (swap routes, hops, price impact)
- [x] Token allowance scanner types
- [x] DeFi protocol whitelisting (DefiProtocolEntry)

## Phase 11 — Observability & Operations (3-4 weeks) ✅ DONE (types)
> Metric definitions and health check types. Implementation needs Prometheus + Grafana.

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
- [ ] Multi-region database replication
- [ ] Automatic failover for chain nodes
- [ ] DR runbook and regular drill schedule
- [ ] Backup verification (automated restore tests)
- [ ] RTO < 1 hour, RPO < 1 minute targets

### 11D — Performance
- [x] Load testing scenarios (smoke/standard/peak/stress)
- [x] Latency benchmarks (p50/p95/p99 types)
- [ ] Connection pool tuning
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

## Phase 13 — Settlement & Counterparty Network (8-12 weeks) ✅ DONE (types)
> Settlement types defined. Implementation needs counterparty API integration.

- [x] SettlementTransfer with status tracking
- [x] Counterparty connection management types
- [x] Internal settlement execution (netting engine)
- [x] Counterparty API protocol (Hello/Request/Confirm/Reject)
- [x] Settlement netting (bilateral)

## Phase 14 — Tokenization & RWA (6-8 weeks) ✅ DONE (types)
> Tokenization types defined. Implementation needs smart contract integration.

- [x] TokenStandard (ERC-20/721/1155, SPL, TRC-20)
- [x] ManagedToken with full lifecycle tracking
- [x] TokenOperation enum (Mint/Burn/Pause/Freeze)
- [x] Smart contract deployment types
- [x] Token lifecycle types

## Phase 15 — Platform & WaaS (8-12 weeks) ✅ DONE (types)
> WaaS types defined. Implementation needs multi-tenant infrastructure.

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

## Phase 16 — Security Certifications (ongoing)
> **Why**: SOC 2 Type 2 is table stakes. CCSS Level 3 is gold standard.

- [ ] SOC 2 Type 2 audit (3-6 month engagement)
- [ ] ISO 27001 certification
- [ ] CCSS Level 3 assessment
- [ ] Annual penetration testing program
- [x] Bug bounty program (types + rewards)
- [x] Security incident response plan
- [ ] Insurance program (broker engagement)

---

## Summary: Fireblocks Parity Tracker

| Fireblocks Feature | Our Phase | Status |
|-------------------|-----------|--------|
| Key management (basic) | Phase 1 | ✅ Done |
| 4 chain support | Phase 3 | ✅ Done |
| Deposit scanning | Phase 2-3 | ✅ Done |
| Withdrawal pipeline | Phase 2 | ✅ Done |
| MPC signing | **Phase 4** | 🔜 Next |
| HSM integration | **Phase 4** | 🔜 Next |
| Policy engine (TAP) | Phase 5 | Planned |
| AML/KYT screening | Phase 6 | Planned |
| OFAC/sanctions | Phase 6 | Planned |
| Travel Rule | Phase 6 | Planned |
| Hot/warm/cold tiering | Phase 7 | Planned |
| Gas Station | Phase 7 | Planned |
| 50+ chains | Phase 8 | Planned |
| Staking | Phase 9 | Planned |
| DeFi suite | Phase 10 | Planned |
| Monitoring/alerting | Phase 11 | Planned |
| Proof of Reserves | Phase 12 | Planned |
| Settlement network | Phase 13 | Planned |
| Tokenization/RWA | Phase 14 | Planned |
| WaaS platform | Phase 15 | Planned |
| SOC 2 / CCSS | Phase 16 | Planned |
