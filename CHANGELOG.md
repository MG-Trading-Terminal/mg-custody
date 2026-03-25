# Changelog

All notable changes to this project will be documented in this file.
Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added (2026-03-26) — Level 8: Platform & Reporting
- **Workspace management** — multi-tenant isolation with CRUD
  - `GET/POST /v1/workspaces`, `GET/PUT /v1/workspaces/{id}`
- **Workspace users** — RBAC (Owner/Admin/Signer/Viewer), invite/remove
  - `GET/POST /v1/workspaces/{id}/users`, `DELETE .../users/{user_id}`
- **Deposit address listing + vault binding**
  - `GET /v1/deposit-addresses`, `POST /v1/deposit-addresses/{addr}/bind`
- **Reporting/export** — generate transaction reports in JSON or CSV
  - Filter by date range, chain, asset, vault; `POST /v1/reports/generate`
- **Signing key info** — public key metadata per vault/chain (NEVER private keys)
  - `GET /v1/vaults/{id}/keys`, `GET /v1/vaults/{id}/keys/{chain}`
- **Travel rule (FATF R.16)** — originator/beneficiary data collection
  - Automatic threshold detection (EU €1K / US $3K)
  - `POST /v1/travel-rule`, `GET /v1/travel-rule/{withdrawal_id}`
- **DB migration 006_platform.sql**: 3 new tables (workspaces, workspace_users, travel_rule_data)
- **15+ new API endpoints**, 11 new tests (564 total)

### Added (2026-03-25) — Level 7: Connectivity & Integrations
- **Exchange connections** (`connectivity.rs`, `db/connections.rs`, `gateway/connectivity_routes.rs`)
  - CRUD for exchange/OTC/custodian connections with heartbeat monitoring
  - `GET/POST /v1/connections`, `GET/PUT/DELETE /v1/connections/{id}`, `POST /v1/connections/{id}/heartbeat`
- **API key management** — create, list, revoke, delete with SHA-256 hashed storage
  - Raw key returned only on creation, scoped permissions (10 types), IP whitelist, rate limit
  - `GET/POST /v1/api-keys`, `GET /v1/api-keys/{id}`, `POST /v1/api-keys/{id}/revoke`
- **Notification channels** — Telegram, Slack, Email, Webhook, PagerDuty
  - Typed config per channel, min severity filter, enable/disable toggle, test delivery
  - `GET/POST /v1/notifications/channels`, `DELETE/POST toggle`, `POST test`
- **System configuration API** — runtime config via REST
  - Withdrawal limits (single/hourly/daily), pause withdrawals/deposits, maintenance mode
  - Confirmation overrides per chain
  - `GET/PUT /v1/system/config`
- **Network fees & status** — real-time fee cache per chain with block numbers
  - `GET /v1/network/fees`, `GET /v1/network/status`
- **DB migration 005_connectivity.sql**: 4 new tables (exchange_connections, api_keys, notification_channels, system_config)
- **20+ new API endpoints**, 13 new tests (553 total)

### Added (2026-03-25) — Level 6: Operational API
- **Webhook management API** (`common/webhook.rs`, `db/webhook_endpoints.rs`, `gateway/ops_routes.rs`)
  - Register endpoints with URL, event filters, chain filters
  - HMAC secret auto-generated per endpoint
  - Test webhook delivery (`POST /v1/webhooks/{id}/test`)
  - CRUD: `GET/POST /v1/webhooks`, `GET/PUT/DELETE /v1/webhooks/{id}`
- **Batch transactions** — submit up to 100 withdrawals in one call
  - Per-item validation, idempotency checks, partial failure support
  - `POST /v1/transactions/batch`
- **Audit log API** — query tamper-evident audit trail via REST
  - Filter by operation, actor, chain; max 500 entries
  - `GET /v1/audit-log?operation=signing_request&limit=100`
- **Address book** — saved destination addresses with labels and tags
  - Idempotent on (address, chain), whitelist flag for withdrawal approval
  - CRUD: `GET/POST /v1/address-book`, `GET/PUT/DELETE /v1/address-book/{id}`
- **Gas station status API** — per-chain gas balance and refuel status
  - `GET /v1/gas-station/status`
- **DB migration 004_webhooks_addressbook.sql**: 2 new tables (webhook_endpoints, address_book)
- **15+ new API endpoints**, 7 new tests (540 total)

### Added (2026-03-25) — Level 5: Fireblocks Functional Parity
- **Vault abstraction** (`common/vault.rs`): Vault, VaultAccount, InternalTransfer, Transaction types
  - Fireblocks' core organizational primitive (Vault → VaultAccount → Asset hierarchy)
  - 8 API endpoints: CRUD vaults, create/list/get vault accounts
- **Internal transfers** (vault-to-vault): instant off-chain ledger-only transfers
  - Double-entry ledger recording, cached balance updates, idempotency keys
  - `POST /v1/transfers`, `GET /v1/transfers/{id}`
- **Unified transaction history**: deposits + withdrawals + internal transfers in one view
  - Cursor-based pagination, filtering by vault/asset/chain/status/type/date/amount
  - `GET /v1/transactions`, `GET /v1/transactions/{id}`
- **Raw message signing** (`signing/raw.rs`): secp256k1 + Ed25519 arbitrary message signing
  - EIP-191 personal_sign, EIP-712 typed data, hex, UTF-8 encodings
  - Sign + verify roundtrip tests for both algorithms
  - `POST /v1/vaults/{id}/raw-sign`
- **Fee estimation API**: per-chain low/medium/high fee levels
  - ETH (gwei), BTC (sat/vB), TRX (energy), SOL (priority fee)
  - `GET /v1/estimate-fee?chain=ethereum`
- **Asset management API**: dynamic token registry in PostgreSQL
  - CRUD endpoints, case-insensitive lookup, enable/disable
  - `GET/POST /v1/assets`, `GET /v1/assets/{symbol}/{chain}`, `PUT/DELETE /v1/assets/{id}`
- **Transaction cancel/accelerate**: RBF fee bumping for BTC/ETH
  - Cancel: mark broadcast tx as cancelled
  - Accelerate: validate multiplier (1.125–5.0x), prepare replacement
  - `POST /v1/transactions/{id}/cancel`, `POST /v1/transactions/{id}/accelerate`
- **DB migration 003_vaults.sql**: 4 new tables (vaults, vault_accounts, internal_transfers, managed_assets)
- **20+ new API endpoints** across vault, transfer, transaction, asset, fee routes
- **24 new tests** (509 → 533 total), all passing

### Added (2026-03-25) — Production Infrastructure + End-to-End Wiring
- `docker-compose.yml`: PostgreSQL 16 + custody service with health checks
- `Dockerfile`: Multi-stage Rust build (alpine, ~15MB final image)
- `.env.example`: All configuration variables documented
- `scripts/dev-setup.sh`: One-command dev environment setup
- `server/main.rs`: Full rewrite — DB connect, migrations, 4 background task loops:
  - Ethereum scanner (12s interval, real Sepolia RPC)
  - Confirmation tracker (12s, checks deposit/withdrawal confirmations)
  - Withdrawal pipeline (5s, state machine transitions)
  - Webhook retry (30s, exponential backoff)
- Real SQL queries: scan_progress, nonces (no more stubs for core tables)
- Service starts, loads config, connects DB, spawns tasks, serves HTTP on :8443

### Added (2026-03-25) — SQL Migrations + Real DB Queries
- `db/migrations/001_initial_schema.sql`: Complete PostgreSQL schema (14 tables, 20+ indexes)
  - addresses + address_counters (atomic allocation)
  - deposits (dedup by tx_hash+log_index)
  - withdrawals (idempotency by exchange_withdrawal_id)
  - utxos, custody_ledger, scan_progress, nonce_counters
  - webhook_deliveries, audit_log, policy_rules, address_whitelist
  - approval_requests, compliance_screenings
- `db/addresses.rs`: Real sqlx queries (find, insert, all_for_chain, next_index)
- `db/deposits.rs`: Real sqlx queries (insert, find_by_tx_hash, by_status, update_status)
- `db/withdrawals.rs`: Real sqlx queries (insert, find, by_status, update_status, set_tx_hash)

### Added (2026-03-25) — Phases 12-16: Reconciliation, Settlement, Tokenization, WaaS
- `reconciliation/reconciler.rs`: On-chain vs ledger reconciliation with discrepancy detection (warning > 0.001%, critical > 1%)
- `reconciliation/proof_of_reserves.rs`: Merkle-tree PoR with BalanceLeaf, MerkleProof (verifiable), ReservesAttestation
- `common/settlement.rs`: SettlementTransfer, Counterparty types
- `common/tokenization.rs`: TokenStandard (ERC-20/721/1155), ManagedToken, TokenOperation lifecycle
- `common/waas.rs`: Workspace (Sandbox/Testnet/Mainnet), ApiKeyInfo, SdkLanguage
- 279 tests passing (18 new)

### Added (2026-03-25) — Phases 8-11: Extended Chains, Staking, DeFi, Observability
- `common/evm_chains.rs`: EvmChainRegistry with 9 pre-configured EVM chains (ETH, Arbitrum, Optimism, Base, Polygon, Avalanche, BSC, zkSync, Sepolia) + token contracts per chain
- `common/staking.rs`: StakingPosition, StakingStatus, StakingParams for ETH (32 ETH min) and SOL (1 SOL min)
- `common/defi.rs`: WalletConnectSession, SwapRequest, TokenAllowance with risk detection, slippage validation
- `common/metrics.rs`: Full Prometheus metric name definitions (deposits, withdrawals, chain health, signing, policy, compliance, wallets) + HealthStatus + ChainHealth
- 261 tests passing (20 new)

### Added (2026-03-25) — Phase 6: Compliance & AML + Phase 7: Wallet Infrastructure
- New crate: `custody-compliance` (17th workspace crate)
- `compliance/screening.rs`: ScreeningProvider trait, RiskLevel, ChainalysisProvider stub
- `compliance/ofac.rs`: SanctionsList + SanctionsChecker with known OFAC addresses (Tornado Cash)
- `compliance/travel_rule.rs`: TravelRuleProvider trait, NotabeneProvider, EU/US thresholds
- `compliance/risk.rs`: RiskAssessment aggregation (worst-of screening + sanctions)
- `wallet-manager/tiers.rs`: TierConfig (5/15/80%), TierStatus, refill/excess detection
- `wallet-manager/sweep.rs`: SweepRecommendation with direction + priority
- `wallet-manager/gas_station.rs`: GasStation with per-chain auto-funding configs
- 241 tests passing (33 new: 16 compliance + 17 wallet-manager)

### Added (2026-03-25) — Phase 5: Policy Engine
- New crate: `custody-policy` (16th workspace crate) — TAP-like transaction authorization
- `custody-policy/rule.rs`: PolicyRule with conditions (chain, asset, amount, destination, role) + actions (Allow, Deny, RequireApproval, Delay)
- `custody-policy/engine.rs`: Top-down first-match-wins evaluation engine (secure default: deny)
- `custody-policy/whitelist.rs`: Address whitelist with expiry, one-time use, case-insensitive lookup
- `custody-policy/approval.rs`: M-of-N quorum workflows with expiry, rejection, duplicate detection
- `custody-policy/rbac.rs`: Role hierarchy (Owner>Admin>Signer>Editor>Viewer) + 7 permissions
- `custody-db/policy.rs`: Persistence stubs for rules, whitelists, approvals
- 208 tests passing (31 new policy tests)

### Added (2026-03-25) — Phase 4: MPC Signing & HSM
- New crate: `custody-mpc` (15th workspace crate) — threshold signing infrastructure
- `custody-mpc/ecdsa/`: CGGMP21 threshold ECDSA for Bitcoin/Ethereum/Tron (keygen + sign stubs)
- `custody-mpc/frost/`: FROST threshold EdDSA for Solana (keygen + signing FULLY WORKING)
  - Trusted dealer keygen (2-of-3, 3-of-5)
  - 2-round signing with real signature verification
  - All 3 signer combinations tested for 2-of-3
  - Uses frost-ed25519 v2.2 (IETF RFC 9591, audited by NCC Group)
- `custody-mpc/hsm/`: HsmAdapter trait + SoftwareHsm (in-memory dev)
- `custody-mpc/coordinator.rs`: MPC session management
- `custody-mpc/key_store.rs`: Encrypted key share storage
- Dependencies: cggmp21 v0.6 (audited by Kudelski), frost-ed25519 v2.2 (audited by NCC)
- 177 tests passing (24 new MPC tests)

### Added (2026-03-25) — Phase 3: Multi-Chain (Bitcoin, Tron, Solana)
- `custody-utxo`: Bitcoin UTXO tracking + Branch-and-Bound / greedy coin selection (11 tests)
- `custody-scanner/bitcoin.rs`: Bitcoin Core RPC block scanner (stub + sat/BTC conversion)
- `custody-scanner/tron.rs`: Tron HTTP API scanner (stub + sun/TRX conversion)
- `custody-scanner/solana.rs`: Solana RPC scanner (stub + lamports/SOL conversion)
- `custody-fee-estimator/bitcoin.rs`: `estimatesmartfee` BTC/kB → sat/vB conversion
- `custody-fee-estimator/tron.rs`: Energy model cost estimation (TRC-20 burn cost)
- `custody-fee-estimator/solana.rs`: `getFeeForMessage` + base fee per signature
- `custody-signing/bitcoin.rs`: P2WPKH transaction structure, RBF support (BIP-125)
- `custody-signing/tron.rs`: TRC-20 parameter encoding (same ABI as ERC-20)
- `custody-signing/solana.rs`: SOL/SPL transfer params, ATA derivation
- `custody-broadcaster/bitcoin.rs`: `sendrawtransaction` broadcaster
- `custody-broadcaster/tron.rs`: HTTP API broadcaster
- `custody-broadcaster/solana.rs`: Blockhash expiry detection + retry logic
- `custody-common/chain_adapter.rs`: Unified `ChainAdapter` trait for all 4 chains
- 152 tests passing (26 new)

### Added (2026-03-25) — Research: Institutional Custody Gap Analysis
- `docs/gap-analysis-custody-providers.md`: Comprehensive feature comparison of Fireblocks, BitGo, Copper.co, Cobo, Liminal vs. MG Custody
- Identified 6 P0 (critical), 8 P1 (important), 8 P2 (nice-to-have) gaps
- Key finding: MPC signing (P0-1) and HSM integration (P0-2) are non-negotiable before production
- Key finding: AML/KYT + OFAC screening (P0-3, P0-6) are legal requirements, not optional features
- Recommended hybrid approach: self-hosted hot wallet + provider cold storage as viable middle path

### Added (2026-03-25) — Phase 2: Ethereum MVP
- `custody-common/config.rs`: Typed CustodyConfig loaded from TOML + env overrides (figment)
- `custody-db`: 4 new modules — scan_progress, nonces, webhook_deliveries, custody_ledger (double-entry)
- `custody-fee-estimator`: FeeEstimate trait + EthereumFeeEstimator (EIP-1559 baseFee + priorityFee via alloy 1.x)
- `custody-scanner/ethereum.rs`: EthereumScanner — ETH native transfers + ERC-20 Transfer log scanning
- `custody-scanner`: uint256_to_decimal/decimal_to_uint256 with mandatory tests for ETH(18), USDT(6), USDC(6)
- `custody-scanner/address_cache.rs`: DashMap-based address cache with periodic refresh
- `custody-confirmations/tracker.rs`: Generic confirmation tracker for deposits + withdrawals
- `custody-confirmations/webhook.rs`: HMAC-SHA256 signed webhook dispatch + exponential backoff
- `custody-signing/ethereum.rs`: EIP-1559 tx signing for ETH native + ERC-20 transfers
- `custody-broadcaster`: Broadcaster trait + EthereumBroadcaster + fee bump 12.5% logic
- `custody-withdrawal-pipeline`: State machine (Received → Validated → Signed → Broadcast → Finalized)
- `custody-withdrawal-pipeline/nonce_manager.rs`: Per-address Mutex, DB-persistent, chain sync on restart
- `custody-withdrawal-pipeline/validation.rs`: Address format validation + circuit breaker checks
- `custody-gateway/state.rs`: AppState with shared services (DB, allocator, pipeline)
- `custody-gateway/routes.rs`: Wired all 5 endpoints to real service logic
- `custody-server/main.rs`: Config loading, DB connection, background task spawning
- Upgraded alloy 0.12 → 1.x
- 126 tests passing (50 new)

### Added (2026-03-24) — Phase 1: Key Management
- `custody-address-factory`: HD wallet address derivation for all 4 chains
- `custody-address-factory/hd.rs`: BIP-32/BIP-44 secp256k1 key derivation with `SeedBytes` (Zeroize + ZeroizeOnDrop)
- `custody-address-factory/slip0010.rs`: SLIP-0010 Ed25519 derivation for Solana (manual HMAC-SHA512, hardened-only)
- `custody-address-factory/bitcoin.rs`: P2WPKH native SegWit Bech32 address encoding ([BIP-84](https://github.com/bitcoin/bips/blob/master/bip-0084.mediawiki), [BIP-173](https://github.com/bitcoin/bips/blob/master/bip-0173.mediawiki))
- `custody-address-factory/ethereum.rs`: EIP-55 mixed-case checksum address encoding ([EIP-55](https://eips.ethereum.org/EIPS/eip-55))
- `custody-address-factory/tron.rs`: Base58Check address encoding with 0x41 prefix
- `custody-address-factory/solana.rs`: Ed25519 public key Base58 encoding
- `custody-address-factory/allocator.rs`: Idempotent address allocation with DB-backed index counter
- `custody-signing/encrypted_store.rs`: AES-256-GCM encrypted seed storage with PBKDF2-HMAC-SHA256 (600K iterations)
- `custody-signing/shamir.rs`: Shamir Secret Sharing over GF(256) — 3-of-5 split/reconstruct for seed backup
- `custody-signing/software.rs`: SoftwareSigningBackend (dev/test) + HsmSigningBackend stub (PKCS#11)
- `custody-db/audit.rs`: Key management audit log with tamper-evident hash chain design
- Verified against test vectors: BIP-84 (Bitcoin), EIP-55 (Ethereum), SLIP-0010 (Solana/Ed25519)
- 76 tests passing (69 new)
- `custody-db`: Added `next_address_index()` stub for atomic address counter

### Added (Phase 0 foundation)
- Initial 14-crate workspace structure (Phase 0 foundation)
- `custody-common`: Core types — Chain, Asset, Deposit, Withdrawal, CustodyEvent, WalletTier
- `custody-common`: Error types — CustodyError with chain, signing, compliance, and infra variants
- `custody-db`: PostgreSQL persistence stubs (sqlx) — addresses, deposits, withdrawals repos
- `custody-gateway`: axum REST API skeleton — POST /addresses, POST /withdrawals, GET /withdrawals/:id, GET /wallets/:asset, GET /health
- `custody-gateway`: HMAC-SHA256 request authentication with timestamp validation
- `custody-scanner`: ChainScanner trait for per-chain block polling
- `custody-signing`: SigningBackend trait for swappable key management (software → HSM → MPC)
- `custody-server`: Binary entry point with tracing + JSON structured logging
- `config/default.toml`: Full configuration with chain endpoints, token contracts, limits, wallet policy
- 7 tests passing (types serialization, HMAC auth)
- CLAUDE.md with project rules, security requirements, chain-specific rules
- 6 specialized agents: architect, developer, security, ops, blockchain, pm
- 6 slash commands matching agents

### References
- Architecture: ~/Projects/exchange/docs/roadmaps/self-hosted-custody-research.md
- Exchange CustodyProvider trait: ~/Projects/exchange/crates/custody/src/provider.rs
- BIP-44 coin types: https://github.com/satoshilabs/slips/blob/master/slip-0044.md
- EIP-1559: https://eips.ethereum.org/EIPS/eip-1559
- SLIP-0010 (Solana): https://github.com/satoshilabs/slips/blob/master/slip-0010.md
