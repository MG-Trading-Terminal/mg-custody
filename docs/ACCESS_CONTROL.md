# Access Control Matrix

> Role hierarchy: **Owner > Admin > Signer > Editor > Viewer**
> Each role includes all permissions of roles below it.
> Default-deny: no AuthContext = rejected (401/403).

## Role Descriptions

| Role | Purpose | Can do |
|------|---------|--------|
| **Viewer** | Read-only monitoring | View balances, transactions, addresses, health |
| **Editor** | Operational use | Create addresses, submit withdrawals, manage address labels |
| **Signer** | Approve/sign transactions | Approve Tier 2+ withdrawals, sign raw messages, execute swaps |
| **Admin** | System management | Manage users, webhooks, policies, tokens, emergency freeze |
| **Owner** | Full control | All of the above + role promotion, force-override withdrawals |

## Endpoint Permissions

### Core API (routes.rs)

| Method | Path | Min Role | Description |
|--------|------|----------|-------------|
| POST | /v1/addresses | Editor | Generate deposit address |
| POST | /v1/withdrawals | Editor | Submit withdrawal request |
| GET | /v1/withdrawals/{id} | Viewer | Check withdrawal status |
| POST | /v1/withdrawals/{id}/approve | Signer | Approve/reject withdrawal (FIDO2 for $10K+) |
| GET | /v1/wallets/{asset} | Viewer | View wallet balance |
| GET | /health | Public | Health check (no auth) |
| GET | /health/chains | Public | Chain health (no auth) |
| GET | /metrics | Public | Prometheus metrics (no auth) |
| GET | /v1/openapi.json | Public | OpenAPI spec (no auth) |

### User Endpoints (user_routes.rs)

| Method | Path | Min Role | Description |
|--------|------|----------|-------------|
| GET | /v1/user/portfolio | Viewer | Portfolio overview |
| GET | /v1/user/activity | Viewer | Activity feed |
| GET | /v1/user/deposit-address/{chain} | Viewer | Get deposit address |
| GET | /v1/user/deposit-address/{chain}/qr | Viewer | QR code for address |
| GET | /v1/user/deposit-history | Viewer | Deposit history |
| GET | /v1/user/withdrawal-history | Viewer | Withdrawal history |
| POST | /v1/user/withdrawals/estimate | Viewer | Fee estimation |
| GET | /v1/user/profile | Viewer | User profile |
| POST | /v1/user/password | Editor | Change password |
| GET | /v1/user/proof-of-reserves | Viewer | Proof of Reserves |
| POST | /v1/user/proof-of-reserves/verify | Viewer | Verify PoR proof |

### Vault Management (vault_routes.rs)

| Method | Path | Min Role |
|--------|------|----------|
| GET | /v1/vaults | Viewer |
| POST | /v1/vaults | Admin |
| GET | /v1/vaults/{id} | Viewer |
| PUT | /v1/vaults/{id} | Admin |
| GET | /v1/vaults/{id}/accounts | Viewer |
| POST | /v1/vaults/{id}/accounts | Editor |
| POST | /v1/vaults/{id}/transfer | Signer |
| POST | /v1/admin/tokens/import | Admin |

### Webhook Management (webhook_mgmt_routes.rs)

| Method | Path | Min Role |
|--------|------|----------|
| GET | /v1/admin/webhooks | Admin |
| POST | /v1/admin/webhooks | Admin |
| PUT | /v1/admin/webhooks/{id} | Admin |
| DELETE | /v1/admin/webhooks/{id} | Admin |
| POST | /v1/admin/webhooks/{id}/test | Admin |
| GET | /v1/admin/webhooks/metrics | Admin |

### Emergency & Override (webhook_mgmt_routes.rs)

| Method | Path | Min Role |
|--------|------|----------|
| POST | /v1/admin/emergency/freeze | Owner |
| POST | /v1/admin/emergency/unfreeze | Owner |
| GET | /v1/admin/emergency/status | Admin |
| POST | /v1/admin/override/withdrawal/{id}/force-complete | Owner |
| POST | /v1/admin/override/withdrawal/{id}/force-cancel | Owner |

### Swap Execution (swap_routes.rs)

| Method | Path | Min Role |
|--------|------|----------|
| POST | /v1/swaps/quote | Editor |
| POST | /v1/swaps | Signer |
| GET | /v1/swaps/{id} | Viewer |

### Authentication (auth_routes.rs + fido2_routes.rs + totp_routes.rs)

| Method | Path | Auth |
|--------|------|------|
| POST | /v1/auth/sessions | Public (login) |
| POST | /v1/auth/sessions/verify-2fa | Public (TOTP step) |
| DELETE | /v1/auth/sessions/{token} | Viewer |
| POST | /v1/auth/fido2/register/begin | Viewer |
| POST | /v1/auth/fido2/register/complete | Viewer |
| POST | /v1/auth/fido2/authenticate/begin | Viewer |
| POST | /v1/auth/totp/setup | Viewer |
| POST | /v1/auth/totp/verify | Viewer |

## Service-to-Service Auth

Exchange → Custody communication uses HMAC-SHA256:
- Format: `HMAC(method + path + timestamp + body)`
- Header: `X-Custody-Signature` + `X-Custody-Timestamp`
- Grants **Admin** role via RBAC fallback (trusted internal service)
- Stale requests (>30s) rejected

## Audit Trail

All permission denials are logged to the tamper-evident audit trail:
- `PermissionDenied` audit operation
- Includes: actor email, required role, actual role, HTTP method + path
- Non-blocking write via `tokio::spawn`
- Prometheus counter: `custody_permission_denials_total`
