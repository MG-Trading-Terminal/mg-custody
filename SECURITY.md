# Security Architecture

> MG Custody — Self-Hosted Cryptocurrency Custody Service

## Architecture Overview

```
                    ┌─────────────┐
                    │   Exchange   │
                    │  (webhook)   │
                    └──────┬──────┘
                           │ HMAC-SHA256
                    ┌──────▼──────┐
                    │ API Gateway  │ :8443 (axum)
                    │ Rate Limited │ 100 req/min
                    │ HMAC Auth    │
                    └──────┬──────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
    ┌─────────▼──┐  ┌──────▼─────┐  ┌──▼──────────┐
    │ Withdrawal │  │  Chain     │  │ Wallet      │
    │ Pipeline   │  │  Scanners  │  │ Manager     │
    │            │  │  (4 chains)│  │ (tier mgmt) │
    └─────┬──────┘  └────────────┘  └─────────────┘
          │
    ┌─────▼──────┐
    │  Signing   │ ← CURRENTLY in same process (NOT isolated)
    │  Backend   │
    │ (software) │
    └────────────┘
          │
    ┌─────▼──────┐
    │ PostgreSQL │ 30 tables, 8 migrations
    │            │ All state persisted
    └────────────┘
```

## Current Security Properties (HONEST)

### What DOES work:
- **HMAC-SHA256 API authentication** with 30-second replay window
- **Encrypted seed storage** (AES-256-GCM, PBKDF2 600K iterations)
- **Zeroize on all key types** (ZeroizeOnDrop for seeds, private keys, shares)
- **Key material never in logs** (tracing filters, redacted Debug impl)
- **Double-entry ledger** (every fund movement has debit + credit pair)
- **Idempotent API endpoints** (duplicate request = same response)
- **Rate limiting** (token bucket per client IP)
- **OFAC sanctions screening** (hardcoded + configurable addresses)
- **Withdrawal validation** (address format, amount, chain match before signing)
- **Audit trail** with SHA-256 hash chain (tamper-evident)
- **Circuit breaker limits** ($500K/tx, $10M/hour, $50M/day)

### What does NOT work (yet):
- **Circuit breaker counters reset on restart** — CRITICAL, Phase 17C
- **Signer not isolated** — single binary, API compromise = key access, Phase 17B
- **HSM returns unimplemented!()** — software keys only, Phase 4C incomplete
- **MPC has no transport** — algorithms work, but no network between parties
- **No reproducible builds** — Dockerfile uses `:latest`, no SBOM
- **No deployment guide** — operator doesn't know which properties hold

## Key Material Flow

```
Seed Generation (BIP-39 compatible)
    │
    ├── Encrypted at rest (AES-256-GCM)
    │   └── PBKDF2-HMAC-SHA256, 600K iterations
    │
    ├── Shamir 3-of-5 backup
    │   └── GF(256) polynomial, all 10 combos verified
    │
    └── Runtime derivation
        ├── BIP-32/44 → secp256k1 (BTC m/84', ETH m/44'/60', TRX m/44'/195')
        └── SLIP-0010 → Ed25519 (SOL m/44'/501')
```

**Key material lifecycle:**
1. Generated from CSPRNG entropy
2. Immediately encrypted with AES-256-GCM
3. Split into Shamir shares for backup
4. Loaded into memory on startup (decrypted)
5. Derived keys exist only during signing operation
6. Derived keys Zeroized immediately after signing
7. Master seed persists in memory for the process lifetime

**WARNING**: While the master seed is in process memory, it is vulnerable to:
- Memory dumps (core dump, cold boot attack)
- Process compromise (same binary as API gateway)
- Debugger attachment

These are mitigated by HSM (not yet operational) and signer isolation (not yet implemented).

## Authentication

| Endpoint | Auth Method | Details |
|----------|-----------|---------|
| `/health*` | None | Public health checks |
| `/v1/*` | HMAC-SHA256 | method + path + timestamp + body |
| `/admin/*` | X-Admin-Key + HMAC | Separate admin key, 20 req/min |
| Webhooks (outbound) | HMAC-SHA256 | Per-endpoint secret |

## Withdrawal Approval Tiers

| Amount | Approval | Delay | Status |
|--------|----------|-------|--------|
| < $1K + whitelisted | Auto | 1-5 min | ✅ Implemented |
| $1K - $10K | Auto + alert | 10 min | ✅ Implemented |
| $10K - $100K | 1 approver (FIDO2) | 30 min | ⚠️ FIDO2 types only |
| $100K - $500K | 2 approvers | 2 hours | ⚠️ Approval types only |
| > $500K | 3 approvers + cold | 24 hours | ⚠️ Types only |

## Reporting Security Issues

If you discover a security vulnerability, please report it privately:
- DO NOT open a public GitHub issue
- Email: security@mg-custody.dev (placeholder)
- Include: description, reproduction steps, impact assessment
- Response time: 48 hours for acknowledgment, 7 days for fix timeline

## Deployment Prerequisites

Before deploying in production:
1. ✅ Change all default keys/passwords
2. ✅ Set `CUSTODY_ADMIN_KEY` environment variable
3. ✅ Configure HMAC secret for API authentication
4. ❌ Deploy signer as separate binary (Phase 17B — NOT YET AVAILABLE)
5. ❌ Configure HSM for key storage (Phase 4C — NOT YET AVAILABLE)
6. ❌ Verify circuit breaker persistence (Phase 17C — NOT YET AVAILABLE)
7. ⚠️ Review and test backup/recovery procedure
8. ⚠️ Configure monitoring and alerting channels
