# Deployment Modes

> Each mode has specific security properties that HOLD and those that DO NOT.
> Choose the mode that matches your threat model.

## Mode 1: Development

**Single process, software keys, no isolation.**

```
┌──────────────────────┐
│   custody (single)   │
│  API + Scanner +     │
│  Pipeline + Signer   │
│  + Software Keys     │
└──────────┬───────────┘
           │
     ┌─────▼─────┐
     │ PostgreSQL │
     └───────────┘
```

### Security Properties
| Property | Status |
|----------|--------|
| Encrypted seed at rest | ✅ AES-256-GCM |
| HMAC-authenticated API | ✅ |
| Rate limiting | ✅ |
| Circuit breaker (DB-persisted) | ✅ |
| Audit trail | ✅ |
| Key zeroization on drop | ✅ |
| **Signer isolation** | ❌ Same process |
| **HSM key storage** | ❌ Software only |
| **MPC threshold signing** | ❌ Not wired |
| **Memory protection** | ❌ Swappable, dumpable |

### Use Case
- Local development and testing
- CI/CD pipeline testing
- **DO NOT use for real funds**

### Quick Start
```bash
cargo run --release --bin custody
```

---

## Mode 2: Production Minimal

**Split signer binary, encrypted storage, separate host possible.**

> ⚠️ Signer isolation (Phase 17B) is not yet implemented. This mode describes the target architecture.

```
┌──────────────┐    IPC     ┌──────────────┐
│ custody      │◄──────────►│ custody-signer│
│ (API +       │  Unix sock │ (signing only)│
│  scanners)   │            │ No network    │
└──────┬───────┘            │ No DB access  │
       │                    └───────────────┘
 ┌─────▼─────┐
 │ PostgreSQL │
 └───────────┘
```

### Additional Properties over Mode 1
| Property | Status |
|----------|--------|
| Signer in separate process | ✅ (when 17B done) |
| API compromise ≠ key access | ✅ (when 17B done) |
| Signer has no network | ✅ (when 17B done) |

### Use Case
- Small exchanges, trading desks
- Testnet with real architecture
- Funds < $1M

---

## Mode 3: Production HSM

**Mode 2 + hardware key storage.**

> ⚠️ HSM integration (Phase 4C) returns `unimplemented!()`. This mode is aspirational.

```
┌──────────────┐    IPC     ┌──────────────┐
│ custody      │◄──────────►│ custody-signer│
│ (API)        │            │ + HSM PKCS#11 │
└──────┬───────┘            └──────┬────────┘
       │                           │
 ┌─────▼─────┐            ┌───────▼───────┐
 │ PostgreSQL │            │ SoftHSM2 or   │
 └───────────┘            │ Thales Luna   │
                          └───────────────┘
```

### Additional Properties over Mode 2
| Property | Status |
|----------|--------|
| Keys never leave HSM | ✅ (when 4C done) |
| Physical tamper resistance | ✅ Hardware-enforced |
| Memory dump safe | ✅ Keys in HSM, not RAM |

### Use Case
- Production with real funds
- Regulated entities
- Funds $1M - $100M

---

## Mode 4: Production MPC

**Mode 3 + distributed key ceremony across 3+ hosts.**

> ⚠️ MPC transport (Phase 4 network) is not implemented. Algorithms work but have no inter-node communication.

```
┌──────────────┐
│ custody      │
│ (coordinator)│
└──────┬───────┘
       │ gRPC/Noise
  ┌────┼────┐
  │    │    │
┌─▼─┐┌─▼─┐┌─▼─┐
│ S1 ││ S2 ││ S3 │  3 signer nodes
│HSM ││HSM ││HSM │  each with HSM
└────┘└────┘└────┘
  2-of-3 threshold
```

### Additional Properties over Mode 3
| Property | Status |
|----------|--------|
| No single point of key compromise | ✅ 2-of-3 threshold |
| Geographic distribution | ✅ Nodes in different DCs |
| Key ceremony required | ✅ Auditable process |
| Institutional grade | ✅ Fireblocks equivalent |

### Use Case
- Institutional custody
- Funds > $100M
- Regulatory compliance required

---

## Choosing a Mode

| Question | Mode 1 | Mode 2 | Mode 3 | Mode 4 |
|----------|--------|--------|--------|--------|
| Real funds? | ❌ | ⚠️ Small | ✅ | ✅ |
| Regulated? | ❌ | ❌ | ⚠️ | ✅ |
| Insider threat? | ❌ | ⚠️ | ⚠️ | ✅ |
| Physical attack? | ❌ | ❌ | ✅ | ✅ |
| Implementation status | ✅ Ready | 🔄 Phase 17B | 🔄 Phase 4C | 🔄 Phase 4 net |
