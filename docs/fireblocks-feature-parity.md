# Fireblocks Feature Parity Analysis

**Date**: 2026-03-25
**Source**: Deep research of Fireblocks developer docs, blog, API reference

## Priority Matrix: Self-Hosted Implementation

| Feature | Fireblocks | Complexity | Our Phase | Priority |
|---------|-----------|------------|-----------|----------|
| **MPC-CMP Signing** | 2-of-3 threshold, SGX, key refresh | XL | Phase 4 ✅ started | P0 |
| **TAP Policy Engine** | SGX-enforced, per-method rules | L | Phase 5 | P0 |
| **Gas Station** | Auto-refuel ERC-20 addresses | S | Phase 7 | P1 |
| **Address Whitelisting** | Admin quorum, OTA, rotation | M | Phase 5 | P1 |
| **AML/KYT (Chainalysis)** | Pre-signing screening | L | Phase 6 | P1 |
| **OFAC/Sanctions** | Real-time, multi-list | S | Phase 6 | P1 |
| **Travel Rule (Notabene)** | FATF Rec.16 compliance | L | Phase 6 | P1 |
| **RBAC + Admin Quorum** | Owner/Admin/Signer/Editor/Viewer | L | Phase 5 | P1 |
| **Staking API** | ETH/SOL/MATIC + validator marketplace | L | Phase 9 | P2 |
| **Smart Contract Interaction** | Deploy, call, ABI decode | M | Phase 10 | P2 |
| **API Co-Signer** | Callback handler, HA, JWT auth | M | Phase 4 | P2 |
| **WalletConnect/DeFi** | dApp browser, swap routing | L | Phase 10 | P2 |
| **Tokenization** | ERC-20/721/1155 lifecycle | L | Phase 14 | P3 |
| **Embedded Wallets** | 2-of-2 MPC, mobile SDKs | XL | Phase 15 | P3 |
| **Off Exchange** | CVA shared custody with exchanges | XL | Phase 13 | P4 |
| **Payment Network** | 40+ providers, 100+ countries | XL | Phase 13 | P4 |

MG Custody integrates with 21 Analytics for TRUST counterparty checks before broadcast, automatic dispatch after broadcast, and receipt tracking. [MG Custody setup](../README.md#21-analytics-trust)

## Key Fireblocks Technical Details

### MPC-CMP
- Open-source: github.com/fireblocks/mpc-lib (C++, no Rust bindings)
- 1-round signing (800% faster than GG18/GG20)
- Proactive key refresh at minute-level intervals
- 3 share types: Hot (API co-signer), Warm (mobile), Cold (air-gapped)
- Multi-cloud: shares across AWS + Azure + GCP + on-prem

### TAP Policy Engine
- Rule evaluation: top-down, first-match-wins
- Parameters: initiator, source, destination, asset, amount, operation type, time
- Can target specific smart contract methods
- Admin quorum required for policy changes
- SGX-enforced — even Fireblocks employees can't bypass

### API Co-Signer
- Holds one MPC key share in customer infra
- Callback handler: JWT-encoded signing requests → approve/reject
- Plugin architecture: github.com/fireblocks/plugin-based-callback-handler
- HA: multiple co-signers for redundancy

### Gas Station
- Trigger: token deposit/withdrawal detected
- 3 params: gasThreshold, gasCap, maxGasPrice
- Gas Station 2.0: designated vault as gas source
- EVM chains only

### Chains
- 150+ chains, 46 new in 2025
- EVM addition: config + RPC + chain ID (days)
- Non-EVM addition: full adapter (weeks-months)

## Our Advantages vs Fireblocks
1. **No vendor lock-in** — keys, code, data all ours
2. **Data sovereignty** — nothing leaves our infra
3. **Cost** — no basis-point fees on AUM
4. **Customization** — any business logic, any integration
5. **Transparency** — full source, auditable

## Our Disadvantages vs Fireblocks
1. **No settlement network** (2400+ counterparties)
2. **No insurance program** ($1B coverage)
3. **No SGX enclaves** (hardware TEE)
4. **4 chains vs 150+**
5. **No certifications** (SOC 2, ISO 27001, CCSS)
6. **Single-team maintenance** vs 100+ engineers
