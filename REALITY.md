# Reality Check

> What this repository **actually is** — and is not — as of the latest commit.
> Read before deploying, investing, or trusting funds to this code.

MG Custody is an ambitious, actively developed self-hosted custody service.
The codebase has a lot of infrastructure, a lot of integrations, and a lot
of honest documentation. It is also **a dev/research project that has not
been audited**, and a meaningful portion of what the code models is still
closer to scaffolding than to production muscle.

This file exists so that anyone — you, me, a future contributor, an
enterprising attacker — can quickly tell which parts of the system are
load-bearing and which are not.

---

## What works end-to-end

Each item here has real code, real DB persistence, passes `cargo test`,
and has been exercised via API or unit tests.

| Area | Status | Notes |
|---|---|---|
| Key derivation (BIP-32/44, SLIP-0010) | ✅ | Per-user, per-chain, deterministic |
| Deposit scanning — 4 chains | ✅ | BTC, ETH, TRX, SOL, block-polling |
| Withdrawal state machine | ✅ | Received → Validated → Signed → Broadcast → Confirmed |
| Circuit breaker (DB-persisted) | ✅ | Per-tx / hourly / daily caps, survives restart |
| RBAC middleware + session tokens | ✅ | Viewer/Editor/Signer/Admin/Owner hierarchy |
| TOTP 2FA (setup + login-time verify) | ✅ | RFC 6238, AES-GCM encrypted secrets, backup codes |
| Registration with bootstrap-owner | ✅ | First user becomes Owner; creates default vault |
| Registration IP rate-limit | ✅ | 5 attempts / IP / 15 min |
| Audit log with SHA-256 hash chain | ✅ | Scheduled hourly integrity check |
| Proof of Reserves (user-verifiable) | ✅ | Merkle inclusion, GET + POST verify |
| Dashboard (SvelteKit → static) | ✅ | 23 pages, EN/RU/ZH localized |
| URL-level access guard | ✅ | Viewer hitting /users gets denied card |

---

## What exists as types but is not operational

Code compiles, types are there, unit tests may pass — but there is no
live integration, no real counterparty, no production-grade driver.

| Area | Why it's not live |
|---|---|
| MPC CGGMP21 signing | Crypto + HTTP transport implemented; no multi-host ceremony wired |
| MPC FROST Ed25519 | Same as above |
| HSM (PKCS#11 via `cryptoki`) | File-backed `HsmSigningBackend` works; real HSM integration is stubbed |
| Chainalysis / Elliptic AML | `ScreeningProvider` trait + cache layer; actual HTTP to vendors needs API keys |
| Email delivery (SMTP) | Real `lettre`-backed SMTP variant wired (rustls, no OpenSSL). Tokio-async, pooled, implicit TLS or STARTTLS (no plaintext fallback). Configured via `CUSTODY_EMAIL__MODE=smtp` + `CUSTODY_EMAIL__SMTP_{HOST,PORT,USERNAME,PASSWORD,TLS,TIMEOUT}`. Falls back to Console with a loud warning if MODE=smtp but HOST is missing. Console + Noop remain for dev/tests. |
| Staking (ETH / SOL) | Structs only — no beacon deposit or SOL delegate broadcast |
| DeFi (WalletConnect, DEX aggregator) | Types only |
| Settlement / counterparty network | Types only |
| Tokenization / RWA | Types only |
| Multi-tenant WaaS | Workspace type exists, but billing/isolation layers are stubs |

If you see something in the sidebar, **it may be a screen with no
backing logic**. The usual tell is `Coming soon` chips (deposit chains)
or empty-state cards that never transition.

---

## Security debt the author knows about

Security work done this session is real but the following remain open.
Labeled by bite size so future-me knows where to start.

### Fixed since last published snapshot
- **Real SMTP transport.** `EmailSender::Smtp` backed by `lettre`
  (tokio1 + rustls, no OpenSSL). Two TLS modes: implicit (port 465)
  and STARTTLS (port 587, no plaintext fallback — the build
  explicitly refuses to downgrade). Optional SMTP AUTH. Selected
  via `CUSTODY_EMAIL__MODE=smtp` + the usual host/port/user/pass
  env vars. Password-reset and email-verification mails now actually
  leave the host instead of being pretty-printed to logs.
- **Removed `bc1q_hot_wallet_placeholder` fund-safety footgun.** The
  UTXO consolidation scheduler in `server/main.rs` baked a literal
  placeholder string as the sweep destination and logged "submitting
  to pipeline" while the actual submit was a `TODO`. Now requires
  `CUSTODY_HOT_WALLET__BTC_ADDRESS`; if unset the loop prints a
  one-time disabled message instead of hourly lies. Candidate logs
  explicitly say "NOT submitted — operator must consolidate manually"
  until the `tx_type=consolidation` pipeline path lands.
- **Push delivery stubs fail loudly.** `send_apns` and `send_web_push`
  used to build the payload, log it, and return `Ok(())` — callers
  saw every "send" as a success and `broadcast()` reported 100 %
  reach while nothing reached the device. Both now bail with
  "delivery not implemented in this build" so the broadcast success
  counter reflects reality. FCM path was and remains real.
- **Operational CLI stubs surface honest results.** `ops::list_utxos`
  now actually queries `get_unspent_utxos`; `ops::consolidate_utxos`
  returns an `Err` with the same "not wired" message (was
  `Ok("consolidation planned")` doing nothing).
- **Reconciliation no longer guarantees false positives.** The ETH
  balance-check capped on-chain queries at the first 50 addresses
  with `.take(50)` to duck rate limits, then compared that partial
  on-chain total against the *full* internal hot balance. Any vault
  with ≥ 51 addresses tripped a "RECONCILIATION ALERT" on every tick.
  Replaced with a batched sweep (25 addresses per batch, 200 ms
  inter-batch delay) that processes all addresses; aborts the tick
  and logs at warn level if > 10 % of RPC queries fail (partial
  totals can't be trusted against the full internal).
- **Travel Rule Notabene receive path is honest.** `receive_beneficiary_data`
  on a successfully-accepted transfer used to log "Beneficiary data
  received" and return `Ok(None)`, indistinguishable from "still
  waiting". It now returns `Err` so the caller has to record the
  transfer for manual review instead of silently treating acceptance
  as "data not yet available".
- **Dashboard env chip reflects reality.** The sidebar env selector
  was a local-state toggle — clicking "testnet" never re-routed to
  a different backend, it just changed a client-side variable.
  Replaced with a read-only chip fed from `workspaces.environment`
  via a new `workspace_environment` field on `GET /v1/user/profile`
  (joined in `load_user_profile`). Tone (green / yellow / blue) now
  matches the actual workspace environment the user is authenticated
  into.
- **Staking routes return 501 instead of fake "pending".** `POST
  /v1/staking/stake` and `/unstake` accepted requests and returned
  `{"status": "pending"}` with no signing backend, no broadcast, no
  persistence — a silent funds-stuck path if wired to real capital.
  `GET /positions` / `/rewards` / `/validators` also now 501; the
  validators list was previously hardcoded Lido/Marinade with
  promotional APYs presented as live data.
- **Settlement routes return 501.** `submit_settlement` persisted
  nothing and the corresponding `/net` built a fresh empty engine
  every call, returning `"Netting executed on pending orders"`
  against zero orders. Both now explicitly 501 until the
  `settlement_orders` table + ledger journaling land.
- **Compliance report export no longer forges SAR XML.** `GET
  /v1/admin/compliance/reports/{id}/export` previously synthesised a
  `SAR-DEMO-*` record for any id (subject_address=0x0000…, narrative
  "Demo SAR export.") and serialised it as valid SAR XML. Handing
  that to a regulator would be material misrepresentation. Now 501.
  `list_reports` also 501 (was `[]` always, implying "you've filed
  nothing"). Report creation still works; listing/export await DB.
- **Travel rule submit persists as `Required`, not `Submitted`.**
  The previous handler stamped `Submitted` without ever calling the
  counterparty VASP provider (`NotabeneProvider::send_originator_data`
  is not wired). Persisted data is now flagged so the dashboard can
  show "awaiting VASP dispatch" instead of "compliance done".
- **Vault keys expose real public keys.** `GET /v1/vaults/{id}/keys`
  and `/keys/{chain}` returned `public_key: ""` with a "would need
  to derive from seed" comment — now derive from the master seed via
  new `custody_address_factory::pubkey_hex_for_index` helper. Hex
  format: 33-byte compressed secp256k1 for BTC/ETH/Tron, 32-byte raw
  ed25519 for Solana.
- **Batch withdrawal USD total is real.** `/v1/withdrawals/batch`
  previously summed heterogeneous native-unit amounts (BTC + USDT +
  ETH) and labelled the result `total_amount_usd`. The field is now
  `Option<String>` — present only when every item's asset has a live
  price in the oracle cache; otherwise `null` with a new
  `unpriced_items: N` counter so the UI can flag "partial total".
- **ERC-20 constructor args now spec-compliant.** `erc20_constructor_args`
  previously stripped the decimal point from `initial_supply.to_string()`
  and encoded the digits as ASCII bytes, with a `u32` length field in
  place of Solidity's `uint256` length. Rebuilt around a proper
  head/tail ABI layout for `constructor(string,string,uint8,uint256)`
  with supply scaled by `10^decimals` into a uint256. Rejects
  negative supplies and inputs that overflow u128.
- **Swap status now real.** Migration 032 + `swap_executions` table;
  `POST /v1/swaps` persists the execution record, `GET /v1/swaps/{id}`
  reads from it (owner-scoped for non-Admins). Previously `GET`
  always returned 404 with "swap persistence is not yet wired to DB."
- **Swap quote is honest about being indicative.** `SwapQuote` gained
  `is_indicative: bool` + `notice: Option<String>`. Uniswap V3 quote
  on the sync path no longer fabricates `price_impact_bps: 10` and
  `estimated_gas: 0.003 ETH` — both are now unknown (0 / None). The
  fee-tier reduction is still applied but flagged with a warning
  string so callers cannot mistake it for a live Quoter RPC response.
  Jupiter sync path also flags `is_indicative: true`.
- **Push tokens persist.** `device_tokens` table (migration 020) is
  finally wired: `POST /v1/user/devices` upserts, `GET` lists the
  caller's real rows, `DELETE` scopes by `(id, user_id)`. Prior to
  this, device registration was a 201 with no storage — notifications
  silently dropped after any restart.
- **NFT routes stopped lying.** `GET /v1/nfts`, `GET /v1/nfts/{id}`,
  `POST /v1/nfts`, `DELETE /v1/nfts/{id}` now hit the
  `custodied_nfts` table (was: "empty list until DB is wired" / "TODO
  insert"). All list/get calls are owner-scoped via join on
  `vaults.owner_id`; register gates on vault ownership.
  `POST /v1/nfts/{id}/transfer` prepares EVM calldata using the
  owner's native-asset address and returns a `"prepared"` result
  (Solana/BTC/Tron return 501). The old `"status: pending, Transfer
  queued"` JSON was a fabrication — no queue existed.
- **Removed hardcoded price fallback.** `PriceOracle::seed_fallback_prices()`
  planted BTC=$65k / ETH=$3.5k / SOL=$150 into the cache on startup,
  and the `/v1/user/portfolio` endpoint used a *fresh* seeded oracle
  on every request — so real-looking USD totals were invented from
  those numbers even after CoinGecko replies had landed. Portfolio
  now reads the DB price column (written by the 60s refresh task) and
  returns `usd_value: null, price_unavailable: true` per-asset when a
  price is missing. Dashboard shows an "N asset(s) unpriced" chip so
  the total is never misread as complete.
- Session tokens now ride an `HttpOnly; Secure; SameSite=Strict` cookie
  instead of `localStorage`; an XSS on the dashboard can no longer
  exfiltrate the session. Bearer-header path kept for HMAC/service
  clients. Logout endpoint revokes + clears the cookie.
- `EmailSender` abstraction wired; password reset and email
  verification (OTP issue + confirm + status, plus registration-time
  dispatch) now go through a real transport, not `tracing::info!`.
- `detailed_health` / `circuit_breaker_status` endpoints are Admin-only.
- Safe-owner membership table (migration 031) replaces the
  "scoped to registering admin" stopgap; `add_signature` pins
  `req.signer` to the caller's linked owner addresses.

### Fixed this session
- 17 ownership-check bugs across `vault_routes`, `governance_routes`,
  `embedded_routes`, `webhook_mgmt_routes` (commits b770d00, 077c664).
- Client-trusted `approver_id` on `/v1/approvals/{id}/decide` — now
  always `auth.user_id`.
- `embedded_wallets.user_id` migrated TEXT → UUID (028).
- `safe_wallets.registered_by` added; propose/sign/execute scoped to
  the admin who registered the Safe (029).
- Self-registration rate limit (5/IP/15 min).

### Still open
- **No external audit.** The ownership fixes above were found by code
  review, not by systematic threat analysis. A real engagement (Trail
  of Bits, NCC, Halborn) would find more.
- **Signing service is in-process by default.** There is an IPC variant
  (`signer-service` crate) but the default deployment has the signer
  in the gateway binary. API compromise ⇒ key compromise in that mode.
- **Live PKCS#11 integration is stubbed.** `HsmSigningBackend` encrypts
  the seed at rest and decrypts on demand, but you still need a real
  HSM to get hardware protection. On a laptop this is, at best, DPAPI-grade.
- **Batch-withdraw address validation** is format-level only. A typo
  that produces a valid-format but wrong destination will not be caught.
- ~~**No Safe-owner membership table.**~~ Migration 031 landed:
  `safe_wallet_members (safe_address, chain, owner_address, user_id)`.
  Access check on Safe routes now threads through membership —
  propose/sign/execute require either workspace Owner, a linked
  membership row, or the registering Admin. The sign endpoint
  additionally pins `req.signer` to the caller's own linked owner
  addresses so members cannot sign on each other's behalf. Admin
  CRUD endpoints (`POST /v1/safe-wallets/{addr}/members`,
  `DELETE .../{owner_address}`, `GET .../members`) manage the roster.
- ~~**Withdrawal memo/tag validation** is length-only.~~ Now
  chain-aware (`validate_withdrawal_memo` in `custody_common::validation`):
  BTC/ETH/TRX reject any memo or tag; Solana allows ≤512-byte UTF-8
  memo and rejects numeric tags. Cosmos/XRP rules will land with the
  matching `Chain` enum variants.
- ~~**`detailed_health` / `circuit_breaker_status` endpoints** leak
  org-wide operational metrics at Viewer role.~~ Fixed — both now
  require `RequireAdmin`.

---

## What you should NOT do

- **Do not run this as-is in front of money you can't afford to lose.**
  Not today, not next month.
- **Do not expose the gateway bind address to the public internet.**
  It was designed for reverse-proxy / VPN deployment. CORS, HSTS, and
  CSRF surfaces have not been pen-tested.
- **Do not skip migrations.** Migrations 027, 028, 029 change the
  shape of `withdrawals`, `embedded_wallets`, `safe_wallets`. Running
  an older binary against a newer schema will produce 500s.
- **Do not assume the seed file is safe because it's encrypted.** It
  is encrypted with a passphrase derived from an env var. If that env
  var leaks (shell history, process listing, core dump) the seed is
  trivially recoverable.

---

## What this repo IS good for

- **A starting point** for a self-hosted custody service that wants to
  be Fireblocks-shaped rather than Metamask-shaped.
- **A reference implementation** of patterns you will not find in most
  open tutorials: HD derivation per user, withdrawal state machine,
  double-entry accounting, approval tiering, audit hash chain,
  proof-of-reserves.
- **A teaching codebase** — the crates are small, typed, and mostly
  single-purpose. `cargo test -p custody-signing` is a good way to
  see what a signing pipeline looks like.
- **A portfolio project.** It compiles, it renders, it has tests, and
  every non-trivial invariant is documented somewhere.

---

## Path to production (rough)

If someone wants to take this from "research" to "real money", the
minimum honest sequence is something like:

1. External security audit (3–6 months, $80–200k).
2. Live PKCS#11 HSM integration + MPC multi-host ceremony.
3. SOC 2 Type II engagement (12 months continuous evidence).
4. Money transmitter licensing in target jurisdictions (6–18 months,
   $2M–$5M for NY BitLicense alone).
5. Insurance (annual premium in the high six figures).
6. 24/7 on-call + incident-response retainer.

None of this is unusual for custody; all of it has to happen before the
first real dollar touches the system. The code in this repo is
necessary but nowhere close to sufficient.

---

*Last updated with commit `49c5a72`. When you add a new item, update
the relevant section above and drop a line in CHANGELOG.md.*
