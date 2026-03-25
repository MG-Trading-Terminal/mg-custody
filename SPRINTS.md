# Sprint Log

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
