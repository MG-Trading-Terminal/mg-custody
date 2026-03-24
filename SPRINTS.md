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

## Sprint 2 — Key Management (TBD)

### Goals
- HD wallet derivation for all 4 chains
- Address generation with test vectors
- Encrypted key storage
- Shamir backup tooling

### Planned
| Task | Crate | Size | Depends |
|------|-------|------|---------|
| BIP32 HD key generation | address-factory | M | - |
| Ethereum address derivation | address-factory | S | HD key |
| Bitcoin P2WPKH derivation | address-factory | S | HD key |
| Tron address derivation | address-factory | S | HD key |
| Solana SLIP-0010 derivation | address-factory | S | HD key |
| Address allocator (idempotent) | address-factory + db | M | All derivations |
| Encrypted seed storage | signing | M | - |
| Shamir 3-of-5 split/restore | signing | M | Encrypted storage |
| SoftHSM2 integration | signing | L | - |
| Audit log for key ops | db | S | - |
