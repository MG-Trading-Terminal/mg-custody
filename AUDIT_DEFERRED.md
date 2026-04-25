# Audit Pass — Deferred Items

> Things found during the 2026-04-15 → 2026-04-25 honesty-audit pass
> that were intentionally left for follow-up. Each entry is
> self-contained: what, why, where, and a suggested approach so a
> future session can pick it up cold.
>
> Status anchor: the audit pass landed 30+ commits between
> `f0a2be2` (constant-time HMAC, Apr 15) and `8cabd76` (audit log
> serialization, Apr 24). See `git log --oneline 76d21f7..HEAD` for
> the full set.

---

## Priority 1 — Real functionality not yet implemented (refused at the boundary today)

### Bitcoin legacy P2PKH / P2SH signing

- **Where:** `crates/withdrawal-pipeline/src/validation.rs::validate_btc_address`
  (refusal added in commit `e149ac6`).
- **Today:** Validation refuses anything that isn't bech32. The
  signer in `crates/signing/src/bitcoin.rs` only emits P2WPKH
  outputs, so a legacy destination would silently divert to the
  custody change script.
- **What's needed:** Decide if legacy support matters. If yes:
  build the P2PKH / P2SH scriptPubKey path in `sign_btc` and lift
  the validation gate.

### Solana / Tron gas station balance check

- **Where:** `crates/wallet-manager/src/gas_station_loop.rs::tick`
  (lines ~118-143).
- **Today:** Only Ethereum addresses get balance-checked. Tron and
  Solana branches log a WARN saying "balance check NOT wired".
- **What's needed:** Add `TrxBalanceProvider` and `SolBalanceProvider`
  modeled after `custody_reconciliation::eth_balance::EthBalanceProvider`.
  Tron via `/wallet/getaccount` (returns balance in sun + frozen
  resources). Solana via `getBalance` RPC.

### DKG instead of trusted-dealer keygen

- **Where:** `crates/mpc/src/frost/keygen.rs::trusted_dealer_keygen`,
  `crates/mpc/src/ecdsa/keygen.rs::trusted_dealer_keygen`.
- **Today:** Both ceremonies use a centralised dealer that sees
  the full key before splitting — defeats the threshold property.
  `crates/mpc/src/distributed_dkg.rs` exists with the right API
  shape but isn't wired to a runnable ceremony binary.
- **What's needed:** Stand up a multi-process DKG runner that uses
  `coordinator_v2` + `transport` to coordinate FROST DKG and
  CGGMP21 keygen across separate hosts. Replace
  `run_frost_ceremony` with the distributed variant.

---

## Priority 2 — Audit-grade follow-ups (single-commit each)

### Webhook retry: `SELECT FOR UPDATE SKIP LOCKED`

- **Where:** `crates/db/src/webhook_deliveries.rs::pending_webhook_deliveries`.
- **Today:** Multiple retry workers can pick the same row and
  deliver twice. At-least-once semantics make this "OK" if the
  exchange is idempotent, but it's wasteful + clearly noted in the
  earlier audit.
- **What's needed:** Add `FOR UPDATE SKIP LOCKED` to the SELECT
  inside a transaction; commit after the per-row UPDATE
  (`mark_webhook_delivered` or `record_webhook_failure`).

### Tracker / ledger UNIQUE constraint to close double-credit window

- **Where:** `crates/db/src/custody_ledger.rs::credit_deposit`,
  `crates/confirmations/src/tracker.rs::check_deposits` (commit
  `5e9775a` left this as a TODO).
- **Today:** If `credit_deposit` succeeds but the subsequent
  `update_deposit_status` fails, the next tracker tick re-credits.
- **What's needed:** Migration adding
  `CREATE UNIQUE INDEX ON custody_ledger (reference_id,
  reference_type, entry_type) WHERE reference_type IN
  ('deposit', 'deposit_credit')`. The existing schema reuses
  `reference_id` across multiple pairs per deposit, so the index
  has to include `entry_type` to not false-trip. Then handle the
  duplicate-key error in `credit_deposit` as "already credited,
  proceed with status update".

### DB row deserialization — port `parse_str` → `try_parse_str`

- **Where:** ~14 sites still using `Chain::parse_str` (after
  commit `4ba7e4c` made it WARN-loud):
  - `crates/db/src/scheduled_tx.rs:98`
  - `crates/db/src/assets.rs:111`
  - `crates/db/src/embedded_wallets.rs:78`
  - `crates/db/src/connections.rs:122`
  - `crates/db/src/deployed_contracts.rs:32, 49`
  - `crates/db/src/audit.rs:349`
  - `crates/db/src/vaults.rs:364, 403`
  - `crates/db/src/transactions.rs:91, 112, 133, 165, 192, 219`
- **Today:** A bad chain string in a DB row produces an ERROR log
  but loads as `Chain::Ethereum`. UI/API silently presents the
  wrong chain.
- **What's needed:** Each call site returns a structured error
  when the row's chain string isn't recognised, instead of
  coercing to Ethereum. The handler that calls into these readers
  surfaces a 500 with a clear message naming the row id.

### `readiness_http` integration tests

- **Where:** `crates/server/tests/readiness_http.rs` —
  five tests marked `#[ignore]` in commit `e55c855`:
  - `health_features_uptime_rejects_unknown_window`
  - `health_features_refresh_requires_admin_body_is_202`
  - `chaos_override_rejects_invalid_state`
  - `chaos_override_round_trip`
  - `ack_endpoint_round_trip`
- **Today:** They were exercising admin handlers via the rbac-auth
  fallback that the HmacVerified gate closed.
- **What's needed:** Port to `router_with_middleware(state,
  hmac_secret)` with each request signed via
  `compute_hmac(method + path + ts + body)`. Or seed an admin
  session row in the test DB and attach the cookie.

### Reconciliation: tier-aware compare

- **Where:** `crates/reconciliation/src/service.rs::reconcile_ethereum`
  (commit `862a91f` fixed the ledger-key casing but left the
  semantic mismatch).
- **Today:** Sums on-chain balances across all addresses in
  `addresses` (which are user deposit addresses) and compares to
  internal `hot+warm+cold:ETH`. The `addresses` table doesn't
  contain custody's own hot/warm/cold wallet addresses — so the
  on-chain side is "user deposit balance" while the internal side
  is "custody's tiered float". Off by design.
- **What's needed:** Either (a) extend `addresses` to flag the
  custody-owned hot/warm/cold wallet rows and sum only those, or
  (b) add a separate `wallet_addresses` table for the custody
  side and reconcile against that.

### `DestinationType::Internal` mapping

- **Where:** `crates/withdrawal-pipeline/src/pipeline.rs::process_validated`
  (commit `bfc2b99` introduced the lookup but only maps
  Whitelisted/Any).
- **Today:** Internal-scoped policy rules silently no-match
  because the pipeline never returns `Internal` from the
  destination_type lookup.
- **What's needed:** When the destination address belongs to one
  of the custody's own vaults, return `DestinationType::Internal`.
  Requires a `is_address_owned_by_us` query that joins
  `addresses` + `vault_accounts`.

---

## Priority 3 — Hygiene + defensive

### OFAC Base58 case collision

- **Where:** `crates/compliance/src/ofac.rs::SanctionsList::add_address` /
  `contains` (lines 36-43).
- **Today:** Both store and lookup lowercase the input. Bitcoin
  Base58 / Tron Base58Check / Solana Base58 are case-sensitive —
  lowercasing collapses different addresses. In practice OFAC
  publishes canonical-case addresses and users send to canonical
  case, so collisions are rare; theoretical false-positives still
  possible.
- **What's needed:** Detect chain by prefix/length and only
  lowercase Ethereum (where it's case-insensitive by spec) and
  bech32 BTC. Preserve case for Base58 chains.

### `finalize_eth_signed_tx` enforce s-low

- **Where:** `crates/signing/src/ethereum.rs` (line 172).
- **Today:** Accepts any `(r, s, v)` from the backend. Our existing
  backends (PKCS#11, k256-based signer-service) normalise s, but a
  future backend that doesn't would produce mainnet-rejected
  signatures with an unhelpful error.
- **What's needed:** Either add `s < N/2` check with a clear bail,
  or call a normalise helper. Defensive only — no current
  exploit.

### signing/raw — full Zeroizing audit

- **Where:** `crates/signing/src/raw.rs` (commit `94b32b3` covered
  Ed25519 SLIP-0010 + secp256k1 derivation).
- **Today:** The crate has additional sub-modules
  (`bitcoin.rs`, `tron.rs`, `solana.rs`, `ethereum.rs`) that may
  hold derived private-key material on the stack without
  zeroize-on-drop wrappers.
- **What's needed:** Walk every `derive_*` / `sign_*` function in
  the signing crate. Wrap any `[u8; 32]` / `[u8; 64]` that holds
  raw private key bytes in `Zeroizing<_>` so panic-paths clear
  memory.

### Address allocator — wasted HD index on race

- **Where:** `crates/address-factory/src/allocator.rs::get_or_create`
  (lines 88-130).
- **Today:** When two callers race for the same `(user_id, asset,
  chain)`, the loser's allocated HD index is never used. Index
  space is 2^31 per account so exhaustion is centuries away —
  documented as cosmetic in the existing comment, not a bug.
- **What's needed:** Nothing urgent. If HD index density ever
  matters, add a `release_unused_index` path.

### MPC freshness window

- **Where:** `crates/mpc/src/transport.rs::verify_message`
  (commit `f935a2f` added timestamp to the MAC).
- **Today:** Timestamp is MAC-protected against tampering, but no
  caller currently checks `now - msg.timestamp < window`. A
  delayed message would still verify.
- **What's needed:** Add `check_message_fresh(&msg, max_age_secs)`
  helper and call it from `collect_round`. Reject messages older
  than the round's expected lifetime (~30s for FROST signing).

---

## Priority 4 — Documentation owed

### Doc: signer-service capabilities

- `crates/signer-service/README.md` (or module doc) — current IPC
  signer only handles `RawSign` and `GetPublicKey` for ETH+SOL.
  Other operations return Error 5003 / 5005. The pipeline still
  works because most production paths go through the HSM
  (`pkcs11.rs`) or seed paths, not the IPC signer.

### Doc: emergency freeze recovery

- `docs/INCIDENT_RESPONSE.md` — add "freeze hydration on startup"
  procedure: server boots FROZEN if `latest_freeze_state()` query
  errors. Operator must verify DB health and explicitly call
  `POST /v1/admin/emergency/unfreeze` to lift.

---

## How to use this list

In a fresh session:
1. `git log --oneline 76d21f7..HEAD` to see the audit-pass commits.
2. Read `REALITY.md` for the broader "what works" picture.
3. Pick an item from this file, read the referenced commit for
   context, then build → test → commit per CLAUDE.md workflow.
4. When an item lands, delete it from this file in the same PR.
