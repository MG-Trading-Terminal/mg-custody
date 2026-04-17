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
| Email delivery (SMTP) | `EmailSender` enum wired (Console/Noop); password reset + email verification dispatch through it. Real SMTP pending `lettre` add. |
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
- **No Safe-owner membership table.** Ownership of a registered Gnosis
  Safe is currently scoped to the admin who registered it. A proper
  mapping from onchain owner addresses to workspace user IDs is a
  follow-up migration.
- **Withdrawal memo/tag validation** is length-only. Chain-specific
  invariants (Cosmos hash length, XRP u32 range) not enforced.
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
