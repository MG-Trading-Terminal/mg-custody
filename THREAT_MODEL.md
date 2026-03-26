# Threat Model

> MG Custody — Self-Hosted Cryptocurrency Custody Service
> Framework: STRIDE + OWASP + CCSS

## 1. Assets Under Protection

| Asset | Sensitivity | Impact if Compromised |
|-------|-----------|----------------------|
| Private keys (BIP-32 seeds) | CRITICAL | **Total, irreversible loss of all funds** |
| MPC key shares | CRITICAL | Partial key compromise (threshold-dependent) |
| Encrypted seed file | HIGH | Loss if passphrase also compromised |
| Shamir shares (3-of-5) | HIGH | Single share useless; 3+ = full compromise |
| Database (PostgreSQL) | HIGH | Ledger manipulation, withdrawal injection |
| HMAC signing keys | HIGH | API impersonation, unauthorized withdrawals |
| API keys | MEDIUM | Scoped access depending on permissions |
| Configuration files | MEDIUM | Endpoint/limit changes, chain misconfiguration |
| Audit logs | MEDIUM | Cover attack tracks |

## 2. Trust Boundaries

```
┌─────────────────────────────────────────────┐
│  Zone 1: DMZ (Internet-facing)              │
│  ┌──────────────┐                           │
│  │ API Gateway  │ ← HMAC-authenticated only │
│  │ (axum)       │                           │
│  └──────┬───────┘                           │
├─────────┼───────────────────────────────────┤
│  Zone 2: Internal Network                   │
│  ┌──────┴───────┐  ┌──────────────┐        │
│  │ Withdrawal   │  │ Chain        │        │
│  │ Pipeline     │  │ Scanners     │        │
│  ├──────────────┤  ├──────────────┤        │
│  │ Wallet Mgr   │  │ Broadcaster  │        │
│  ├──────────────┤  ├──────────────┤        │
│  │ PostgreSQL   │  │ RPC Nodes    │        │
│  └──────────────┘  └──────────────┘        │
├─────────────────────────────────────────────┤
│  Zone 3: Air-gapped (ASPIRATIONAL)         │
│  ┌──────────────┐                           │
│  │ Signing      │ ← SHOULD be separate     │
│  │ Service      │   binary, currently NOT   │
│  ├──────────────┤                           │
│  │ HSM / MPC    │ ← Types only, not wired  │
│  └──────────────┘                           │
└─────────────────────────────────────────────┘
```

**CURRENT REALITY**: Zone 3 does not exist. All components run in a single `custody` binary in a single process. API compromise = signer compromise.

## 3. Adversaries

| Adversary | Capability | Motivation | Threat Level |
|-----------|-----------|------------|-------------|
| External attacker | Network access, public API | Fund theft | CRITICAL |
| Compromised API key | Authenticated API access | Unauthorized withdrawals | HIGH |
| Malicious insider | Source code + DB access | Key exfiltration | HIGH |
| Supply chain attack | Malicious dependency | Backdoor / key leak | HIGH |
| Compromised RPC node | False chain data | Double-spend, fake deposits | MEDIUM |
| Physical access | Server hardware | Cold boot, disk forensics | MEDIUM |

## 4. Attack Vectors (STRIDE)

### Spoofing
| Attack | Mitigation | Status |
|--------|-----------|--------|
| Forge API request | HMAC-SHA256(method+path+timestamp+body) | ✅ Implemented |
| Replay API request | 30-second timestamp window | ✅ Implemented |
| Impersonate admin | X-Admin-Key header | ✅ Implemented |
| Forge webhook callback | HMAC on outbound webhooks | ✅ Implemented |

### Tampering
| Attack | Mitigation | Status |
|--------|-----------|--------|
| Modify withdrawal destination | Pre-sign validation (address matches record) | ✅ Implemented |
| Modify withdrawal amount | Pre-sign validation (amount matches record) | ✅ Implemented |
| Tamper audit log | SHA-256 hash chain on entries | ✅ Implemented |
| Modify DB records directly | DB credentials separate from app | ⚠️ Same binary, same env |

### Repudiation
| Attack | Mitigation | Status |
|--------|-----------|--------|
| Deny signing request | Audit log with hash chain | ✅ Implemented |
| Deny withdrawal approval | Approval decisions stored with timestamp | ✅ Implemented |

### Information Disclosure
| Attack | Mitigation | Status |
|--------|-----------|--------|
| Key material in logs | Tracing filters, Zeroize on key types | ✅ Implemented |
| Key material in DB | Encrypted seed storage (AES-256-GCM) | ✅ Implemented |
| Key material in memory | `Zeroize` + `ZeroizeOnDrop` on all key types | ✅ Implemented |
| Key in API response | Key fields excluded from serialization | ✅ Implemented |
| Key in error messages | Error types never include key data | ✅ Implemented |
| Memory dump (cold boot) | NOT mitigated — requires HSM | ❌ Not mitigated |

### Denial of Service
| Attack | Mitigation | Status |
|--------|-----------|--------|
| API flood | Rate limiting (token bucket, 100 req/min) | ✅ Implemented |
| Large request body | Request timeout (30s) | ✅ Implemented |
| Slow loris | axum + tower timeout layer | ✅ Implemented |
| DB exhaustion | Connection pool limits (max 20) | ✅ Implemented |

### Elevation of Privilege
| Attack | Mitigation | Status |
|--------|-----------|--------|
| API key scope bypass | Per-key permission enforcement | ✅ Implemented |
| Admin API without auth | X-Admin-Key required if env var set | ✅ Implemented |
| Vault access cross-tenant | Workspace isolation check | ⚠️ Types only |
| Signer compromise via API | Zone 3 isolation | ❌ NOT IMPLEMENTED |

## 5. Circuit Breaker Threat

**CRITICAL**: Circuit breaker counters are in-memory only. Process restart = counter reset.

**Attack scenario**:
1. Attacker compromises API key with withdrawal permission
2. Submits $9M in withdrawals (under $10M hourly limit)
3. Causes process restart (crash, OOM, or deployment)
4. Submits another $9M (counter is zero again)
5. Repeat

**Required fix**: Phase 17C — persist counters in PostgreSQL, load on startup.

## 6. What This System Does NOT Protect Against

- **Physical access to signing host** — requires HSM (Phase 4C incomplete)
- **Compromised build environment** — no reproducible builds yet (Phase 17E)
- **Single operator going rogue** — requires MPC with 3+ independent parties (Phase 4 transport missing)
- **Nation-state adversary** — out of scope for self-hosted custody
- **Smart contract vulnerabilities** — custody signs transactions, doesn't audit contracts
- **Blockchain consensus attacks** — 51% attacks, chain reorganizations beyond confirmation depth
- **Social engineering** — operator training is out of scope

## 7. Priority Mitigations (Phase 17)

1. **P0**: Persist circuit breaker state in DB (17C)
2. **P0**: Extract signer into separate binary (17B)
3. **P1**: Attack simulation tests (17D)
4. **P1**: Supply chain pinning (17E)
5. **P2**: Deployment modes documentation (17F)
