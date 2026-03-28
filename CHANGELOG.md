# Changelog

All notable changes to this project will be documented in this file.
Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added (2026-03-27) — Substrate Scanner+Signer, Transaction Analytics, Custody Statements, Device Trust, API Key Usage

- **Feature 1: Polkadot/Substrate chain support** (`crates/common/src/polkadot_chain.rs`)
  - `SubstrateScanner` — async JSON-RPC HTTP client; `latest_finalized()`, `block_hash()`, `scan_block()`, `get_balance()`
  - `SubstrateSigner` — Ed25519 key container with `ZeroizeOnDrop`; `sign_transfer()` builds SCALE-encoded signed extrinsic
  - `encode_ss58(pubkey, prefix)` / `decode_ss58(address)` — full SS58 round-trip with Blake2b-512-approximate (SHA-512) checksum; two-byte canary prefix for values ≥64
  - `scale_compact_encode(u128)` / `scale_compact_decode(&[u8])` — four SCALE compact modes (single/two/four byte + big-int)
  - `Era::Immortal` / `Era::Mortal { period, phase }` — SCALE era encoding for extrinsic validity windows
  - All five chain constructors: `polkadot()`, `kusama()`, plus Moonbeam/Acala/Astar via `SubstrateChain` enum
  - 32 unit tests: SS58 round-trip (prefix 0, 2, 1284), checksum rejection, SCALE compact (all four modes + 100 DOT), balance parsing (DOT 10^10, KSM 10^12), era encoding, signer determinism, `sign_transfer` output, invalid address rejection
  - References: https://docs.substrate.io/reference/address-formats/, https://docs.substrate.io/reference/scale-codec/

- **Feature 2: Transaction analytics + fee tracking**
  - `crates/common/src/analytics.rs` — extended with `AnalyticsPeriod`, `TransactionAnalytics`, `FeeBreakdown`, `FlowDataPoint`, `AnalyticsSummaryQuery`
  - `AnalyticsPeriod::bounds()` — returns `(DateTime<Utc>, DateTime<Utc>)` for any period variant including December year-wrap
  - `TransactionAnalytics::compute()` — derives `net_flow_usd` and `avg_fee_per_tx_usd` from raw inputs; zero-safe division
  - `FeeBreakdown::new()` — computes `avg_fee_native`; zero-safe
  - `AnalyticsSummaryQuery::to_period()` — parses `period=daily|weekly|monthly` query params with validation
  - `crates/db/src/analytics.rs` — new DB module: `daily_summary()`, `fee_breakdown_by_chain()`, `top_transactions()`, `flow_timeseries()` (hourly/daily truncation)
  - `dashboard/src/routes/analytics/+page.svelte` — summary cards (inflow/outflow/net/fees/busiest chain), per-chain fee table, top-transactions table; date filter controls
  - 18 unit tests (net flow, zero-tx divide guard, fee avg, period labels, bounds, month wrap, query parse, invalid month/period rejection)

- **Feature 3: Custody statements**
  - `crates/common/src/custody_statement.rs` — `CustodyStatement`, `StatementPeriod`, `AssetBalance`, `StatementEntry`, `FeeSummary`
  - `StatementPeriod::parse("YYYY-MM")`, `start_date()`, `end_date_exclusive()`, December year-wrap
  - `CustodyStatement::compute_attestation_hash()` — SHA-256 over canonical JSON payload (all material fields except the hash itself)
  - `CustodyStatement::verify_attestation()` — recomputes and compares; detects tampered fields
  - `export_statement_json()`, `export_statement_text()`, `export_statement_csv()` — three export formats; amounts always `Decimal::to_string()` (never float)
  - `crates/db/src/statements.rs` — `upsert_statement()` (ON CONFLICT idempotent), `list_statements()`, `get_statement()`, `delete_statement()`
  - 17 unit tests (period parse, invalid month, December wrap, attestation hash length/format, verification, tamper detection, flow calculations, JSON/text/CSV export, no scientific notation in CSV)

- **Feature 4: Concurrent session limits + device trust**
  - `crates/db/migrations/022_device_trust.sql` — `ALTER TABLE sessions ADD COLUMN device_fingerprint TEXT`; `CREATE TABLE trusted_devices (id, user_id, device_fingerprint, name, trusted_until, created_at, UNIQUE(user_id, device_fingerprint))`
  - `crates/db/src/device_trust.rs` — `is_device_trusted()`, `trust_device()` (upsert), `list_trusted_devices()`, `revoke_trusted_device()`, `revoke_all_trusted_devices()`, `cleanup_expired_trusted_devices()`
  - `enforce_session_limit(user_id, max_sessions)` — counts active sessions, revokes oldest N via `ORDER BY created_at ASC LIMIT excess`; returns count revoked; emits tracing log on revocation
  - `count_active_sessions(user_id)` — helper for callers
  - Constants: `DEFAULT_MAX_CONCURRENT_SESSIONS = 5`, `DEFAULT_TRUST_DURATION_DAYS = 30`
  - 4 unit tests (constant values, TrustedDevice struct fields, serialization)

- **Feature 5: API key usage analytics**
  - `crates/db/migrations/023_api_key_usage.sql` — `CREATE TABLE api_key_usage (key_id, date, request_count, last_endpoint, last_ip, PRIMARY KEY (key_id, date))`
  - `crates/db/src/api_key_usage.rs` — `record_api_key_usage()` (ON CONFLICT DO UPDATE atomic increment), `get_api_key_usage()`, `total_api_key_requests()`, `get_expiring_keys(within_days)`, `cleanup_old_api_key_usage(retain_days)`
  - `DailyUsage` and `ApiKeyInfo` (with `days_until_expiry: Option<i64>`) row types with Serialize
  - 6 unit tests (field checks, serialization, aggregation logic, expiry detection, negative days for expired keys)

### Added (2026-03-27) — Cosmos Scanner+Signer, Staking Executor, i18n Dashboard, Push Notifications, IP Geo Detection

- **Feature 1: Cosmos SDK chain support**
  - `crates/common/src/cosmos_chain.rs` — rewritten with real `CosmosScanner` (async REST LCD client), `CosmosSigner` (Amino JSON signing), and `StdSignDoc` types
  - `CosmosScanner::cosmos_hub()` and `::osmosis()` factory methods; `scan_block(height)`, `get_balance(address)`, `latest_height()` async methods
  - `CosmosSigner::sign_send()` and `sign_ibc_transfer()` — secp256k1 Amino JSON signing with canonical key-sorted JSON
  - `bech32_address(pubkey, prefix)` — SHA-256 + RIPEMD-160 + bech32 address derivation matching Cosmos spec
  - `SignedTx::to_broadcast_json()` — produces `StdTx` JSON for `/cosmos/tx/v1beta1/txs` broadcast
  - Key material zeroed in `CosmosSigner::drop()`; private key never logged or serialized
  - 20 unit tests (scanner defaults, uatom→ATOM conversion, bech32, sign doc fields, canonical JSON sort order, signer determinism, zero key rejection, broadcast JSON structure)
  - References: Cosmos LCD REST API https://v1.cosmos.network/rpc/, Amino signing spec https://docs.cosmos.network/v0.47/learn/beginner/tx-lifecycle

- **Feature 2: Staking execution**
  - `crates/common/src/staking_executor.rs` — new `StakingExecutor` wiring existing calldata builders into an execution flow
  - `create_eth_stake(amount, withdrawal_credentials, pubkey, sig, root)` — validates exactly 32 ETH, decodes hex args, calls deposit contract ABI encoder
  - `create_sol_delegation(amount, validator, from, stake_account)` — validates ≥0.01 SOL minimum, converts SOL→lamports, decodes base58 vote account, calls stake instruction builder
  - `create_unstake(position_id, chain)` and `create_claim_rewards(position_id, chain)` — chain validation, operation record construction
  - Added `ETH_DEPOSIT_CONTRACT`, `build_eth_deposit_calldata_full()`, and `build_sol_stake_instructions()` to `staking_contracts.rs`
  - `StakingOperation` and `StakingOpStatus` types with Serialize/Deserialize
  - 20 unit tests (32 ETH enforced, fractional rejection, calldata selector, unique IDs, SOL minimum, base58 decode, unsupported chain rejection)

- **Feature 3: i18n — multi-language dashboard**
  - `dashboard/src/lib/i18n.ts` — complete i18n module: 3 locales (en/ru/zh), 60+ translation keys covering all nav items, portfolio, deposit, withdraw, transactions, approvals, profile, wallets, policies, users, audit, settings, and common labels
  - Svelte `writable` store for reactive locale state; `setLocale()` persists to `localStorage`; `detectInitialLocale()` reads browser `navigator.language`
  - `t(key)` — returns translation for current locale, falls back to `en`, then raw key
  - `+layout.svelte` updated: nav labels use `t()` + `$locale` reactive pattern; language picker select control in sidebar footer
  - `+page.svelte` (Portfolio) updated: all user-visible strings replaced with `t()` calls

- **Feature 4: Mobile push notifications**
  - `crates/common/src/push_notifications.rs` — `PushNotificationService`, `DeviceToken`, `Platform`, `PushPayload` types
  - `send_fcm()` — HTTP POST to `https://fcm.googleapis.com/fcm/send` with `Authorization: key=...` header
  - `send_apns()` — formats APNs aps payload, logs with tracing (HTTP/2 stub with clear TODO comment)
  - `send_web_push()` — formats RFC 8030 notification payload, logs (VAPID stub)
  - `broadcast(tokens, payload)` — fans out to multiple devices, logs per-device failures without aborting
  - `PushPayload::withdrawal_approval()`, `large_deposit()`, `circuit_breaker_tripped()` convenience constructors
  - Migration `020_push_tokens.sql` — `device_tokens` table with platform CHECK constraint and UNIQUE token
  - `crates/gateway/src/push_routes.rs` — `POST/GET /v1/user/devices` and `DELETE /v1/user/devices/{id}` handlers (RequireViewer); token validation, platform check, prefix-only logging
  - Wired into `server/src/main.rs` in both dev and HMAC router branches
  - 14 unit tests (platform serialization, payload constructors, FCM/APNs/web payload structure, service construction)

- **Feature 5: IP geolocation + login anomaly detection**
  - `crates/common/src/geo_detection.rs` — pure in-memory IP geolocation (no external API calls)
  - `is_known_tor_exit(ip)` — hardcoded list of 75+ well-known Tor exit nodes
  - `is_datacenter_ip(ip)` — CIDR range checks for AWS, GCP, Azure, Hetzner, DigitalOcean, OVH, Linode, Vultr, Cloudflare
  - `detect_country(ip)` — maps IPv4 to country via static CIDR blocks (top countries); IPv6 → is_vpn=true; IANA private ranges → "Private/Loopback"
  - `is_new_country(history, country)` — returns true if country has never appeared in user's successful login history
  - `analyze_login(ip, history)` — returns `GeoInfo` + `Vec<LoginAnomaly>` with severity levels (High: Tor, Medium: VPN/new country)
  - Migration `021_login_history_geo.sql` — adds `country_code`, `is_tor`, `is_vpn`, `anomaly_flags` to `login_history`
  - 28 unit tests (IP parser, CIDR range math, Tor detection, datacenter ranges, private IP handling, country detection, new-country logic, analyze_login severity)

### Added (2026-03-27) — SDK Auto-Generation, Rate Limit Persistence, Compliance Reporting, EVM L2 Wiring, API Versioning

- **Feature 1: SDK auto-generation (Python + TypeScript)**
  - New `crates/common/src/sdk_generator.rs` — generates real, working client code (not stubs)
  - `generate_python_sdk()`: complete Python file with `CustodyClient` class, HMAC signing, typed dataclasses, docstrings, all 7 major API methods, `CustodyApiError` exception
  - `generate_typescript_sdk()`: complete TypeScript file with full interface types, `CustodyClient` class, async methods, HMAC signing via Node `crypto` module, `CustodyApiError` class
  - Both SDKs enforce string amounts — no float in monetary fields
  - `GET /v1/sdk/python` and `GET /v1/sdk/typescript` endpoints in `routes.rs` serve as text/plain downloads
  - 13 unit tests covering method presence, HMAC computation, type correctness, error classes, non-empty output

- **Feature 2: Rate limit persistence in DB**
  - Migration `019_rate_limits.sql`: `rate_limit_counters` table with `key`, `count`, `window_start`, `window_seconds`
  - `UserRateLimiter` gains `with_db_pool()`, `load_from_db()`, `flush_to_db()`, and `start_sync_task(interval)` methods
  - In-memory SlidingWindow is the fast path; DB persistence survives restarts — avoids bypass window on restart
  - Background sync task flushes dirty counters every 10 seconds; cleanup evicts stale windows
  - Uses `sqlx::query()` (non-macro) — no DATABASE_URL required at compile time
  - Lock is never held across `await` — MutexGuard is dropped before any DB call
  - All 13 existing rate limit tests continue to pass

- **Feature 3: Compliance reporting (SAR/CTR)**
  - New `crates/compliance/src/reporting.rs` — SAR and CTR generation and export
  - `generate_sar(screening, tx) -> SuspiciousActivityReport` — creates Draft SAR from screening result
  - `generate_ctr(tx, threshold_usd) -> Option<CurrencyTransactionReport>` — auto-generates CTR for txs > $10K per 31 U.S.C. § 5313
  - `export_sar_xml(report) -> String` — FinCEN BSA E-Filing XML format with proper entity escaping
  - `export_ctr_csv(reports) -> String` — CSV with comma-quoting for compliance
  - New `crates/gateway/src/compliance_routes.rs` with `GET /v1/admin/compliance/reports`, `POST /v1/admin/compliance/reports/sar`, `GET /v1/admin/compliance/reports/{id}/export`
  - All endpoints require `RequireAdmin` role
  - 15 unit tests: SAR generation, CTR threshold boundary (exact threshold = no CTR), filing deadline (15 days), XML structure, XML escaping, CSV format, CSV empty case

- **Feature 4: EVM L2 scanner wiring**
  - `crates/server/src/main.rs` now spawns scanner tasks for all enabled non-Ethereum EVM chains from `default_evm_chains()` (Arbitrum, Optimism, Base, Polygon, Avalanche, BSC, zkSync, Linea, Scroll, Mantle)
  - Uses `EvmScanner::from_chain_def()` — each chain gets its own `AddressCache` and task
  - Loop follows the same error-backoff pattern as Ethereum scanner (60s backoff on RPC failure)
  - Sleep interval is `chain_def.block_time_secs` — Arbitrum sleeps 1s, Polygon 2s, BSC 3s
  - Failed scanner initialization logs warning and continues — does not crash the service

- **Feature 5: API versioning headers middleware**
  - New `api_version_headers` middleware in `crates/gateway/src/middleware.rs`
  - Adds `X-API-Version: 1.0` and `X-Powered-By: MG-Custody/1.0` to every response
  - Adds `Deprecation: true`, `Sunset: 2026-06-01`, `Link: </v1/openapi.json>; rel="successor-version"` for any `/v0` prefixed path (RFC 8594)
  - Wired into both `router()` and `router_with_middleware()` in routes.rs
  - 3 tests: deprecated headers present for v0, absent for v1, v1 in supported versions list

### Added (2026-03-27) — Batch Transactions, Address Book Labels, Token Import, Workspace Isolation

- **Feature 1: Atomic batch withdrawals** — `POST /v1/withdrawals/batch`
  - All-or-nothing semantics: all items validated before any DB write; single PostgreSQL transaction wraps all inserts
  - Max 50 withdrawals per batch (was previously 100, reduced for safety)
  - Upgraded to `RequireSigner` (was `RequireEditor`) — bulk fund movement is high-risk
  - New `insert_withdrawals_atomic(&[Withdrawal])` method in `crates/db/src/withdrawals.rs` — transaction logic in DB layer, no raw sqlx in gateway
  - Response shape changed: `count` + `total_amount_usd` replace `accepted`/`rejected` (batch never partially succeeds)
  - New `ApiError::unprocessable_entity(msg)` (HTTP 422) for validation failures that list all errors before rejecting
  - Route moved from `/v1/transactions/batch` to `/v1/withdrawals/batch` to match spec

- **Feature 2: Address book labels and search** — `PUT /v1/address-book/{id}/label`, `GET /v1/addresses/search?q=`
  - Migration `018_address_labels.sql`: `ADD COLUMN IF NOT EXISTS notes TEXT`, `ADD COLUMN IF NOT EXISTS last_used_at TIMESTAMPTZ`, GIN index on `label` for full-text search
  - `AddressBookEntry` common type gains `notes: Option<String>` and `last_used_at: Option<DateTime<Utc>>`
  - `CustodyDb::update_address_label(id, label, notes)` — COALESCE partial update, `RequireEditor`
  - `CustodyDb::search_addresses(query)` — ILIKE on label and address, LIKE-escape special chars, 50-result cap
  - New `UpdateAddressLabelRequest` type in `custody-common::webhook`

- **Feature 3: Custom token import at runtime** — `POST /v1/admin/tokens/import`
  - `ImportTokenRequest` type in `custody-common::vault` (`symbol`, `chain`, `contract_address`, `decimals`, `name`, `coingecko_id`)
  - Chain-specific address validation: ETH (0x + 42 hex chars), Tron (T prefix + 34 chars), Solana (32–44 base58), Bitcoin (rejected — no contract model)
  - Duplicate check on `(symbol, chain)` pair before insert — returns 409 Conflict with existing asset ID
  - `RequireAdmin` required; asset is inserted enabled=true and is_native=false
  - 18 unit tests covering all chain validation cases and serde round-trips

- **Feature 4: Multi-workspace isolation enforcement**
  - `CustodyDb::find_vault_for_workspace(vault_id, owner_id)` — SQL `WHERE id = $1 AND owner_id = $2`; returns `None` for cross-workspace access (not an error) to prevent vault-ID enumeration
  - `CustodyDb::list_vaults_for_workspace(owner_id, include_hidden)` — explicit workspace-scoped listing
  - `get_vault`, `update_vault`, `delete_vault`, `create_vault_account`, `list_vault_accounts`, `get_vault_account` all now call workspace-scoped queries using `auth.0.user_id`
  - Cross-workspace access returns 404 (not 403) — prevents enumeration of vault IDs belonging to other workspaces

### Added (2026-03-27) — Deployment Runbook

- **`docs/DEPLOYMENT_RUNBOOK.md`** — comprehensive operator runbook covering:
  - Hardware requirements (CPU, RAM, disk), software prerequisites (Rust 1.85+, PostgreSQL 16+, Node.js 22+), firewall rules, and secrets generation checklist
  - Method A: Docker Compose deployment with all environment variables, seed generation, Shamir backup, and health verification steps
  - Method B: Binary from source with systemd service file and nginx reverse proxy config
  - Initial configuration: HMAC secret rotation, seed ceremony, per-chain RPC setup, circuit breaker tuning, FIDO2 domain setup
  - Security hardening: signer isolation (Mode 2 target), HSM config placeholders (Phase 4C), OS-level hardening (core dump disable, ptrace restriction), pre-deployment checklist
  - Operational procedures: key rotation schedule (HMAC, admin key, DB password), quarterly rotation steps, emergency seed rotation trigger conditions
  - Backup and recovery: daily `pg_dump` cron, seed file backup, three recovery scenarios (DB loss, seed file loss, complete host loss) with step-by-step commands and RTO targets
  - Monitoring: Prometheus scrape config, Grafana import instructions, key metrics table with alert thresholds, Alertmanager rules for scan lag, RPC down, withdrawal failures, circuit breaker trip
  - Incident response: circuit breaker trip procedure, emergency freeze (6-step compromise protocol), lost FIDO2 key recovery, DB corruption point-in-time recovery
  - Upgrade procedure: rolling upgrade with pre-upgrade backup, migration application, Docker Compose upgrade, rollback procedure
  - Troubleshooting: 8 common error patterns with root cause and fix, log grep patterns, health check interpretation, direct DB diagnostic queries
  - Background tasks reference table with intervals and descriptions

### Added (2026-03-27) — Mobile-Responsive Dashboard, Production Docker, Real QR Code SVG

- **`dashboard/src/routes/+layout.svelte`** — mobile-responsive sidebar:
  - Hamburger button (three-bar icon, visible only on `<768px`)
  - Sidebar slides in from left on hamburger click; closes on nav link click or overlay tap
  - Dark overlay backdrop when sidebar is open on mobile
  - `@media (max-width: 768px)` CSS: sidebar `position:fixed; left:-220px; transition:left 0.2s`, `.sidebar.open { left:0 }`, `content { padding:1rem }`
  - Sticky mobile header bar with title when sidebar is collapsed

- **Dashboard pages** — horizontal scroll for all tables on mobile:
  - Added `.table-wrap { overflow-x: auto; -webkit-overflow-scrolling: touch }` wrapper to all `<table>` elements in: `+page.svelte`, `addresses`, `approvals`, `audit`, `deposit`, `transactions`, `users`, `wallets`, `withdraw`, `policies`, `settings`
  - Withdrawal form grid collapses to single column on mobile (`grid-template-columns: 1fr`)
  - Portfolio top-row cards stack vertically on mobile

- **`Dockerfile`** — three-stage multi-stage build:
  - Stage 1 (`node:22-alpine`): builds SvelteKit dashboard (`npm ci && npm run build`)
  - Stage 2 (`rust:1.85-bookworm`): copies dashboard build, compiles all Rust binaries
  - Stage 3 (`debian:bookworm-slim`): copies binaries + dashboard build + config, sets `CUSTODY_DASHBOARD_DIR`

- **`docker-compose.yml`** — production-ready compose:
  - Network isolation: `frontend` (custody ↔ host) and `backend` (internal only, postgres + signer)
  - `signer` service has no `frontend` network — isolated to backend
  - All secrets via env vars (`${POSTGRES_PASSWORD}`, `${CUSTODY_HMAC_SECRET}`, etc.)
  - FIDO2 RP_ID and RP_ORIGIN configurable
  - `restart: unless-stopped` on all services
  - Postgres `start_period: 10s`, `retries: 10`

- **`crates/common/src/qr_svg.rs`** — ISO/IEC 18004:2015 QR Code Model 2 generator:
  - Alphanumeric mode encoding (0–9, A–Z, space, $ % * + - . / :)
  - Versions 1–10 (21×21 to 57×57 modules), EC level L
  - Reed-Solomon GF(256) error correction with generator polynomial 285
  - Module placement: finder patterns, alignment patterns, timing patterns, dark module, format info areas
  - All 8 mask patterns evaluated; best chosen by ISO penalty scoring (rules 1–4)
  - BCH(15,5) format information with XOR mask 101010000010010
  - SVG output: pure `<rect>` elements, white background, 4px module size, 4-module quiet zone
  - 10 unit tests: character set correctness, encoding arithmetic, RS length, mask pattern inversion, module matrix size, SVG dimensions, full round-trips for HELLO, ETH address, BTC address

- **`crates/gateway/src/user_routes.rs`** — `deposit_address_qr` handler:
  - Replaced placeholder text-SVG with `custody_common::qr_svg::generate_qr_svg`
  - QR data uses uppercased `format_qr_data` output (BIP-21 / EIP-681 URI prefix preserved)
  - Graceful fallback to plain-text SVG if address contains non-alphanumeric characters

### Added (2026-03-28) — Per-User Rate Limiting, Login History, Email OTP + Password Reset

- **`crates/gateway/src/rate_limit.rs`** — per-user sliding window rate limiter:
  - `UserRateLimiter` — sliding window counter keyed on `"user:{uuid}"` or `"ip:{ip}"`
  - `RoleLimit::for_role_ordinal` — per-role caps: Viewer=30, Editor=60, Signer=100, Admin=200, Owner=500 req/min
  - `user_key` / `ip_key` — key builders for user and IP buckets
  - Falls back to IP-based `RateLimiter` (token bucket) for unauthenticated requests
  - `UserRateLimiter::cleanup()` — evicts windows idle for 2× window duration
  - 13 new unit tests: per-role values, ordinal mapping, sliding window expiry, different-user independence, retry_after ≥ 1s

- **`crates/gateway/src/middleware.rs`** — updated `rate_limit` middleware:
  - Reads `AuthContext` from request extensions (injected by `rbac_auth`)
  - Authenticated requests: per-user sliding window with role-based cap
  - Unauthenticated: IP token bucket fallback
  - Adds `X-RateLimit-Limit`, `X-RateLimit-Reset`, `X-RateLimit-Remaining` headers to every response
  - Returns 429 with `Retry-After` header on limit exceeded
  - `MiddlewareState` gains `user_rate_limiter: Arc<UserRateLimiter>` field

- **`crates/gateway/src/state.rs`** — `AppState` gains `user_rate_limiter: Arc<UserRateLimiter>`

- **`crates/db/migrations/016_login_history.sql`** — login history table:
  - `login_history(id, user_id, ip_address, user_agent, method, success, failure_reason, created_at)`
  - `method` values: `password`, `totp`, `fido2`, `backup_code`, `email_otp`
  - Index `idx_login_history_user` on `(user_id, created_at DESC)` for fast per-user queries

- **`crates/db/src/login_history.rs`** — login history DB operations:
  - `record_login(user_id, ip, ua, method, success, failure_reason)` — insert one row
  - `list_login_history(user_id, limit)` — newest-first, limit clamped to 1–200
  - `count_failed_logins(user_id, since)` — count failures since a timestamp (brute-force detection)
  - `cleanup_old_logins(days)` — purge rows older than N days
  - 6 unit tests: serialization, brute-force threshold logic, limit clamping

- **`crates/gateway/src/auth_routes.rs`** — updated `login` handler:
  - Records every login attempt (success and failure) in `login_history`
  - Extracts real IP from `X-Forwarded-For`, user agent from `User-Agent`
  - Brute-force lockout: 5 failures in 15 minutes → 429 with 5-minute cooldown message
  - Locked attempts recorded with `failure_reason = 'account_locked'`
  - New `GET /v1/user/login-history` endpoint (RequireViewer) — lists recent entries

- **`dashboard/src/routes/login-history/+page.svelte`** — login history dashboard page:
  - Table: Time, IP Address, Method, Success/Fail badge, Failure Reason, User Agent
  - Success rows neutral, failure rows highlighted with danger background

- **`crates/db/migrations/017_email_otp.sql`** — email OTP table:
  - `email_otp(id, user_id, code_hash, purpose, created_at, expires_at, consumed_at)`
  - Only SHA-256 hash of 6-digit code stored — raw code never persisted
  - Purpose values: `login`, `password_reset`, `email_verify`
  - Codes expire after 10 minutes; single-use via `consumed_at` atomic UPDATE RETURNING

- **`crates/db/src/email_otp.rs`** — email OTP DB operations:
  - `generate_otp_code()` — 6-digit decimal, zero-padded, OsRng CSPRNG
  - `hash_otp_code(raw)` — SHA-256 hex
  - `create_email_otp(user_id, purpose)` — inserts hash, returns raw code
  - `verify_email_otp(user_id, code, purpose)` — atomic verify-and-consume, constant-time
  - `cleanup_expired_otps()` — delete expired or consumed rows
  - `find_user_id_by_email_for_reset(email)` — for password reset (includes disabled accounts)
  - 8 unit tests: code length, leading zeros, hash determinism/uniqueness, expiry constant bounds

- **`crates/gateway/src/password_reset_routes.rs`** — password reset endpoints:
  - `POST /v1/auth/password-reset/request` — generates OTP, logs dispatch (SMTP stub)
  - `POST /v1/auth/password-reset/confirm` — verifies OTP, updates password hash, revokes all sessions
  - Minimum password length: 12 characters
  - Generic error responses prevent email enumeration
  - 7 unit tests: input validation, body deserialization, response serialization

- **`crates/gateway/src/totp_routes.rs`** — extended `verify_2fa_login`:
  - Accepts `code_type: "email_otp"` to route 6-digit codes to `email_otp` table instead of TOTP
  - Falls through to email OTP path when TOTP is not enrolled for the user

- Public paths updated in `rbac_middleware::RbacState` and rate-limit `MiddlewareState` to include password-reset endpoints

### Added (2026-03-28) — Phase 28: TOTP 2FA + Proof of Reserves User Verification

- **`crates/common/src/totp.rs`** — RFC 6238 TOTP implementation:
  - `TotpSecret` — 20-byte secret, `Zeroize + ZeroizeOnDrop`, never logged or serialized plaintext
  - `TotpSecret::generate()` — OsRng CSPRNG, 20 random bytes
  - `TotpSecret::generate_code(time)` — HMAC-SHA1 + dynamic truncation per RFC 4226 §5.3
  - `TotpSecret::verify(code, time)` — ±1 window clock-skew tolerance, constant-time comparison
  - `TotpSecret::to_base32()` / `from_base32()` — RFC 4648 standard alphabet, no padding
  - `TotpSecret::provisioning_uri(email, issuer)` — `otpauth://totp/` scheme for QR code
  - 13 unit tests including RFC 6238 Appendix B test vectors (SHA1, 6 digits)

- **`crates/db/migrations/015_totp.sql`** — TOTP persistence:
  - `user_totp` — encrypted secret (AES-256-GCM), `enabled` flag, 8 backup code hashes, `backup_codes_used` boolean array
  - `pending_2fa_tokens` — short-lived (5 min) intermediate token issued after password check, before TOTP; single-use via `consumed_at`

- **`crates/db/src/totp.rs`** — TOTP DB operations:
  - `encrypt_secret` / `decrypt_secret` — AES-256-GCM, nonce stored as `hex(nonce):hex(ciphertext)`
  - `generate_backup_codes()` — 8 × 8-char alphanumeric (no ambiguous chars), returns `(raw, hashed)`
  - `hash_backup_code(code)` — SHA-256, case-insensitive
  - `upsert_totp_setup`, `enable_totp`, `delete_totp`, `load_totp` — full lifecycle
  - `consume_backup_code` — atomic mark-used by array index, checks not-already-consumed
  - `create_pending_2fa_token`, `consume_pending_2fa_token` — single-use 5-min tokens
  - 11 unit tests: encrypt/decrypt roundtrip, wrong-key failure, tampered ciphertext, backup code uniqueness, case-insensitivity, format validation

- **`crates/gateway/src/totp_routes.rs`** — TOTP API routes:
  - `POST /v1/auth/totp/setup` — generate secret, return base32 + provisioning URI + 8 raw backup codes (shown once)
  - `POST /v1/auth/totp/verify` — verify first code, enable TOTP; rejects if already enabled
  - `POST /v1/auth/totp/disable` — requires valid TOTP code (Admin may skip); revokes all sessions
  - `POST /v1/auth/sessions/verify-2fa` — consume pending token + TOTP/backup code → full session
  - Encryption key: SHA-256("totp-v1:" || seed[0..32]) — deterministic across restarts
  - 2 unit tests: key determinism, different seeds produce different keys

- **Updated `crates/gateway/src/auth_routes.rs`** — 2FA login flow:
  - `LoginResponse` updated: `token/user_id/role/expires_in_seconds` are now `Option<_>`, `requires_2fa: bool`, `pending_token: Option<String>` added
  - Login checks `load_totp(user_id).enabled` — if true, issues 5-min `pending_token` instead of session
  - 2 updated tests for both response shapes

- **Updated `crates/gateway/src/rbac_middleware.rs`** — `/v1/auth/sessions/verify-2fa` added to public paths (pre-auth endpoint)

- **`crates/reconciliation/src/proof_of_reserves.rs`** — `generate_proof` function added:
  - Generates Merkle inclusion proof for any leaf index in O(log N)
  - Exactly mirrors `merkle_root` logic (odd-level duplication, `while len > 1` condition)
  - Single-leaf tree: empty sibling list, root = leaf hash (consistent with `merkle_root` behavior)
  - 5 new tests: single-leaf, two-leaf both, four-leaf all, three-leaf (odd tree) all, out-of-bounds → None

- **Updated `crates/gateway/src/user_routes.rs`** — Proof of Reserves endpoints:
  - `GET /v1/user/proof-of-reserves` — returns user's balance leaves, Merkle proofs, root, attestation time
  - `POST /v1/user/proof-of-reserves/verify` — verifies a submitted proof; accepts/strips `0x` prefixes
  - `PorBalanceItem`, `ProofOfReservesResponse`, `PorLeafProof`, `VerifyProofRequest`, `VerifyProofResponse` types
  - 7 unit tests: single-leaf verify, two-leaf both, four-leaf all, tampered leaf fails, tampered sibling fails, empty balances, odd tree (3 leaves)

### Added (2026-03-27) — Phase 27: Gnosis Safe Multi-Sig + Scheduled Withdrawals + WebSocket Enhancements

- **`crates/common/src/safe_executor.rs`** — Gnosis Safe multi-sig execution engine:
  - `SafeWallet` — registered Safe proxy with address, chain, threshold, owners, nonce
  - `SafeManagedTx` — proposed transaction with EIP-712 hash, signatures, status state machine
  - `SafeTxStatus` — `Proposed → Signing → Ready → Executed / Failed`
  - `SafeOperation` re-exported from `safe_wallet` for downstream use
  - `SafeSignature` — 65-byte ECDSA signature with signer + timestamp
  - `SafeWallet::propose_transaction()` — computes EIP-712 safe_tx_hash, returns `SafeManagedTx`
  - `SafeManagedTx::add_signature()` — validates owner membership, rejects duplicates + wrong length, transitions status
  - `SafeManagedTx::is_ready()` — checks `signatures.len() >= threshold`
  - `SafeManagedTx::encode_exec()` — builds `execTransaction` calldata (0x6a761202) with signatures sorted ascending per Safe.sol#checkNSignatures
  - `compute_safe_tx_hash()` — full EIP-712: `0x1901 || domainSeparator || keccak256(encodeTransactionData)`, uses `sha3::Keccak256` directly, no float, deterministic
  - API request/response types: `RegisterSafeRequest`, `ProposeSafeTxRequest`, `AddSafeSignatureRequest`, `SafeWalletResponse`, `SafeManagedTxResponse` — all amounts as string-encoded Decimal
  - 12 unit tests: hash determinism, different chain IDs, different nonces, signature validation, threshold logic, encode_exec selector, case-insensitive owner check, no-float response
- **`crates/common/src/scheduled_withdrawal.rs`** — Scheduled and recurring withdrawals:
  - `WithdrawalSchedule` — `Once`, `Daily { hour_utc }`, `Weekly { day, hour_utc }`, `Monthly { day_of_month, hour_utc }`
  - `WithdrawalSchedule::next_occurrence(after)` — computes next UTC execution time; `Monthly` clamps day ≤ 28 for all months
  - `ScheduledWithdrawal` — full record with `executions_count`, `max_executions`, `enabled`, `next_execution`
  - `ScheduledWithdrawal::should_execute(now)` — enabled && due && not exhausted
  - `ScheduledWithdrawal::advance(now)` — atomic: increments count, updates `last_executed`, computes next occurrence, disables `Once` and exhausted schedules
  - 17 unit tests: all schedule variants, boundary conditions, month crossings, disable-on-exhaust, no-float amount
- **`crates/db/migrations/013_scheduled_withdrawals.sql`** — `scheduled_withdrawals` table; amount stored as TEXT; indexed on `(next_execution WHERE enabled = true)` and `user_id`
- **`crates/db/migrations/014_safe_wallets.sql`** — `safe_wallets` table (threshold, owners as JSONB, nonce) + `safe_transactions` table (value_wei as TEXT, data as BYTEA, signatures as JSONB); indexes on `lower(safe_address)` and status
- **`crates/db/src/scheduled_withdrawals.rs`** — CRUD: `insert_scheduled_withdrawal`, `list_scheduled_withdrawals`, `due_scheduled_withdrawals`, `advance_scheduled_withdrawal`, `cancel_scheduled_withdrawal`; schedule serialized to `(schedule_type TEXT, schedule_config JSONB)`; 7 unit tests: roundtrip for all schedule types, unknown type error, weekday parsing, Decimal precision
- **`crates/db/src/safe_wallets.rs`** — CRUD: `upsert_safe_wallet`, `find_safe_wallet`, `list_safe_wallets`, `insert_safe_tx`, `update_safe_tx`, `find_safe_tx`, `list_safe_txs`; value stored as TEXT; 4 unit tests: status roundtrip, op roundtrip, error cases
- **`crates/gateway/src/safe_routes.rs`** — Gnosis Safe API routes:
  - `POST /v1/safe-wallets` (`RequireAdmin`) — register Safe with threshold/owners validation
  - `GET /v1/safe-wallets` (`RequireViewer`) — list all registered Safes
  - `POST /v1/safe-wallets/{address}/transactions` (`RequireEditor`) — propose transaction
  - `POST /v1/safe-wallets/{address}/transactions/{id}/sign` (`RequireSigner`) — add ECDSA signature
  - `GET /v1/safe-wallets/{address}/transactions` (`RequireViewer`) — list transactions for a Safe
  - `POST /v1/safe-wallets/{address}/transactions/{id}/execute` (`RequireSigner`) — encode execTransaction calldata, mark Executed
  - 4 unit tests: hex decoding edge cases
- **`crates/gateway/src/user_routes.rs`** — Scheduled withdrawal endpoints:
  - `POST /v1/user/scheduled-withdrawals` (`RequireEditor`) — create schedule, validates amount > 0
  - `GET /v1/user/scheduled-withdrawals` (`RequireViewer`) — list user's schedules
  - `DELETE /v1/user/scheduled-withdrawals/{id}` (`RequireEditor`) — cancel (sets `enabled = false`)
- **`crates/gateway/src/websocket.rs`** — Enhanced WebSocket with auth + new event types:
  - `WsEvent` enum adds `BalanceUpdate`, `TransactionStatusChange`, `ApprovalRequired`, `Ping`
  - Auth via `?token=<bearer>` query param (WS headers inaccessible in browser API)
  - Session resolved from DB; falls back gracefully on invalid/missing token
  - `filter` shared between send/recv tasks via `Arc<tokio::sync::Mutex<EventFilter>>`; recv task updates filter from client JSON messages
  - `balance_update_event()`, `tx_status_change_event()`, `approval_required_event()` — public helpers for pipeline/confirmation tracker to call
  - Heartbeat `Ping` sent immediately on connect
  - Lag detection: logs dropped events without disconnecting
  - 8 unit tests: serialization, no-float amounts, filter integration, new event types
- **`crates/server/src/main.rs`** — scheduled withdrawal background task (every 60s): loads due schedules, creates `Withdrawal` records, calls `advance_scheduled_withdrawal`; adds migrations 013 + 014 to startup sequence
- **`crates/common/src/lib.rs`** — exports `safe_executor`, `scheduled_withdrawal`
- **`crates/db/src/lib.rs`** — exports `safe_wallets`, `scheduled_withdrawals`
- **`crates/gateway/src/lib.rs`** — exports `safe_routes`

### Added (2026-03-27) — Phase 26: Notification Dispatch + CSV Export + QR SVG

- **`crates/confirmations/src/notification_dispatch.rs`** — `NotificationDispatcher`:
  - Routes custody events (`deposit_confirmed`, `withdrawal_broadcast`, `withdrawal_confirmed`, `withdrawal_failed`) to all matching enabled channels
  - `ChannelType` enum: `Webhook`, `Email`, `Slack`
  - `NotificationChannelConfig` — per-channel config with event filter list; empty list = "all events"
  - `dispatch_event(&str, Value)` — fan-out to all channels; per-channel errors logged, do not abort others
  - `send_webhook(url, payload)` — HTTP POST with `X-Custody-Timestamp` + `X-Custody-Signature` (HMAC-SHA256 identical to exchange webhook format); `send_webhook_with_retry` adds exponential-backoff retry loop (cap 300 s)
  - `send_email_notification(to, event_type, payload)` — formats subject + plain-text body, logs at INFO (SMTP stub; replace with `lettre` when added)
  - `send_slack_notification(webhook_url, payload)` — posts `{"text": "..."}` JSON to Slack incoming-webhook URL; `send_slack_notification_with_retry` adds same backoff retry
  - `compute_hmac` pub free function — tested against OpenSSL reference vector
  - `channel_matches_event` — boolean event-filter predicate
  - 20 unit tests: HMAC determinism, known test vector, channel filtering, email/Slack formatting, backoff schedule, email dispatch stub
  - Module exported via `pub use notification_dispatch::NotificationDispatcher` in `lib.rs`
- **`crates/gateway/src/user_routes.rs`** — two new endpoints:
  - `GET /v1/user/transactions/export` (`RequireViewer`) — fetches up to 200 transactions via `db.list_transactions()`; builds RFC-4180 CSV with headers `id,type,chain,asset,amount,status,tx_hash,created_at`; returns `Content-Type: text/csv; charset=utf-8` + `Content-Disposition: attachment; filename="custody_transactions.csv"`
  - `GET /v1/user/deposit-address/{chain}/qr` (`RequireViewer`) — allocates/retrieves deposit address; returns `image/svg+xml` SVG showing address in two lines with chain label; placeholder note for future real QR matrix; `xml_attr_escape()` sanitises all user-supplied content before SVG embedding
  - `csv_escape(s)` — RFC-4180 quoting (wraps in `"..."`, doubles embedded quotes)
  - `build_address_svg(address, chain)` — 400x200 px SVG, monospace text, styled boxes; uses `r##"..."##` raw string to allow `#` hex colors without Rust 2024 lexer collision
  - `xml_attr_escape(s)` — escapes `& < > " '` for XML safety
  - Routes wired into `user_router()`
  - 15 new unit tests: CSV escape edge cases, column count, row format, SVG content, XML escaping, address split correctness

### Added (2026-03-27) — Phase 24: Swap Execution Engine

- **`crates/common/src/swap_executor.rs`** — new `SwapExecutor` trait + two implementations:
  - `SwapExecutor` trait: `quote()`, `execute()`, `status()` — chain-agnostic interface
  - `UniswapV3Executor` — Ethereum mainnet + Arbitrum configs; encodes `exactInputSingle` calldata via existing `encode_uniswap_v3_swap()`; validates slippage via `defi_execution::validate_slippage()`; produces 228-byte ABI-encoded calldata
  - `JupiterExecutor` — Solana; synchronous quote path + async `quote_async()` / `build_swap_tx_async()` for production use; returns base64 serialized transaction
  - `SwapQuote`, `SwapResult`, `SwapExecution`, `SwapProtocol`, `SwapExecutionStatus` types — all `Decimal`, no f64; full `serde` round-trip
  - `JupiterQuoteResponse`, `JupiterSwapTxResponse` — camelCase API types matching Jupiter V6 JSON
  - All amounts as `rust_decimal::Decimal`; `SwapExecution::from_quote_and_result()` for DB record creation
  - 29 unit tests covering: slippage boundaries, calldata length (228 bytes), quote expiry, chain rejection, EVM address parsing, u256 encoding, JSON round-trips
- **`crates/gateway/src/swap_routes.rs`** — 3 new API endpoints:
  - `POST /v1/swaps/quote` — get quote (RequireEditor); chain dispatch to Uniswap/Jupiter
  - `POST /v1/swaps` — execute quote → unsigned tx (RequireSigner); validates quote_id matches
  - `GET /v1/swaps/{id}` — swap status (RequireViewer); stub pending DB wiring
  - 13 unit tests: serialization, round-trips, chain rejection, slippage boundary tests
- **Module wiring**: added to `crates/common/src/lib.rs` and `crates/gateway/src/lib.rs`; `swap_router()` merged into main router in `routes.rs`

### Added (2026-03-27) — Phase 25: OpenAPI Spec Endpoint

- **`crates/gateway/src/openapi.rs`** — `generate_openapi_v3()` replacing the existing `developer::generate_openapi_spec()`:
  - Returns OpenAPI **3.1.0** spec (upgraded from 3.0.3)
  - Full schemas for core types: `CreateAddressRequest/Response`, `SubmitWithdrawalRequest/Response`, `WithdrawalStatusResponse`, `WalletBalanceResponse`, `Chain`, `WithdrawalStatus`
  - Full schemas for swap types: `SwapQuoteRequest` (with slippage min=1 max=200), `SwapQuote`, `SwapExecuteRequest/Response`, `SwapStatusResponse`, `SwapExecutionStatus`
  - Two security schemes: `bearerAuth` (HTTP Bearer) + `hmacAuth` (API key in header, HMAC-SHA256)
  - Amounts documented as `type: string` — explicit guard against float deserialization
  - 6 shared error response components (`BadRequest`, `Unauthorized`, `Forbidden`, `NotFound`, `Conflict`, `InternalError`)
  - 33 tags including new `swaps` tag
  - Remaining 100+ endpoints from developer catalog merged via `remaining_catalog_entries()`
  - `GET /v1/openapi.json` handler updated in `routes.rs` to use new generator
  - 22 unit tests: version, security schemes, schemas, swap endpoints, amount type enforcement
- **Module wiring**: added to `crates/gateway/src/lib.rs`

### Added (2026-03-27) — Phase 19A: RBAC Enforcement & Sessions (started)

- **DB migration 011: sessions** — sessions table (token_hash PK, user_id FK, IP/UA binding, absolute 8h + idle 30m timeouts, revocation)
  - password_hash column added to workspace_users
  - Session tokens stored as SHA-256 hashes (never plaintext)
- **Session DB layer** (`crates/db/src/sessions.rs`) — create/load/touch/revoke/cleanup sessions, user lookup by email
  - `generate_token()`: cryptographically random 32 bytes
  - `hash_token()`: SHA-256 for storage
  - `Session::is_valid()`: checks expiry, idle, revocation, user enabled
  - `Session::ip_matches()`: hijack detection
- **RBAC middleware** (`crates/gateway/src/rbac_middleware.rs`) — axum middleware + extractors
  - `rbac_auth`: extracts Bearer token → loads session → validates → injects AuthContext
  - `AuthUser`, `RequireViewer`, `RequireEditor`, `RequireSigner`, `RequireAdmin`, `RequireOwner` extractors
  - HMAC-authed requests (service-to-service) get Admin role by default
  - Default-deny: no auth context = rejected
- **Auth endpoints** (`crates/gateway/src/auth_routes.rs`) — login, list sessions, revoke
  - `POST /v1/auth/sessions` — email + password → Bearer token
  - `GET /v1/auth/sessions` — list active sessions
  - `DELETE /v1/auth/sessions/{token}` — revoke session
- **Permission guards wired on core endpoints**:
  - `create_address`: Editor+
  - `submit_withdrawal`: Editor+
  - `approve_withdrawal`: Signer+
  - `get_withdrawal_status`: Viewer+
  - `get_wallet_balance`: Viewer+
- **RBAC guards on ALL 125+ endpoints** across 11 route files:
  - vault_routes: Viewer (list/get), Editor (create account, transfer), Signer (raw-sign, cancel, accelerate), Admin (create/update/delete vault, manage assets)
  - ops_routes: Admin (rescan, consolidate), Editor (batch withdraw)
  - governance_routes: Viewer (list), Signer (approve), Admin (create/update policies)
  - operations_routes: Viewer (list), Admin (schedule, compliance config)
  - connectivity/platform/embedded/exchange/admin/settlement/staking routes
- **14 new tests** (1384 total, 0 failures)

### Added (2026-03-27) — Phase 22: AML/KYT + Phase 23: UTXO Consolidation

- **Elliptic Lens provider** (`EllipticProvider`) — full HTTP integration for wallet exposure + source-of-funds screening
  - POST `/v2/wallet/synchronous` for address screening
  - POST `/v2/txn/synchronous` for transaction screening
  - Risk score → RiskLevel mapping (0.3/0.6/0.8 thresholds)
  - Graceful degradation on API errors
- **ScreeningService** — multi-provider orchestration with worst-of aggregation
  - `AnyScreeningProvider` enum dispatch (avoids dyn-incompatible async traits)
  - Configurable fallback: `Block` (safe) or `AllowAndFlag` (operator choice)
  - Cache TTL configuration (default 24h)
  - `add_chainalysis()` / `add_elliptic()` builder pattern
- **UTXO Consolidation** (`crates/utxo/src/consolidation.rs`)
  - `ConsolidationPolicy`: min UTXO count (50), max per tx (20), max fee rate (10 sat/vB), schedule
  - `should_consolidate()`: checks UTXO count + fee environment + schedule window
  - `plan_consolidation()`: selects smallest UTXOs first, estimates fee + savings
  - Skips locked and unconfirmed UTXOs, validates destination is internal
- **12 new tests** (6 screening + 6 consolidation)

### Added (2026-03-27) — Phase 21A: Web Dashboard Infrastructure

- **SvelteKit dashboard** (`dashboard/`) — `adapter-static` compiles to pure HTML/CSS/JS, no Node.js in production
  - 9 pages: Dashboard, Wallets, Transactions, Approvals, Addresses, Policies, Users, Audit Log, Settings
  - Login page with email+password → Bearer token flow
  - TypeScript API client (`$lib/api.ts`) with auto 401 → redirect to login
  - Dark theme, sidebar navigation, CSS variables
- **axum static file serving** — `ServeDir` with `index.html` fallback for SPA routing
  - Configurable via `CUSTODY_DASHBOARD_DIR` env var (default: `dashboard/build`)
- **Security headers middleware** — CSP `default-src 'self'`, `X-Frame-Options: DENY`, HSTS 1yr, `nosniff`, `Referrer-Policy: same-origin`, `Permissions-Policy` (no camera/mic/geo)
- **Air-gap compatible** — zero CDN dependencies, all assets bundled

### Added (2026-03-27) — Phase 20: FIDO2/WebAuthn Verification

- **DB migration 012: fido2** — `fido2_credentials` (credential_id, public_key_cose, sign_count, aaguid, enabled) + `fido2_challenges` (hash, purpose, single-use, 5min expiry)
- **FIDO2 DB layer** (`crates/db/src/fido2.rs`) — credential CRUD, challenge create/consume (atomic), sign count monotonic update, cleanup
- **6 FIDO2 API endpoints** (`crates/gateway/src/fido2_routes.rs`):
  - `POST /v1/auth/fido2/register/begin` → challenge + options
  - `POST /v1/auth/fido2/register/complete` → verify challenge, store credential
  - `POST /v1/auth/fido2/authenticate/begin` → challenge + allowed credentials
  - `POST /v1/auth/fido2/authenticate/complete` → verify assertion, check sign count
  - `GET /v1/auth/fido2/credentials` → list user's hardware keys
  - `DELETE /v1/auth/fido2/credentials/{id}` → soft-disable credential (Admin only)
- **Security**: challenges SHA-256 hashed (never plaintext), single-use (consumed atomically), 5min expiry, sign count monotonic increment (cloned key detection)
- **7 new tests**

### Added (2026-03-27) — ROADMAP Expansion: Phases 19-25

- **Honest product gap assessment** — compared all features vs Fireblocks API
  - Identified 8 working features, 9 types-only, 3 completely missing
  - Overall product completeness: ~30-40% of Fireblocks feature set
- **Phase 19: RBAC Enforcement** — session management + authorization middleware (P0)
  - PostgreSQL sessions, `RbacMiddleware`, `require_permission()` extractor, default-deny
- **Phase 20: FIDO2/WebAuthn Verification** — real hardware key verification via `webauthn-rs` (P0)
  - Registration/authentication flows, Tier 2+ approval enforcement
- **Phase 21: Web Dashboard** — SvelteKit + `adapter-static`, served by axum (P0)
  - Technology decision: SvelteKit over React/Next.js (no Node.js in production, ~40KB bundle, air-gap compatible)
  - 9 pages: Dashboard, Wallets, Transactions, Approvals, Policies, Addresses, Users, Audit Log, Settings
  - Security: CSP `default-src 'self'`, no inline JS, HSTS, X-Frame-Options: DENY, bundled fonts/icons
- **Phase 22: AML/KYT Real Integration** — Chainalysis KYT v2 + Elliptic Lens HTTP calls (P1)
- **Phase 23: UTXO Consolidation Execution** — automated Bitcoin UTXO consolidation (P1)
- **Phase 24: Swap Execution Engine** — Uniswap V3 + Jupiter live swaps (P2)
- **Phase 25: OpenAPI Auto-Generation** — `utoipa` replacing hand-maintained spec (P2)
- **Dependency graph**: 19 → 20 → 21A → [22 + 23] → 21B/C/D → 24 → 25

### Added (2026-03-27) — Phase 18: Competitive Gap Closure

- **HsmSigningBackend (18A)** — encrypted seed on disk, on-demand decrypt→sign→zeroize per operation
  - Seed NOT held in memory between signing ops (unlike SoftwareSigningBackend)
  - HSM and Software backends produce identical signatures (parity test)
  - Operation counter for audit, wrong password rejected
  - 5 new tests, P0-2 gap closed
- **Generalized EVM Scanner (18B)** — works with any EVM chain from registry
  - Native token + ERC-20 Transfer event scanning (same as Ethereum scanner)
  - `from_chain_def()` factory creates scanner from EvmChainDef
  - `create_scanners_from_registry()` bulk creates for all enabled chains
  - Server wired: Arbitrum, Optimism, Base, Polygon L2 scanners active
  - 6 new tests, P1-1 gap partially closed
- **MPC HTTP Transport (18D)** — real network transport for MPC parties
  - `HttpTransport` implements `MpcTransport` via HTTP POST
  - HMAC-SHA256 authentication on every inter-party message
  - `deliver()` with auth verification + 1MB size limit
  - Polling `receive()` with configurable timeout
  - 8 new tests, P0-1 transport gap closed
- **Feature freeze lifted** — Phase 17 trust infrastructure complete

### Added (2026-03-27) — Phase 17 Complete
- **Circuit breaker restart persistence test** — integration test proving counters survive process restart
  - Records $5M outflow, drops DB connection, reconnects, verifies $5M still tracked
  - Tests that tripped state also survives restart
  - Tests that post-restart check correctly blocks over-limit withdrawals
- **Onboarding walkthrough** — `docs/ONBOARDING.md` with hands-on guides for 2 user archetypes
  - Archetype 1 (Crypto Startup): testnet setup, deposit detection, withdrawal, webhooks
  - Archetype 2 (Trading Desk): vault structure, approval workflows, raw signing, circuit breaker tuning
  - Migration checklist for Fireblocks/BitGo users
  - Common post-onboarding tasks (HMAC auth, diagnostics, monitoring)

### Added (2026-03-26) — IPC Signing Wired into Pipeline
- **SigningMode** in withdrawal pipeline — Direct (dev) or IPC (production)
  - Auto-detect from `SIGNER_SOCKET` + `SIGNER_IPC_SECRET` env vars
  - `get_public_key_ipc()`, `raw_sign_ipc()` convenience methods
  - Phase 17B complete: pipeline can now sign via isolated signer process

### Added (2026-03-26) — Expanded Security Tests (33 total)
- Address poisoning detection, error recovery classification
- Proof of Reserves solvency/insolvency, circuit breaker error messages
- Bitcoin dust detection, Solana tx size enforcement
- **9 new security tests** (33 attack simulation tests total, 1112 overall)

### Added (2026-03-26) — MPC Share Persistence
- **ShareStore trait** — persist key shares across process restarts
  - FileShareStore (dev), integrity verification (SHA-256 hash), zeroized DecryptedShare
  - Save/load/list/delete with file-based storage
- **7 new tests** (1103 total)

### Added (2026-03-26) — MPC Transport + HSM Guard
- **MPC network transport** — `MpcTransport` trait with HMAC-authenticated messaging
  - InMemoryTransport for testing, session/round-based message filtering
  - HMAC sign/verify on all inter-party messages, tamper detection
- **HSM runtime guard** — `assert_hsm_operational()` panics if HSM not wired
  - `check_hsm_health()` for monitoring, `is_operational()`/`hsm_type()` on trait
- **11 new tests** (1096 total)

### Added (2026-03-26) — EVM Access Lists + Address Poisoning Detection
- **EVM Access Lists (EIP-2930)** — gas savings estimation, storage slot computation
  - Balance/allowance mapping slot calculation (keccak256)
- **Address poisoning detection** — zero-value transfer attack prevention
  - Prefix+suffix matching, risk classification (None/Low/Medium/High)
  - Warning message generation for UI
- **15 new tests** (1085 total)

### Added (2026-03-26) — Transaction Simulation + Preflight Checks
- **Tx simulation** — eth_call, eth_estimateGas, SOL simulateTransaction, TRX triggerConstantContract
- **Revert decoder** — Error(string) and Panic(uint256) with known codes
- **Preflight checks** — empty tx, non-EIP-1559, size limits per chain
- **12 new tests** (1070 total)

### Added (2026-03-26) — Error Recovery Patterns
- **RPC error classification** — 11 error classes (connection, rate limit, nonce, gas, blockhash, etc.)
- **Recovery actions** — 7 strategies (retry, backoff, rebuild, replace, abort, manual, skip)
- **Stuck transaction detection** — per-chain thresholds (ETH 5min, BTC 1hr, SOL 90s, TRX 1min)
- **Recovery recommendations** — ETH/BTC: RBF after timeout, SOL: always rebuild, TRX: rebuild after 5min
- **16 new tests** (1058 total)

### Added (2026-03-26) — Quickstart + 80+ tokens + 17G
- **docs/QUICKSTART.md** — secure deployment in 30 minutes, 8-step guide
- **Token database expanded** — 80+ tokens (was 55): 24 new ERC-20 (DeFi, stablecoins, infrastructure) + 7 new SPL
  - New stablecoins: USDe, GHO, crvUSD, PYUSD, LUSD
  - New DeFi: DYDX, 1INCH, PENDLE, BLUR, ENA, EIGEN, ETHFI, SAFE
  - New infrastructure: W (Wormhole), ZRO (LayerZero), WLD (Worldcoin)
  - New Solana: ORCA, RENDER, W, TENSOR, HNT, MOBILE

### Changed (2026-03-26) — CI hardening + signer in Docker
- **CI pipeline** — pinned toolchain 1.85.0, added security job (cargo-deny + attack tests), signer binary build
- **All 9 migrations** run in CI integration tests (was only migration 001)
- **Dockerfile** — builds + includes custody-signer binary
- **RUSTFLAGS="-D warnings"** in CI

### Changed (2026-03-26) — Phase 17B client + 17G: IPC client, README, CONTRIBUTING
- **Signer IPC client** — `SignerClient` for main binary to talk to custody-signer
  - HMAC-authenticated requests, response verification
  - `get_public_key()`, `raw_sign()` convenience methods
- **README updated** — dev-mode warning, 1031 test count, security doc links
- **CONTRIBUTING.md updated** — security section, signer-service docs

### Added (2026-03-26) — Phase 17B: Signer Process Isolation
- **New crate `custody-signer-service`** — separate binary for signing operations
  - NO network listener, NO database, NO HTTP — Unix domain socket ONLY
  - HMAC-authenticated IPC protocol with constant-time verification
  - SignRequest/SignResponse with per-chain operation types
  - Supports GetPublicKey, RawSign for all 4 chains
  - Socket permissions 0600 (owner only), seed zeroized on drop
  - 7 protocol tests (HMAC roundtrip, wrong secret, tamper detection)

### Added (2026-03-26) — Phase 17E + 17F: Supply Chain + Deployment Modes
- **deny.toml** — cargo-deny config: license allow-list, RUSTSEC advisory check, source pinning
- **Dockerfile pinned** — `rust:1.85-bookworm` instead of `:latest`
- **docs/DEPLOYMENT_MODES.md** — 4 modes with honest security property tables
  - Mode 1 (Dev), Mode 2 (Production Minimal), Mode 3 (HSM), Mode 4 (MPC)
  - Each mode: what HOLDS and what DOES NOT

### Added (2026-03-26) — Phase 17D: Attack Simulation Tests (24 security tests)
- Key material never leaks to serialization, error types, or API responses
- Circuit breaker enforces all limits, webhook signatures not forgeable
- Address validation rejects dangerous inputs, token decimals verified

### Added (2026-03-26) — Phase 17A + 17C: Trust Infrastructure
- **THREAT_MODEL.md** — STRIDE framework analysis: 6 adversaries, 20+ attack vectors, honest mitigations
  - Assets, trust boundaries, circuit breaker attack scenario documented
  - What the system does NOT protect against (physical access, nation-state, smart contract bugs)
- **SECURITY.md** — architecture diagram, key material flow, authentication methods
  - Honest status of withdrawal approval tiers (FIDO2 = types only)
  - Deployment prerequisites with ❌ for missing items
- **Circuit breaker persistence (17C)** — CRITICAL fund safety fix
  - DB table `circuit_breaker_state` (migration 009)
  - Load counters on startup, atomic increment with withdrawal
  - Window rollover (hourly/daily), trip/reset with logging
  - No more counter reset on process restart

### Added (2026-03-26) — Level 29: 1000 Tests Milestone
- **Solana versioned transactions** — v0 with Address Lookup Tables, tx size estimation
  - Priority fee levels (None → VeryHigh), fee calculation in SOL
- **Bitcoin fee estimation** — mempool rate buckets, RBF bump calc, CPFP effective rate
  - Confirmation time estimation by fee rate
- **21 new tests** (1000 total!)

### Changed (2026-03-26) — Trust Pivot: Honest Statuses + Phase 17
- **ROADMAP.md overhaul** — project target rewritten from "Fireblocks feature parity" to "trustworthy self-hosted custody"
- **Honest status markers** — Phases 4C (HSM), 9, 10, 11, 13, 14, 15 re-marked as "Types Only / Not Operational"
- **Phase 4 MPC/HSM** — added missing tasks: network transport, share persistence, live HSM integration
- **Phase 17 — Trust Infrastructure** added as NEXT priority with 7 sub-phases:
  - 17A: THREAT_MODEL.md + SECURITY.md + honest README
  - 17B: Signer process isolation (separate binary, IPC)
  - 17C: Circuit breaker DB persistence (fund safety P0)
  - 17D: Attack simulation tests
  - 17E: Supply chain (pinned images, cargo-deny, SBOM)
  - 17F: Deployment modes document
  - 17G: Go-to-market foundation
- **Feature freeze** — no new chains/DeFi/types until Phase 17 complete
- **Parity Tracker** replaced with "Honest Status Tracker" (4 tiers: Operational / Crypto Only / Types Only / Missing)

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
