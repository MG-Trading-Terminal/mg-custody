# Changelog

All notable changes to this project will be documented in this file.
Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added (2026-03-26) — Level 28: Bitcoin Script + EVM RLP + Crypto Utils
- **Bitcoin Script** — opcodes, P2PKH/P2WPKH/P2WSH/multisig/HTLC/OP_RETURN builders
- **EVM RLP** — encode/decode, tx type detection (Legacy/EIP-1559/EIP-4844)
- **Crypto utils** — SHA-256, Keccak-256, HMAC, Base58Check, EIP-55 checksum
- **52 new tests** (979 total — 21 to 1000!)

### Added (2026-03-26) — Level 27: ZK Proofs + Solana Instructions
- **ZK Proof of Reserves** — Merkle tree construction, proof verification, attestation generation
  - Pedersen commitments, range proofs, user balance leaf hashing
  - Solvency check (total_assets >= total_liabilities)
- **Solana instruction builders** — 12 instruction types across 4 programs
  - System: Transfer, CreateAccount
  - SPL Token: Transfer, Approve, MintTo, Burn, CloseAccount
  - Stake: Initialize, Delegate, Deactivate, Withdraw
  - Memo: arbitrary message
- **27 new tests** (927 total)

### Added (2026-03-26) — Level 26: Lending Protocols + Yield Strategies
- **Aave V3** — supply/borrow/repay/withdraw calldata encoding, health factor calculation
- **Compound V3** — supply/withdraw calldata, Comet addresses
- **Lending markets** — 7 markets with APYs and LTV ratios, liquidation detection
- **Yield strategies** — 6 strategies (Maker DSR, Yearn, Convex, Ethena, EigenLayer, EtherFi)
  - Risk classification, APY comparison, asset-specific lookup
- **19 new tests** (900 total)

### Added (2026-03-26) — Level 25: ABI Decoder + Tron Energy + Event Parsing
- **ABI decoder** — 26 function selectors, 7 event topics, Transfer event decoding
- **Tron Energy** — TRC-20 cost estimation, resource checks, freeze calculation
- **19 new tests** (881 total)

### Added (2026-03-26) — Level 24: ENS + Safe{Wallet} + ERC-4626 Vaults
- **ENS/Name Resolution** — namehash computation (verified vitalik.eth), resolver/addr calldata
  - Unstoppable Domains, Solana Name Service, SpaceID support
  - Auto-detect resolver by TLD (.eth/.crypto/.sol/.bnb)
- **Safe{Wallet}** — execTransaction/multiSend encoding, proxy factory addresses
  - Transaction hashing, multi-send batch packing
- **ERC-4626 Vaults** — deposit/withdraw/convertToShares/convertToAssets calldata
  - 3 known vaults (sDAI, yvUSDC, maUSDC) with APY estimates
- **21 new tests** (862 total)

### Added (2026-03-26) — Level 23: Gas Optimization + Multicall + Permit + Flashloans
- **Multicall3** — batch calls, gas savings estimator (20%+ savings on 10 txs)
- **ERC-20 Permit (EIP-2612)** — gasless approvals, 8 permit-supporting tokens
- **Aave V3 Flash Loans** — 0.05% fee calculation, selector encoding
- **10 new tests** (841 total)

### Added (2026-03-26) — Level 22: EIP-712 + Token-2022 + Cross-chain Messaging
- **EIP-712 Typed Data** — domain separator hashing, type encoding, struct hash
- **Solana Token-2022** — 13 extension types, transfer fee calculation
- **Cross-chain messaging** — Wormhole/LayerZero/CCIP/Hyperlane with chain IDs
- **19 new tests** (831 total)

### Added (2026-03-26) — Level 21: ERC-4337 + Taproot + MEV + Compute Budget
- **ERC-4337 Account Abstraction** — UserOperation types, EntryPoint v0.6/v0.7, handleOps calldata
  - 4 bundler endpoints (Pimlico, Stackup, Alchemy, Biconomy), smart account factories
- **Bitcoin Taproot (P2TR)** — address detection, input vBytes (58 vB, most efficient)
  - Fee estimation per address type, PSBT metadata, Ordinals inscription detection
  - BIP-21 URI builder with amount/label
- **MEV Protection** — Flashbots Protect, MEV Blocker, Jito block engine
  - Risk assessment (sandwich/frontrun detection based on tx type + value)
  - Flashbots bundle request builder, Jito tip account
- **Solana Compute Budget** — SetComputeUnitLimit/Price instruction encoding
  - Recommended units: transfer (200K), token (300K), swap (600K), stake (400K)
- **23 new tests** (812 total)

### Added (2026-03-26) — Level 20: Polkadot + Jupiter + Token Auto-Scan
- **Polkadot/Substrate** — 5 chains (Polkadot, Kusama, Moonbeam, Acala, Astar)
  - SS58 validation, Substrate calls (transfer, bond, nominate, XCM)
- **Jupiter DEX** (Solana) — V6 swap instruction, 10 known mints, route building
- **Token auto-scan** — detect held tokens via batch RPC (EVM/Solana/Tron)
  - Top 15 ERC-20 contracts, Solana getTokenAccountsByOwner, TronGrid URLs
- **24 new tests** (789 total)

### Added (2026-03-26) — Level 19: Staking Contracts + WalletConnect v2 + Cosmos
- **Lido staking calldata** — `submit()` for staking, `requestWithdrawals()` for unstaking
- **Rocket Pool** — `deposit()` for rETH minting, `burn()` for redemption
- **Marinade Finance** — Solana deposit instruction discriminator, program IDs
- **Known validators** — 5 validators/pools (Lido, Rocket Pool, Coinbase, Marinade, Jito) with APY/fees
- **WalletConnect v2** — full session lifecycle types
  - Pairing, session, peer metadata, namespace building (EIP-155 + Solana)
  - WC URI parsing, JSON-RPC request types, EVM + Solana method constants
- **Cosmos SDK chains** — 5 chains (Cosmos Hub, Osmosis, Celestia, dYdX, Injective)
  - MsgSend, MsgDelegate, MsgUndelegate, MsgWithdrawRewards, IBC MsgTransfer
  - Transaction body builders for send + IBC transfer
- **25 new tests** (765 total)

### Added (2026-03-26) — Level 18: DeFi Calldata + Bridges + Allowances
- **Uniswap V3 calldata** — `exactInputSingle` ABI encoding with fee tier, recipient, price limit
- **Curve calldata** — `exchange(int128,int128,uint256,uint256)` pool swap encoding
- **ERC-20 approve/allowance** — approve calldata, allowance query, unlimited detection
- **Balance queries** — `balanceOf(address)` calldata encoding
- **Cross-chain bridges** — 6 routes across 5 protocols (Native L2, Wormhole, LayerZero, Across, Stargate)
  - ETH→Arbitrum/Optimism/Base (native), ETH→Polygon (Stargate), SOL→ETH (Wormhole)
  - Route lookup by source chain + token
- **12 new tests** (740 total)

### Added (2026-03-26) — Level 17: Token Registry + Price Feeds + NFT
- **Token database** — 55+ verified tokens across 4 chains with contract addresses
  - 4 native (BTC/ETH/TRX/SOL), 26 ERC-20, 8 TRC-20, 12 SPL tokens
  - Lookup by symbol+chain, by contract address (case-insensitive)
  - CoinGecko IDs, stablecoin flag, verified status
  - Top DeFi: UNI, AAVE, LINK, CRV, LDO, MKR, COMP, SNX
  - Liquid staking: stETH, rETH, cbETH, mSOL, stSOL
  - Meme: SHIB, PEPE, BONK, WIF
- **Price engine** — multi-source aggregation with median
  - 10 Chainlink price feeds (ETH, BTC, LINK, USDT, USDC, SOL, DAI, AAVE, UNI, MKR)
  - 5 Pyth Network feeds (BTC, ETH, SOL, USDT, USDC)
  - Staleness detection, portfolio value calculation
- **NFT custody** — ERC-721, ERC-1155, Solana Metaplex support
  - `transferFrom` calldata encoding (ERC-721)
  - `safeTransferFrom` calldata encoding (ERC-1155)
  - `approve` calldata encoding (ERC-721)
  - NFT asset tracking with metadata (name, image, collection)
- **33 new tests** (728 total)

### Added (2026-03-26) — Level 16: Deep Implementation (6 features in parallel)
- **DeFi execution** — swap routing with multi-hop, DEX aggregator, slippage protection (max 2%), price impact checks
  - Known DEX configs: Uniswap V3, SushiSwap, Curve, Jupiter, Raydium, SunSwap
- **Settlement protocol** — inter-custody bilateral netting with real algorithm
  - `bilateral_net()` reduces gross legs to net transfers, calculates savings %
  - Protocol messages: Hello, ProposeSettlement, Accept, Reject, ConfirmExecution
- **Tokenization execution** — ERC-20 deploy calldata, mint/burn/pause/unpause encoding
  - Function selectors: mint (0x40c10f19), burn (0x42966c68), pause (0x8456cb59)
  - Constructor args ABI encoding
- **FIDO2/WebAuthn** — credential registration/authentication types
  - Challenge generation, attestation options, authenticator selection
  - UserVerification::Required for custody approvals
- **Prometheus metrics wiring** — 35+ metric definitions with standard labels
  - HTTP, deposits, withdrawals, signing, chain health, vaults, policy, compliance, webhooks
  - Load test scenarios: smoke (5 rps), standard (50), peak (150), stress (500)
- **33 new tests** (695 total)

### Added (2026-03-26) — Level 15: Accounting & Events
- **Ledger accounting** — balance sheets, asset balances with available calculation
  - P&L calculation (withdrawal fees - network fees - gas costs)
  - Fee tracking with aggregated summaries (total/avg/min/max)
- **Event catalog** — 31 custody event types across 8 categories
  - Categories: deposit, withdrawal, transfer, vault, policy, compliance, system, key_management
  - Severity classification (critical/warning/info)
- **Token bucket rate limiter** — configurable capacity and refill rate
  - Single and batch token consumption, exhaustion detection
- **Key fingerprints** — SHA-256 truncated to 8 bytes for safe key identification
  - Deterministic, never exposes key material
- **13 new tests** (662 total)

### Added (2026-03-26) — Level 14: Chain Utilities & Advanced Wallet
- **UTXO analysis** — coin selection stats, dust detection, consolidation recommendation
  - Dust threshold 546 sats (P2WPKH), fee estimation for consolidation
- **EVM gas utilities** — EIP-1559 calculation, gas limits for common ops
  - ETH transfer (21K), ERC-20 (65K), approve (46K), swap (200K), deploy (1M)
  - Safety buffer function, cost estimation in ETH
- **Deposit auto-forwarding** — configurable rules per chain/asset
  - Min amount threshold, destination vault, gas deduction option
- **Transaction categories** — 11 predefined categories (deposit, withdrawal, consolidation, gas_refuel, staking, sweep, etc.)
  - Custom labels + JSON metadata per transaction
- **Multi-sig wallet types** — Bitcoin P2SH-P2WSH, Ethereum Gnosis Safe, Solana Squads, MPC threshold
  - Signer management with weights, configurable M-of-N threshold
- **14 new tests** (649 total)

### Added (2026-03-26) — Level 13: Middleware & Infrastructure
- **Idempotency records** — key-based dedup with 24h expiry
- **Webhook signing** — HMAC-SHA256 per-endpoint for outbound payloads
  - Constant-time signature verification
- **Retry policies** — configurable per-chain with exponential backoff
  - ETH: 3 retries/1s, BTC: 5/2s, TRX: 3/500ms, SOL: 3/500ms/1.5x
- **Circuit breaker** — DB-backed state with check method
  - Single tx ($500K), hourly ($10M), daily ($50M) limits
  - Trip detection with reason tracking
- **Request analytics** — usage tracking with latency percentiles (p95/p99)
- **16 new tests** (635 total), all green

### Added (2026-03-26) — Level 12: Hardening & Validation
- **Multi-chain address validation** — real format validation for all 4 chains
  - Bitcoin: Bech32 (bc1q/bc1p), Legacy (1), P2SH (3) with character validation
  - Ethereum: 0x-prefix, 42 chars, hex validation, zero address rejection
  - Tron: T-prefix, 34 chars, Base58Check validation
  - Solana: 32-44 chars, Base58 validation
- **Error catalog** — 30+ standardized error codes across 7 categories
  - General (1xxx), Validation (2xxx), Vault (3xxx), Transaction (4xxx),
    Signing (5xxx), Policy (6xxx), Compliance (7xxx)
- **CLI tools** — key ceremony steps, wallet info, test fixtures
  - Shamir 3-of-5 ceremony workflow generator
  - Default wallet info for all 4 chains with derivation paths
  - Integration test fixtures per chain
- **27 new tests** (619 total), all green

### Added (2026-03-26) — Level 11: Developer Experience
- **OpenAPI 3.0 spec** — auto-generated from endpoint catalog
  - 100+ endpoints documented with tags, auth requirements, rate limits
  - `GET /v1/openapi.json`
- **SDK method definitions** — auto-generated method names for all endpoints
  - `GET /v1/sdk/methods`, `GET /v1/sdk/endpoints`
- **Endpoint catalog** — programmatic listing of all 100+ API endpoints
  - Method, path, summary, tag, auth required, rate limit per endpoint
- **PaginatedResponse** — standard cursor pagination wrapper type
- **8 new tests** (592 total), all green

### Added (2026-03-26) — Level 10: Production Polish
- **Transaction scheduling** — future-dated withdrawals with cancel support
  - Due-detection query for background executor, vault-scoped listing
  - `GET/POST /v1/scheduled`, `GET /v1/scheduled/{id}`, `POST .../cancel`
- **Compliance screening API** — address risk assessment endpoint
  - Risk levels (Low/Medium/High/Severe), sanctions flag, provider field
  - `POST /v1/screening/address`, `GET /v1/screening/history/{addr}`
- **Backup & recovery status** — key backup health dashboard
  - Encrypted seed status, Shamir shares, MPC key health, backup locations
  - `GET /v1/backup/status`
- **Token discovery** — auto-detect token metadata by contract address
  - Recognizes known contracts (USDT/USDC), supports ERC-20/TRC-20/SPL
  - `POST /v1/tokens/discover`
- **Health dashboard** — comprehensive system status
  - DB health, per-chain health, pipeline status, background tasks
  - `GET /v1/dashboard/health`, `GET /v1/dashboard/pipeline`
- **DB migration 008_operations.sql**: 1 new table (scheduled_transactions)
- **10 new API endpoints**, 9 new tests (584 total)

### Added (2026-03-26) — Level 9: Governance & Advanced Features
- **Vault policies** — per-vault spending rules (max amount, velocity limits, time locks, freeze, whitelist-only, require approvals)
  - Tagged enum serialization for rules; CRUD: `GET/POST /v1/vaults/{id}/policies`, `GET/PUT/DELETE /v1/policies/{id}`
- **Approval workflows** — multi-sig quorum with decision tracking
  - Duplicate approver detection, auto-status transition (approve/reject)
  - `GET /v1/approvals/{withdrawal_id}`, `POST .../decide`, `GET /v1/vaults/{id}/approvals`
- **Supported networks** — chain metadata with tokens, block times, explorers
  - 4 networks, 9 tokens (ETH/BTC/TRX/SOL + USDT/USDC variants)
  - `GET /v1/supported-networks`, `GET .../{chain}`, `GET .../{chain}/tokens`
- **Transaction notes** — attach metadata to any transaction
  - `GET/POST /v1/transactions/{id}/notes`
- **Staking execution** — stake/unstake with minimum validation and unbonding
  - ETH 32 min, SOL 1 min; unbonding periods (ETH 27d, SOL 3d)
  - `GET /v1/staking/positions`, `POST /v1/staking/stake`, `POST /v1/staking/unstake`
- **DB migration 007_governance.sql**: 3 new tables (vault_policies, approval_workflows, transaction_notes)
- **17 new API endpoints**, 11 new tests (575 total)

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
