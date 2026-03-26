# MG Custody — Positioning

## One-Line

**Open-source custody engine for self-hosted cryptocurrency key management.**

## What It Is

A Rust-based custody service that manages private keys, signs transactions, monitors deposits, and processes withdrawals across Bitcoin, Ethereum, Tron, and Solana. Self-hosted alternative to Fireblocks, BitGo, and Copper.co.

## Who It's For

### Archetype 1: Crypto Startup (< $10M assets)

**Situation**: Building an exchange, neobank, or DeFi protocol. Need custody but can't afford Fireblocks ($50K+/year). Don't trust cloud custody with keys.

**What they get**: Full custody stack with real signing, scanning, and withdrawal processing. 125+ API endpoints. Start in dev mode, graduate to HSM as volume grows.

**Key selling point**: No vendor lock-in. No monthly fees. Audit the code yourself.

### Archetype 2: Trading Desk ($10M-$500M assets)

**Situation**: Running OTC or prop trading. Currently using Fireblocks or BitGo. Want to reduce custody costs and have more control over signing latency.

**What they get**: Same API surface as Fireblocks (vaults, raw signing, batch withdrawals, approval workflows). Deploy on own infrastructure for lower latency and zero per-transaction fees.

**Key selling point**: Fireblocks-equivalent API. Migration path from cloud custody. Data sovereignty.

## Why Not Just Use Fireblocks?

| | Fireblocks | MG Custody |
|---|---|---|
| Cost | $50K-$500K/year + per-tx fees | Free (self-hosted) |
| Data sovereignty | They hold your data | You hold everything |
| Vendor lock-in | High (proprietary API) | None (open source) |
| Audit | Request audit report | Read the code |
| Customization | Limited | Full |
| Chains | 80+ | 4 (+ config for 23) |
| Insurance | $30M+ | None (your responsibility) |
| MPC | Production MPC-CMP | MPC algorithms work, transport in progress |
| Support | 24/7 enterprise | Community / self-support |

## Why Not Build From Scratch?

- **1112 tests** proving correctness of signing, key derivation, and security
- **Real cryptography**: BIP-143 (Bitcoin), EIP-1559 (Ethereum), SLIP-0010 (Solana)
- **Security architecture**: threat model, attack simulation tests, circuit breakers
- **Production patterns**: double-entry ledger, idempotent APIs, webhook retry
- **Compliance ready**: OFAC screening, Travel Rule types, approval tiers

Building this from scratch takes 12-18 months for a team of 3-5 engineers.

## Current Limitations (Honest)

- Signer isolation binary exists but isn't wired into main pipeline yet
- HSM adapters are stubs (software keys only for now)
- MPC algorithms work but no network transport between parties
- 4 chains operational (BTC/ETH/TRX/SOL), rest are config-only
- No insurance, no SOC 2 certification
- Single-operator: no distributed consensus on signing

## Competitive Landscape

| Provider | Model | Chains | MPC | Open Source |
|----------|-------|--------|-----|-------------|
| Fireblocks | SaaS | 80+ | ✅ CMP | ❌ |
| BitGo | SaaS/Self | 700+ tokens | ✅ | ❌ |
| Copper.co | SaaS | 40+ | ✅ | ❌ |
| Cobo | SaaS | 80+ | ✅ | ❌ |
| **MG Custody** | **Self-hosted** | **4 (+ 23 config)** | **⚠️ Algorithms only** | **✅** |

## Getting Started

```bash
git clone <repo>
cd mg-custody
cargo build --release
# See docs/QUICKSTART.md for full setup
```
