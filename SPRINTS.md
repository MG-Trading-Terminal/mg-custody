# Sprint Log

## Sprint N+5 — Phase 30: Batch Transactions, Address Labels, Token Import, Workspace Isolation (2026-03-27)

### Goals
- Atomic batch withdrawal endpoint (all-or-nothing, RequireSigner, max 50)
- Address book labels + notes + fuzzy search
- Runtime custom token import via API
- Multi-workspace vault isolation at DB and route layer

### Completed
| Task | Crate/File | Status | Tests | Notes |
|------|-----------|--------|-------|-------|
| Atomic batch withdrawal `POST /v1/withdrawals/batch` | ops_routes.rs, db/withdrawals.rs | DONE | 10 | RequireSigner, 50-item cap, single TX |
| `insert_withdrawals_atomic` DB method | db/withdrawals.rs | DONE | — | sqlx transaction, ON CONFLICT DO NOTHING |
| Migration 018 address_book labels | db/migrations/018_address_labels.sql | DONE | — | notes, last_used_at, GIN index |
| `update_address_label` + `search_addresses` DB methods | db/address_book.rs | DONE | 3 | ILIKE escape, 50-result cap |
| `PUT /v1/address-book/{id}/label` + `GET /v1/addresses/search` | ops_routes.rs | DONE | 3 | RequireEditor / RequireViewer |
| `AddressBookEntry` notes + last_used_at fields | common/webhook.rs | DONE | — | backward-compatible, Option fields |
| `ImportTokenRequest` type + validate_contract_address | common/vault.rs, vault_routes.rs | DONE | 12 | ETH/Tron/Solana/BTC validation |
| `POST /v1/admin/tokens/import` endpoint | vault_routes.rs | DONE | — | RequireAdmin, 409 on duplicate |
| `find_vault_for_workspace` + `list_vaults_for_workspace` | db/vaults.rs | DONE | 2 | owner_id = user_id guard |
| Workspace isolation in get/update/delete vault + account endpoints | vault_routes.rs | DONE | — | All 6 handlers updated |
| `ApiError::unprocessable_entity` (HTTP 422) | gateway/errors.rs | DONE | 1 | Batch validation failure |

### Metrics
- **New tests added**: 31 (10 ops_routes batch + 3 address_book DB + 3 ops_routes address + 12 vault_routes token + 1 errors + 2 vault workspace)
- **Tests passing**: 1,527 (all passing, 0 failed)
- **New migrations**: 1 (`018_address_labels.sql`)
- **Modified files**: 10 (ops_routes.rs, vault_routes.rs, errors.rs, db/address_book.rs, db/vaults.rs, db/withdrawals.rs, common/webhook.rs, common/vault.rs)
- **Clippy**: zero new warnings in all modified files (pre-existing common crate warnings are unchanged)

### References Used
| Reference | URL | Used For |
|-----------|-----|----------|
| PostgreSQL ILIKE | https://www.postgresql.org/docs/current/functions-matching.html | Fuzzy label/address search |
| PostgreSQL transactions (sqlx) | https://docs.rs/sqlx/latest/sqlx/struct.Transaction.html | Atomic batch insert |
| HTTP 422 Unprocessable Entity | RFC 9110 §15.5.21 | Batch validation failure response |
| OWASP: don't confirm existence on 404 | https://owasp.org/www-project-web-security-testing-guide/ | Cross-workspace 404 vs 403 |

## Sprint N+4 — Phase 29: Mobile Dashboard + Docker Production + QR Code (2026-03-27)

### Goals
- Mobile-responsive dashboard (hamburger, sidebar, table scroll wrappers)
- Production-ready multi-stage Dockerfile with dashboard build stage
- Real QR Code Model 2 SVG generator (ISO/IEC 18004:2015)

### Completed
| Task | Crate/File | Status | Tests | Notes |
|------|-----------|--------|-------|-------|
| Hamburger menu + collapsible sidebar | dashboard/+layout.svelte | DONE | — | CSS slide-in, overlay backdrop |
| table-wrap on all pages with `<table>` | dashboard/routes/* (11 pages) | DONE | — | overflow-x: auto |
| withdraw form-grid single column on mobile | dashboard/withdraw | DONE | — | grid-template-columns: 1fr |
| Multi-stage Dockerfile (node + rust + runtime) | Dockerfile | DONE | — | dashboard build in stage 1 |
| Network-isolated docker-compose | docker-compose.yml | DONE | — | frontend/backend networks, signer isolated |
| QR Code Model 2 ISO 18004:2015 SVG generator | crates/common/src/qr_svg.rs | DONE | 10 | GF(256) RS, 8 masks, BCH format info |
| Wire QR generator into deposit_address_qr handler | crates/gateway/src/user_routes.rs | DONE | — | fallback to text SVG on error |

### Metrics
- **New tests added**: 10 (all in `custody-common::qr_svg`)
- **Tests passing**: 1070 (custody-common: 952, custody-gateway: 118)
- **New files**: 1 (`qr_svg.rs`)
- **Modified files**: 14 (11 dashboard pages + layout, Dockerfile, docker-compose.yml, user_routes.rs, lib.rs)
- **Dashboard build**: passes (`npm run build` succeeds)
- **Clippy**: zero new warnings in all added/modified files

### References Used
| Reference | URL | Used For |
|-----------|-----|----------|
| ISO/IEC 18004:2015 | standard | QR Code module placement, masking, format info |
| thonky.com QR tutorial | https://www.thonky.com/qr-code-tutorial/ | Capacity tables, RS encoding, BCH format |
| BIP-21 URI scheme | https://github.com/bitcoin/bips/blob/master/bip-0021.mediawiki | Bitcoin QR URI prefix |
| EIP-681 URI scheme | https://eips.ethereum.org/EIPS/eip-681 | Ethereum QR URI prefix |

## Sprint N+3 — Phase 28: TOTP 2FA + Proof of Reserves User Verification (2026-03-28)

### Goals
- Implement RFC 6238 TOTP 2FA with a two-step login flow (password → pending token → TOTP → session)
- Add user-facing Proof of Reserves inclusion proof endpoint

### Completed
| Task | Crate | Status | Tests | Notes |
|------|-------|--------|-------|-------|
| `TotpSecret` (RFC 6238, HMAC-SHA1, Zeroize) | common | DONE | 13 | RFC 6238 Appendix B vectors pass |
| DB migration 015 (`user_totp`, `pending_2fa_tokens`) | db | DONE | — | AES-GCM encrypted secret, single-use pending tokens |
| TOTP DB layer (encrypt/decrypt, backup codes, lifecycle) | db | DONE | 11 | Wrong-key fails, tamper fails, unique codes |
| TOTP routes (setup/verify/disable/verify-2fa) | gateway | DONE | 2 | Key derivation SHA-256("totp-v1:"\|\|seed) |
| Login handler 2FA branching | gateway | DONE | 2 | `requires_2fa + pending_token` response shape |
| RBAC public path: `/v1/auth/sessions/verify-2fa` | gateway | DONE | — | Pre-auth endpoint |
| `generate_proof(leaves, idx)` in reconciliation | reconciliation | DONE | 5 | Single-leaf, odd-tree, all configs |
| `GET /v1/user/proof-of-reserves` | gateway | DONE | 4 | Per-leaf proofs with `0x` prefixed hashes |
| `POST /v1/user/proof-of-reserves/verify` | gateway | DONE | 3 | Strips `0x`, validates 64-char hex hashes |

### Metrics
- **New tests added**: 40 (13 totp common + 11 totp db + 2 totp routes + 2 auth_routes + 5 reconciliation + 7 gateway por_tests)
- **Tests passing**: 1531 (up from 509 at last session record — large jump includes all crates)
- **New files**: 5 (`totp.rs` common, `totp.rs` db, `totp_routes.rs` gateway, `015_totp.sql`, TOTP module wiring)
- **Modified files**: 6 (`lib.rs` ×3, `routes.rs`, `auth_routes.rs`, `user_routes.rs`, `rbac_middleware.rs`, `proof_of_reserves.rs`)
- **Clippy**: zero warnings in all new/modified files
- **Bug found + fixed**: `generate_proof` for single-leaf trees was computing root inconsistently with `merkle_root` (pre-padded to 2 vs `while len > 1`); fixed to mirror `merkle_root` exactly

### References Used
- RFC 6238 TOTP: https://www.rfc-editor.org/rfc/rfc6238
- RFC 4226 HOTP: https://www.rfc-editor.org/rfc/rfc4226
- RFC 4648 Base32: https://www.rfc-editor.org/rfc/rfc4648
- Google Authenticator Key Uri Format: https://github.com/google/google-authenticator/wiki/Key-Uri-Format
- Vitalik PoR blog post: https://vitalik.eth.limo/general/2022/11/19/proof_of_solvency.html

---

## Sprint N+2 — Phase 27: Gnosis Safe + Scheduled Withdrawals + WebSocket Enhancements (2026-03-27)

### Goals
- Implement Gnosis Safe multi-sig wallet execution engine with EIP-712 hash + signature collection
- Implement scheduled/recurring withdrawals with cron-style scheduling
- Enhance WebSocket feed: auth, new event types (BalanceUpdate, TransactionStatusChange, ApprovalRequired)

### Completed
| Task | Crate | Status | Tests | Notes |
|------|-------|--------|-------|-------|
| `SafeWallet` + `SafeManagedTx` types | common | DONE | 12 | EIP-712 hash, threshold logic, exec calldata |
| `compute_safe_tx_hash()` EIP-712 | common | DONE | 3 | Deterministic, chain-ID separated, no float |
| `SafeOperation` re-export | common | DONE | — | Avoids import path fragmentation |
| `WithdrawalSchedule` + `ScheduledWithdrawal` | common | DONE | 17 | Once/Daily/Weekly/Monthly, advance(), disable |
| Migration 013: `scheduled_withdrawals` | db | DONE | — | TEXT amount, partial index for due schedules |
| Migration 014: `safe_wallets` + `safe_transactions` | db | DONE | — | JSONB owners/signatures, TEXT value |
| DB CRUD for scheduled withdrawals | db | DONE | 7 | encode/decode all schedule types |
| DB CRUD for Safe wallets + transactions | db | DONE | 4 | Status/op roundtrips, case-insensitive lookup |
| Safe API routes (6 endpoints) | gateway | DONE | 4 | Full RBAC: Admin/Editor/Signer/Viewer |
| Scheduled withdrawal API routes (3 endpoints) | gateway | DONE | — | In user_router, all RBAC |
| WebSocket auth (`?token=`) | gateway | DONE | 8 | Session resolution, graceful fallback |
| `WsEvent` new types + helpers | gateway | DONE | 5 | BalanceUpdate, TxStatusChange, ApprovalRequired |
| Shared filter (Arc<Mutex>) | gateway | DONE | — | Fixes borrow-across-spawn |
| Scheduled withdrawal background loop (60s) | server | DONE | — | Due-schedule detection + pipeline submit |
| Module wiring (lib.rs × 3 + routes.rs) | all | DONE | — | Both routers updated |

### Metrics
- **New tests added**: 52 (12 safe_executor + 17 scheduled_withdrawal + 4 safe_wallets_db + 7 scheduled_withdrawals_db + 4 safe_routes + 8 websocket)
- **Tests passing**: 929 custody-common — all green; full suite 0 failures
- **New files**: 6 (`safe_executor.rs`, `scheduled_withdrawal.rs`, `safe_wallets.rs` DB, `scheduled_withdrawals.rs` DB, `safe_routes.rs`, migrations ×2)
- **Modified files**: 5 (`lib.rs` ×3, `routes.rs`, `user_routes.rs`, `websocket.rs`, `main.rs`)
- **Clippy**: zero new warnings in any new file

### References Used
- Gnosis Safe v1.4.1 contract: https://github.com/safe-global/safe-contracts/blob/v1.4.1/contracts/Safe.sol
- EIP-712 Typed Structured Data: https://eips.ethereum.org/EIPS/eip-712
- Safe signature sorting (checkNSignatures): Safe.sol#L249 — owners sorted ascending
- WebSocket token-in-querystring: standard practice for browser WS clients (MDN WebSocket API)

## Sprint N+1 — Phase 26: Notification Dispatch + CSV Export + QR SVG (2026-03-27)

### Goals
- Implement `NotificationDispatcher` in `custody-confirmations` with webhook/email/Slack channels
- Add `GET /v1/user/transactions/export` CSV download endpoint
- Add `GET /v1/user/deposit-address/{chain}/qr` SVG QR placeholder endpoint

### Completed
| Task | Crate | Status | Tests | Notes |
|------|-------|--------|-------|-------|
| `NotificationDispatcher` + 3 channel types | confirmations | DONE | 20 | HMAC-signed webhooks, email stub, Slack |
| `compute_hmac` + backoff + event filtering | confirmations | DONE | 8 | Known test vector against OpenSSL |
| Email subject/body formatting | confirmations | DONE | 7 | Plain-text, nested objects skipped |
| Slack text formatting | confirmations | DONE | 2 | Incoming-webhook JSON format |
| `GET /v1/user/transactions/export` | gateway | DONE | 6 | RFC-4180 CSV, correct Content-Disposition |
| `GET /v1/user/deposit-address/{chain}/qr` | gateway | DONE | 5 | SVG 400x200, XML-safe, r## raw string fix |
| `csv_escape` + `xml_attr_escape` | gateway | DONE | 4 | Edge cases covered |
| Module wiring (lib.rs + router) | confirmations/gateway | DONE | — | |

### Metrics
- **New tests added**: 35 (20 notification_dispatch + 15 user_routes)
- **Tests passing**: 28 custody-confirmations, 80 custody-gateway — all green
- **New files**: 1 (`notification_dispatch.rs`)
- **Modified files**: 3 (`confirmations/lib.rs`, `user_routes.rs`, `CHANGELOG.md`)
- **Clippy**: all warnings are pre-existing in other crates; zero new warnings in changed files

### References Used
- RFC 4180 (CSV format): https://www.ietf.org/rfc/rfc4180.txt
- HMAC-SHA256 test vector: `printf '%s' "POST..." | openssl dgst -sha256 -hmac "secret"`
- Slack Incoming Webhooks: https://api.slack.com/messaging/webhooks
- SVG 1.1 spec: https://www.w3.org/TR/SVG11/

---

## Sprint N — Phase 24 + 25: Swap Engine + OpenAPI 3.1 (2026-03-27)

### Goals
- Implement swap execution engine (UniswapV3 + Jupiter) with trait abstraction
- Implement OpenAPI 3.1 spec endpoint with full schemas

### Completed
| Task | Crate | Status | Tests | Notes |
|------|-------|--------|-------|-------|
| `SwapExecutor` trait | common | DONE | — | quote/execute/status |
| `UniswapV3Executor` | common | DONE | 12 | 228-byte calldata verified |
| `JupiterExecutor` | common | DONE | 7 | async API methods + sync fallback |
| Swap types (Quote, Result, Execution) | common | DONE | 10 | Full serde round-trip |
| `swap_routes.rs` (3 endpoints) | gateway | DONE | 13 | POST quote, POST execute, GET status |
| `openapi.rs` — OpenAPI 3.1 generator | gateway | DONE | 22 | Full schemas + security schemes |
| Router wiring | gateway | DONE | — | swap_router merged, openapi handler updated |

### Metrics
- **New tests added**: 64 (29 swap_executor + 13 swap_routes + 22 openapi)
- **Total tests**: common 900, gateway 55 — all passing
- **New files**: 3 (`swap_executor.rs`, `swap_routes.rs`, `openapi.rs`)
- **Clippy**: zero warnings in new files

### References Used
- Uniswap V3 `exactInputSingle` ABI: https://docs.uniswap.org/contracts/v3/reference/periphery/SwapRouter
- Uniswap V3 SwapRouter02 address: 0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45
- Jupiter V6 API: https://quote-api.jup.ag/v6
- OpenAPI 3.1.0 spec: https://spec.openapis.org/oas/v3.1.0

---

## Sprint 1 — Foundation (2026-03-24)

### Goals
- Project structure, CI, core types, API skeleton

### Completed
| Task | Crate | Status | Tests | Notes |
|------|-------|--------|-------|-------|
| Cargo workspace (14 crates) | all | DONE | - | Compiles clean |
| Core types (Chain, Asset, Deposit, Withdrawal) | common | DONE | 5 | Serialization verified |
| Error types (CustodyError) | common | DONE | - | 12 error variants |
| DB stubs (addresses, deposits, withdrawals) | db | DONE | - | sqlx, async, ready for schema |
| REST API skeleton (5 endpoints) | gateway | DONE | - | Returns 501, HMAC auth ready |
| HMAC-SHA256 auth | gateway | DONE | 2 | Verify + reject tests |
| Config management | server | DONE | - | figment + TOML |
| Binary entry point | server | DONE | - | Starts on :8443 |
| Agent setup | .claude | DONE | - | 6 agents, 6 commands |

### Metrics
- **Crates**: 14
- **Tests**: 7 passing
- **Lines of Rust**: ~1200
- **Compilation**: cargo check clean, cargo test green

### References Used
- Exchange CustodyProvider trait: `~/Projects/exchange/crates/custody/src/provider.rs`
- Research doc: `~/Projects/exchange/docs/roadmaps/self-hosted-custody-research.md`
- BIP-44: https://github.com/satoshilabs/slips/blob/master/slip-0044.md

---

## Sprint 2 — Key Management: Address Derivation (2026-03-24)

### Goals
- HD wallet derivation for all 4 chains
- Address generation with test vector verification
- Idempotent address allocator

### Completed
| Task | Crate | Status | Tests | Notes |
|------|-------|--------|-------|-------|
| BIP-32/BIP-44 HD key derivation | address-factory/hd.rs | DONE | 7 | SeedBytes with Zeroize, secp256k1 pubkey extraction |
| SLIP-0010 Ed25519 derivation | address-factory/slip0010.rs | DONE | 7 | Manual HMAC-SHA512, verified against SLIP-0010 spec TV1 |
| Bitcoin P2WPKH Bech32 encoding | address-factory/bitcoin.rs | DONE | 5 | BIP-84 m/84'/0'/0'/0/{i}, verified: bc1qcr8te4kr609gc... |
| Ethereum EIP-55 encoding | address-factory/ethereum.rs | DONE | 5 | Keccak-256, verified: 0x9858EfFD232B4033E47d... |
| Tron Base58Check encoding | address-factory/tron.rs | DONE | 5 | 0x41 prefix, roundtrip verified, shares hash with ETH |
| Solana Base58 encoding | address-factory/solana.rs | DONE | 4 | Ed25519 pubkey, roundtrip verified |
| Address allocator | address-factory/allocator.rs | DONE | 6 | Deterministic, unique per index, all 4 chains |
| DB next_address_index stub | db/addresses.rs | DONE | - | Atomic counter stub ready for SQL |

### Completed (cont.)
| Task | Crate | Status | Tests | Notes |
|------|-------|--------|-------|-------|
| AES-256-GCM encrypted seed storage | signing/encrypted_store.rs | DONE | 7 | PBKDF2-HMAC-SHA256 600K iterations, wire format v1 |
| Shamir 3-of-5 secret sharing | signing/shamir.rs | DONE | 12 | GF(256), all 10 3-of-5 combos verified |
| SoftwareSigningBackend | signing/software.rs | DONE | 5 | Dev/test backend, Zeroize on drop |
| HsmSigningBackend stub | signing/software.rs | DONE | - | PKCS#11 placeholder, returns unimplemented |
| Audit log module | db/audit.rs | DONE | 4 | AuditEntry, 8 operation types, hash chain design |

### Metrics
- **Crates modified**: 3 (address-factory, signing, db)
- **New modules**: 11 (hd, slip0010, bitcoin, ethereum, tron, solana, allocator, encrypted_store, shamir, software, audit)
- **Tests**: 76 passing (69 new + 7 existing)
- **Compilation**: cargo check clean, cargo test all green

### References Used
- BIP-32: https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki
- BIP-84: https://github.com/bitcoin/bips/blob/master/bip-0084.mediawiki
- BIP-173: https://github.com/bitcoin/bips/blob/master/bip-0173.mediawiki
- EIP-55: https://eips.ethereum.org/EIPS/eip-55
- SLIP-0010: https://github.com/satoshilabs/slips/blob/master/slip-0010.md
- Tron Account Docs: https://developers.tron.network/docs/account
- Shamir 1979: https://dl.acm.org/doi/10.1145/359168.359176
- OWASP PBKDF2: https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html

---

## Sprint 3 — Ethereum MVP (2026-03-25)

### Goals
- End-to-end Ethereum deposit detection and withdrawal processing
- EIP-1559 fee estimation, signing, broadcasting
- Gateway wiring + server orchestration

### Completed
| Task | Crate | Status | Tests | Notes |
|------|-------|--------|-------|-------|
| Config types (CustodyConfig) | common/config.rs | DONE | 1 | figment TOML + env, all chain configs parsed |
| DB scan_progress | db/scan_progress.rs | DONE | - | get/set last scanned block per chain |
| DB nonces | db/nonces.rs | DONE | - | get/set/increment persistent nonce counter |
| DB webhook_deliveries | db/webhook_deliveries.rs | DONE | 1 | Exponential backoff (2^n, max 300s) |
| DB custody_ledger | db/custody_ledger.rs | DONE | 1 | Double-entry debit/credit pairs |
| EIP-1559 fee estimator | fee-estimator/ | DONE | 6 | wei↔gwei conversion, gas cap check |
| Ethereum scanner | scanner/ethereum.rs | DONE | 4 | ETH native + ERC-20 Transfer logs |
| uint256↔Decimal conversion | scanner/lib.rs | DONE | 11 | ETH(18), USDT(6), USDC(6) tested |
| Address cache | scanner/address_cache.rs | DONE | 1 | DashMap, case-insensitive lookup |
| Confirmation tracker | confirmations/tracker.rs | DONE | 4 | Generic, works for any chain |
| Webhook sender | confirmations/webhook.rs | DONE | 4 | HMAC-SHA256, exponential backoff |
| ETH tx signing | signing/ethereum.rs | DONE | 6 | EIP-1559 type 2, BIP-32 privkey, zeroize |
| ERC-20 calldata encoding | signing/ethereum.rs | DONE | - | 0xa9059cbb selector, address+amount ABI |
| Fee bump logic | broadcaster/lib.rs | DONE | 2 | 12.5% minimum replacement |
| Ethereum broadcaster | broadcaster/ethereum.rs | DONE | - | eth_sendRawTransaction + receipt poll |
| Nonce manager | withdrawal-pipeline/nonce_manager.rs | DONE | - | Per-address Mutex, chain sync |
| Address validation | withdrawal-pipeline/validation.rs | DONE | 7 | ETH/BTC/TRX/SOL format + circuit breaker |
| Pipeline state machine | withdrawal-pipeline/pipeline.rs | DONE | 1 | Received → Validated (full pipeline in progress) |
| Gateway wiring | gateway/routes.rs + state.rs | DONE | - | All 5 endpoints wired to services |
| Server orchestration | server/main.rs | DONE | - | Config, DB, background tasks, HTTP server |

### Metrics
- **Crates modified**: 9 (common, db, fee-estimator, scanner, confirmations, signing, broadcaster, withdrawal-pipeline, gateway, server)
- **New modules**: 17
- **Tests**: 126 passing (50 new + 76 from Phase 1)
- **alloy upgrade**: 0.12 → 1.x
- **Compilation**: cargo check clean, cargo test all green

### References Used
- EIP-1559: https://eips.ethereum.org/EIPS/eip-1559
- ERC-20 Transfer: keccak256("Transfer(address,address,uint256)")
- EIP-2718: https://eips.ethereum.org/EIPS/eip-2718
- alloy 1.x: https://docs.rs/alloy/latest

---

## Research — Institutional Custody Gap Analysis (2026-03-25)

### Goals
- Research top 5 institutional custody providers (Fireblocks, BitGo, Copper.co, Cobo, Liminal)
- Create comprehensive feature comparison matrix
- Identify and prioritize gaps in MG Custody vs. industry standard

### Completed
| Task | Output | Status | Notes |
|------|--------|--------|-------|
| Fireblocks research | Feature matrix | DONE | 150 chains, MPC-CMP, SGX policy engine, $1B insurance |
| BitGo research | Feature matrix | DONE | 700+ tokens, OCC bank charter, multi-sig+MPC, $250M-$1B insurance |
| Copper.co research | Feature matrix | DONE | ClearLoop settlement, 2-of-3 MPC, Notabene Travel Rule |
| Cobo research | Feature matrix | DONE | 80+ chains, 3,000+ tokens, 4 wallet types, SOC 2 + ISO 27001 |
| Liminal research | Feature matrix | DONE | 21+ chains, CCSS Level 3, HSM Vaults (2026), TRM Labs AML |
| Gap analysis | docs/gap-analysis-custody-providers.md | DONE | 6 P0, 8 P1, 8 P2 gaps identified |
| Priority recommendations | Embedded in analysis | DONE | MPC + HSM + AML as non-negotiable P0 |

### Key Findings
- **P0-1 MPC signing** is the single most critical gap — all 5 providers use MPC or multi-sig
- **P0-3 AML/KYT** is a legal requirement, not a feature choice
- Self-hosted advantage: no vendor lock-in, data sovereignty, cost at scale
- Self-hosted disadvantage: no settlement network, no insurance, limited chain breadth
- Hybrid model (self-hosted hot + provider cold) is the recommended middle path

### References Used
- Fireblocks developer docs: https://developers.fireblocks.com/
- BitGo blog + developer portal: https://www.bitgo.com/ + https://developers.bitgo.com/
- Copper.co product pages: https://copper.co/en/products/clearloop
- Cobo developer hub: https://www.cobo.com/developers/
- Liminal custody site: https://www.liminalcustody.com/

---

## Level 5 — Fireblocks Functional Parity (2026-03-25)

### Goals
- Vault/account abstraction (Fireblocks core concept)
- Internal transfers, unified transaction history
- Raw message signing, fee estimation, asset management
- Transaction cancel/accelerate

### Completed
| Task | Crate | Status | Tests | Notes |
|------|-------|--------|-------|-------|
| Vault + VaultAccount types | common/vault.rs | DONE | 12 | Vault, VaultAccount, InternalTransfer, Transaction, FeeLevel, ManagedAsset |
| DB migration 003_vaults.sql | db/migrations | DONE | - | 4 tables: vaults, vault_accounts, internal_transfers, managed_assets |
| Vault CRUD persistence | db/vaults.rs | DONE | - | insert/find/list/update/hide + vault_has_balance check |
| VaultAccount CRUD | db/vaults.rs | DONE | - | insert/find/list + balance update + freeze |
| Internal transfer persistence | db/vaults.rs | DONE | - | insert/find/complete + idempotency key |
| Asset management persistence | db/assets.rs | DONE | - | list/find/insert/update/delete + case-insensitive |
| Transaction history queries | db/transactions.rs | DONE | - | UNION over deposits/withdrawals/transfers, cursor pagination |
| Ledger: internal transfers | db/custody_ledger.rs | DONE | - | record_internal_transfer with vault-scoped accounts |
| Raw secp256k1 signing | signing/raw.rs | DONE | 12 | ETH/BTC/TRX sign + verify, EIP-191, keccak256 |
| Raw Ed25519 signing | signing/raw.rs | DONE | - | SOL sign + verify, SLIP-0010 derivation |
| Vault API routes | gateway/vault_routes.rs | DONE | - | 20+ endpoints merged into main router |
| Fee estimation API | gateway/vault_routes.rs | DONE | - | 4-chain low/medium/high levels |
| Cancel/accelerate API | gateway/vault_routes.rs | DONE | - | BTC/ETH only, multiplier validation |

### Metrics
- **New files**: 7 (vault.rs, vaults.rs, assets.rs, transactions.rs, raw.rs, vault_routes.rs, 003_vaults.sql)
- **Modified files**: 6 (common/lib.rs, db/lib.rs, signing/lib.rs, gateway/lib.rs, gateway/routes.rs, custody_ledger.rs)
- **Tests**: 533 passing (24 new + 509 existing)
- **New API endpoints**: 20+
- **Lines of Rust**: ~27K
- **Compilation**: cargo check clean, cargo test all green

### References Used
- Fireblocks API: https://developers.fireblocks.com/reference/api-overview
- EIP-191: https://eips.ethereum.org/EIPS/eip-191
- EIP-712: https://eips.ethereum.org/EIPS/eip-712
- BIP-125 (RBF): https://github.com/bitcoin/bips/blob/master/bip-0125.mediawiki

---

## Levels 6-16 — Full Feature Build (2026-03-26)

### Goals
- Close ALL remaining Fireblocks functional gaps
- API surface parity: 125+ endpoints
- Deep implementation of all "types only" phases

### Completed (Levels 6-16)

| Level | Features | New Files | Tests Added |
|-------|----------|-----------|-------------|
| 6 | Webhook management, batch tx, audit log API, address book, gas station status | 6 | +7 |
| 7 | Exchange connections, API keys (SHA-256), notifications (Telegram/Slack/PagerDuty), system config, network fees | 7 | +13 |
| 8 | Workspace management, reporting/export (JSON/CSV), signing key info, travel rule (FATF R.16) | 6 | +11 |
| 9 | Vault policies (spending limits, velocity, time locks, freeze), approval workflows (multi-sig quorum), supported networks, tx notes, staking execution | 5 | +11 |
| 10 | Transaction scheduling, compliance screening, backup/recovery status, token discovery, health dashboard | 5 | +9 |
| 11 | OpenAPI 3.0 spec (auto-generated), SDK method definitions, endpoint catalog | 1 | +8 |
| 12 | Multi-chain address validation (Bech32/hex/Base58Check), error catalog (30+ codes), CLI tools (key ceremony, test fixtures) | 2 | +27 |
| 13 | Idempotency records, webhook HMAC signing, per-chain retry policies, circuit breaker v2, request analytics | 1 | +16 |
| 14 | UTXO analysis (dust detection, consolidation), EVM gas utilities (EIP-1559), deposit forwarding, tx categories, multi-sig wallets | 1 | +14 |
| 15 | Ledger accounting (balance sheet, P&L, fee tracking), event catalog (31 types), token bucket rate limiter, key fingerprints | 1 | +13 |
| 16 | DeFi execution (swap routing, slippage), settlement protocol (bilateral netting), tokenization (mint/burn calldata), FIDO2/WebAuthn, metrics wiring (35+), load test scenarios | 5 | +33 |
| 17 | Token database (55+ verified tokens), Chainlink/Pyth price feeds, NFT custody (ERC-721/1155/Metaplex) | 3 | +33 |
| 18 | Uniswap V3/Curve calldata, ERC-20 approve/allowance, cross-chain bridges (6 routes, 5 protocols) | 1 | +12 |
| 19 | Lido/Rocket Pool/Marinade staking calldata, WalletConnect v2 protocol, Cosmos SDK (5 chains + IBC) | 3 | +25 |
| 20 | Polkadot/Substrate (5 chains), Jupiter DEX (Solana), token auto-scan (EVM/SOL/TRX) | 3 | +24 |
| 21 | ERC-4337 Account Abstraction, Bitcoin Taproot (P2TR), MEV protection (Flashbots/Jito), Solana compute budget | 3 | +23 |
| 22 | EIP-712 typed data hashing, Solana Token-2022 extensions, cross-chain messaging (Wormhole/LZ/CCIP) | 3 | +19 |
| 23 | Multicall3 batching, ERC-20 Permit (EIP-2612), Aave flash loans, gas savings estimator | 1 | +10 |
| 24 | ENS namehash + name resolution, Safe{Wallet} multi-sig, ERC-4626 yield vaults | 3 | +21 |
| 25 | ABI decoder (26 selectors, 7 events), Tron Energy/bandwidth estimation | 2 | +19 |

### Session Metrics (Final)
- **Levels completed**: 21 (Level 5 through Level 25)
- **Lines of Rust**: 40,974
- **Rust files**: 215+
- **Tests**: 881 passing, 0 failures
- **API endpoints**: 125+
- **DB tables**: 30 (across 8 migrations)
- **Common modules**: 69 (lib.rs exports)
- **Commits this session**: 22
- **Total commits**: 89

### Architecture Summary
```
crates/
  common/     69 modules — types, DeFi, NFT, bridges, staking, MEV, ABI, pricing, governance
  db/         18 modules — PostgreSQL persistence, 30 tables, 8 migrations
  gateway/    15 modules — 12 route groups, 125+ REST endpoints
  scanner/    6 modules  — 4-chain deposit detection (BTC/ETH/TRX/SOL + EVM L2)
  signing/    9 modules  — BIP-143, EIP-1559, SLIP-0010, raw signing, staking
  mpc/        10 modules — CGGMP21 (secp256k1) + FROST (Ed25519), HSM
  broadcaster/5 modules  — 4-chain tx broadcast + confirmation tracking
  pipeline/   5 modules  — withdrawal state machine (Received → Finalized)
  + 9 more crates (address-factory, policy, compliance, wallet-manager, etc.)
```

### Chain & Protocol Coverage
| Category | Count | Details |
|----------|-------|---------|
| Native chains | 4 | Bitcoin, Ethereum, Tron, Solana |
| EVM L2s | 9 | Arbitrum, Optimism, Base, Polygon, BSC, Avalanche, zkSync, Linea, Scroll |
| Cosmos chains | 5 | Hub, Osmosis, Celestia, dYdX, Injective |
| Polkadot chains | 5 | DOT, Kusama, Moonbeam, Acala, Astar |
| DEXes | 6 | Uniswap V3, Curve, SushiSwap, Jupiter, Raydium, SunSwap |
| Bridges | 5 | Native L2, Wormhole, LayerZero, Across, Stargate |
| Staking protocols | 5 | Lido, Rocket Pool, Marinade, Jito, Coinbase |
| Tokens | 55+ | Verified with contract addresses and CoinGecko IDs |

### References Used
- Fireblocks API: https://developers.fireblocks.com/reference/api-overview
- Uniswap V3: https://docs.uniswap.org/contracts/v3/overview
- ERC-20 selectors: https://www.4byte.directory/
- FIDO2/WebAuthn: https://www.w3.org/TR/webauthn-2/
- FATF Recommendation 16: https://www.fatf-gafi.org/en/topics/fatf-recommendations.html
- BIP-67 (sorted multi-sig): https://github.com/bitcoin/bips/blob/master/bip-0067.mediawiki
- Gnosis Safe: https://docs.safe.global/
- ENS: https://docs.ens.domains/
- ERC-4337: https://eips.ethereum.org/EIPS/eip-4337
- ERC-4626: https://eips.ethereum.org/EIPS/eip-4626
- EIP-2612 Permit: https://eips.ethereum.org/EIPS/eip-2612
- Jupiter V6: https://station.jup.ag/docs
- Chainlink feeds: https://data.chain.link/
- Pyth Network: https://pyth.network/
- Wormhole: https://docs.wormhole.com/
- LayerZero: https://layerzero.gitbook.io/
- Chainlink CCIP: https://docs.chain.link/ccip
- Solana Token-2022: https://spl.solana.com/token-2022
- Aave V3: https://docs.aave.com/

---

## Trust Pivot — PM Analysis Sprint (2026-03-26)

### Goals
- External review analysis: identify trust gaps blocking adoption
- Honest status audit of all ROADMAP phases
- Define Phase 17 (Trust Infrastructure) as next priority

### Completed
| Task | Output | Status | Notes |
|------|--------|--------|-------|
| PM gap analysis | Full report | DONE | 9 gaps identified (signer isolation, circuit breaker, HSM, MPC transport, docs) |
| ROADMAP honest statuses | ROADMAP.md | DONE | Phases 4C/9/10/11/13/14/15 re-marked as types-only |
| Phase 17 definition | ROADMAP.md | DONE | 7 sub-phases: 17A-17G, P0/P1/P2 prioritized |
| Feature freeze | ROADMAP.md | DONE | No new features until Phase 17 complete |
| Honest Status Tracker | ROADMAP.md | DONE | Replaced misleading parity tracker with 4-tier honest status |
| CHANGELOG update | CHANGELOG.md | DONE | Trust pivot entry added |

### Key Findings
- **P0 fund safety bug**: Circuit breaker counters reset on process restart ($10M limit bypass)
- **P0 false claim**: Signer isolation (Zone 3) doesn't exist — single binary, single process
- **P0 false claim**: HSM marked "Done" but returns `unimplemented!()`
- **P0 false claim**: MPC marked "Done" but has no network transport between parties
- **Strategic pivot**: Stop adding features, start adding trust artifacts

### Decision
**Feature freeze effective immediately.** Next work = Phase 17A (THREAT_MODEL.md + SECURITY.md), then 17C (circuit breaker persistence), then 17B (signer isolation).

---

## Phase 17 Completion Sprint (2026-03-27)

### Goals
- Close remaining Phase 17 items (17C test + 17G onboarding)
- Phase 17 fully complete → unblock Phase 16

### Completed
| Task | Output | Status | Notes |
|------|--------|--------|-------|
| Circuit breaker restart persistence test | crates/db/tests/integration.rs | DONE | 9-step test: record $5M, drop connection, reconnect, verify counters survived, verify tripped state survives |
| Onboarding walkthrough | docs/ONBOARDING.md | DONE | 2 archetypes: crypto startup (testnet setup, deposit, withdrawal, webhooks) + trading desk (vaults, approvals, raw signing, circuit breaker) |
| ROADMAP/CHANGELOG/SPRINTS update | All 3 docs | DONE | Phase 17 fully checked off |

### Metrics
- **Phase 17 status**: COMPLETE (all items checked)
- **New test**: 1 integration test (circuit breaker restart persistence)
- **New doc**: docs/ONBOARDING.md (~250 lines)
- **Phase 16 (Security Certifications)**: Now unblocked

### References Used
- Fireblocks onboarding: https://developers.fireblocks.com/docs/quickstart
- BitGo getting started: https://developers.bitgo.com/guides/get-started

---

## Phase 18 — Competitive Gap Closure (2026-03-27)

### Goals
- Close remaining P0/P1 gaps from gap analysis
- Lift feature freeze (Phase 17 complete)

### Completed
| Task | Crate | Status | Tests | Notes |
|------|-------|--------|-------|-------|
| HsmSigningBackend (18A) | signing | DONE | +5 | Encrypted seed, on-demand decrypt→sign→zeroize, parity verified |
| Generalized EVM Scanner (18B) | scanner | DONE | +6 | from_chain_def(), native + ERC-20, registry factory |
| Multi-chain fees (18C) | fee-estimator | ALREADY DONE | 0 | BTC/TRX/SOL estimators existed |
| MPC HTTP Transport (18D) | mpc | DONE | +8 | HttpTransport, HMAC auth, deliver/receive, multi-party |
| Feature freeze lifted | ROADMAP | DONE | - | Phase 17 complete, gap closure in progress |

### Metrics
- **New tests**: 19 (HSM: 5, EVM scanner: 6, HTTP transport: 8)
- **New files**: 2 (http_transport.rs, ONBOARDING.md)
- **Gaps closed**: P0-2 (HSM), P1-1 (EVM L2 scanners), P0-1 transport
- **Feature freeze**: LIFTED

### References Used
- PKCS#11 spec: https://docs.oasis-open.org/pkcs11/pkcs11-base/v2.40/pkcs11-base-v2.40.html
- EVM chain IDs: https://chainlist.org/

---

## Product Gap Assessment & ROADMAP Expansion (2026-03-27)

### Goals
- Honest assessment: what do we have vs Fireblocks?
- Identify all unplanned product gaps
- Add Phases 19-25 to ROADMAP with technology choices and security rationale

### Completed
| Task | Output | Status | Notes |
|------|--------|--------|-------|
| Fireblocks API feature comparison | Gap analysis | DONE | 20 feature areas compared |
| Honest assessment of codebase | Status report | DONE | 8 real, 9 types-only, 3 missing |
| Token audit | Report | DONE | 100+ tokens with verified addresses across 4 chains |
| Phase 19 planning (RBAC) | ROADMAP.md | DONE | axum middleware, PostgreSQL sessions, default-deny |
| Phase 20 planning (FIDO2) | ROADMAP.md | DONE | webauthn-rs, registration/auth flows |
| Phase 21 planning (Dashboard) | ROADMAP.md | DONE | SvelteKit + adapter-static, 9 pages, air-gap compatible |
| Phase 22 planning (AML) | ROADMAP.md | DONE | Chainalysis KYT v2, Elliptic Lens, cache + fallback |
| Phase 23 planning (UTXO) | ROADMAP.md | DONE | Consolidation scheduler, fee gate, circuit breaker bypass |
| Phase 24 planning (Swaps) | ROADMAP.md | DONE | UniswapV3Executor, JupiterExecutor, MEV protection |
| Phase 25 planning (OpenAPI) | ROADMAP.md | DONE | utoipa, auto-generation from handlers |
| Dependency graph | ROADMAP.md | DONE | 19→20→21A→[22+23]→21B/C/D→24→25 |
| Summary table update | ROADMAP.md | DONE | "Product Gaps" section added |

### Key Findings
- **#1 adoption blocker**: No Web UI — operators cannot use the system
- **#2 security hole**: RBAC types exist but nothing is enforced — any API caller is god-mode
- **#3 regulatory blocker**: AML/KYT stubs only — cannot operate in regulated jurisdictions
- **Technology decision**: SvelteKit for dashboard (static output, no Node.js in prod, air-gap compatible, ~40KB bundle)
- **Overall product completeness vs Fireblocks**: ~30-40%

### Metrics
- **New ROADMAP phases**: 7 (Phases 19-25)
- **New subtasks planned**: 80+
- **Estimated total effort**: 21-30 weeks
- **New DB migrations planned**: 3 (011, 012, 013)
- **New API endpoints planned**: 12+
- **Tests**: 1234 passing (unchanged — planning only)
- **Lines of Rust**: 41K (unchanged)

### References Used
- Fireblocks API: https://developers.fireblocks.com/reference/api-overview
- webauthn-rs: https://github.com/kanidm/webauthn-rs
- SvelteKit adapter-static: https://svelte.dev/docs/kit/adapter-static
- utoipa: https://github.com/juhaku/utoipa
- Chainalysis KYT: https://docs.chainalysis.com/
- Elliptic Lens: https://www.elliptic.co/solutions/lens
- Jupiter v6: https://station.jup.ag/docs
