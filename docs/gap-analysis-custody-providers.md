# Institutional Custody Provider Gap Analysis

**Date**: 2026-03-25
**Scope**: Feature comparison of top 5 institutional custody providers vs. MG Custody (self-hosted)

The MG Custody Travel Rule assessment was updated on 2026-11-26. The rest of the comparison reflects the original snapshot.

---

## 1. Provider Feature Matrix

### 1.1 Key Management

| Feature | Fireblocks | BitGo | Copper.co | Cobo | Liminal | MG Custody |
|---------|-----------|-------|-----------|------|---------|------------|
| **Primary approach** | MPC (TSS) | Multi-sig + MPC | MPC (2-of-3) | MPC + Smart Contract | MPC + HSM | HD wallet (single seed) |
| **Key sharding** | MPC-CMP across 3+ parties | 2-of-3 / 3-of-5 multi-sig | 3 shards, 2-of-3 quorum | Flexible M-of-N MPC | MPC + HSM hybrid | Shamir 3-of-5 (backup only) |
| **HSM support** | SGX enclaves for policy engine | HSM for cold storage keys | HSM integration | Optional HSM | Securosys HSM Vaults (2026) | Stub only (PKCS#11 placeholder) |
| **Key recovery** | MPC share rotation + backup | Multi-sig redundancy | 3-party shard recovery | M-of-N recovery configs | Policy-based shard recovery | Shamir 3-of-5 seed reconstruction |
| **Policy engine isolation** | SGX enclave (tamper-proof) | Server-side enforcement | MPC-integrated | Server-side + smart contract | HSM-enforced authorization | None (application-level only) |

### 1.2 Chain & Asset Support

| Feature | Fireblocks | BitGo | Copper.co | Cobo | Liminal | MG Custody |
|---------|-----------|-------|-----------|------|---------|------------|
| **Blockchains** | 150+ | 50+ | 10+ (DeFi chains) | 80+ | 21+ | 4 (BTC, ETH, TRX, SOL) |
| **Tokens** | Thousands | 700+ | Hundreds | 3,000+ | Hundreds | 4 (ETH, USDT, USDC native) |
| **New chain onboarding** | Self-service SDK | Request-based | Request-based | Developer API | Request-based | Manual crate implementation |
| **NFT/RWA support** | Yes | Yes | Limited | Yes | No | No |

### 1.3 Policy Engine & Governance

| Feature | Fireblocks | BitGo | Copper.co | Cobo | Liminal | MG Custody |
|---------|-----------|-------|-----------|------|---------|------------|
| **Approval workflows** | TAP: multi-level quorum, per-asset/amount/destination rules | Role-based, configurable quorum | Exchange-integrated via ClearLoop | Role-based (Admin/Spender/Approver/Operator) | Firewall + HSM-verified auth chain | 5-tier amount-based (auto-approve only) |
| **Address whitelisting** | Internal/External/Contract wallet categories | Yes, with approval | Yes | Yes | Yes, with screening | Not implemented |
| **Velocity limits** | Time-period accumulation rules | Configurable per-wallet | Per-ClearLoop settlement | Transfer limits per policy | Configurable | Per-tx/hourly/daily USD (basic) |
| **Smart contract interaction rules** | TAP rules per contract method | Limited | ClearLoop scope | Smart contract wallet native | Limited | Not applicable |
| **Admin quorum for policy changes** | Yes (SGX-enforced) | Yes | Yes | Yes | Yes (HSM-verified) | No |
| **API-driven policy management** | Full API (Configure Policies endpoint) | Full API | API available | Full API | API available | No policy API |

### 1.4 Compliance & AML

| Feature | Fireblocks | BitGo | Copper.co | Cobo | Liminal | MG Custody |
|---------|-----------|-------|-----------|------|---------|------------|
| **AML/KYT screening** | Integrated (Chainalysis, Elliptic) | Integrated | Integrated | Chainalysis + Elliptic | TRM Labs | Not implemented |
| **Travel Rule** | Notabene/TRISA integration | Supported | Notabene partnership | Supported | Built-in checks | 21 Analytics (TRUST) |
| **OFAC/sanctions screening** | Real-time | Real-time | Real-time | Real-time | Real-time | Planned (Phase 4) |
| **Regulatory licenses** | NYDFS Trust, MiCA-aligned | OCC national bank charter, BaFin (MiCA), Dubai VASP | Swiss registration | SOC 2 Type II, ISO 27001 | CCSS Level 3, SOC 2 Type 2, ISO 27001/27701 | None |
| **Audit certifications** | SOC 2, NIST CSF 2.0 alignment | SOC 2 Type 2 | SOC 2 Type 2 | SOC 2 Type II, ISO 27001 | SOC 2 Type 2, CCSS Level 3, ISO 27001/27701 | None |
| **Proof of Reserves** | Supported | Supported | Via trust structure | Supported | Not specified | Planned (Phase 5) |

With 21 Analytics enabled, MG Custody checks the TRUST counterparty before broadcast for withdrawals subject to its Travel Rule gate. It sends automatically after broadcast, when the on-chain transaction ID required by TRUST is available. The custody API reports dispatch status and the receipt ID. Operators can look up counterparties and retrieve beneficiary data through `custody-admin trust`. [MG Custody setup and supported assets](../README.md#21-analytics-trust) · [21 Analytics outgoing workflow](https://docs.21analytics.ch/8.4.0/api/outgoing_integration.html)

### 1.5 DeFi & Staking

| Feature | Fireblocks | BitGo | Copper.co | Cobo | Liminal | MG Custody |
|---------|-----------|-------|-----------|------|---------|------------|
| **Native staking** | Staking API (multi-chain) | ETH/BTC staking | SOL/DOT staking, more planned | In-wallet staking | Multi-protocol dashboard | Not implemented |
| **DeFi access** | DeFi suite (Swaps, Token Allowance Manager) | Limited | 15,000+ dApps across 10 chains | Smart Contract Wallet native | Limited | Not implemented |
| **DeFi security** | Whitelist interfaces, monitor approvals | N/A | ClearLoop settlement | Policy engine for DeFi txs | N/A | Not applicable |

### 1.6 Infrastructure & API

| Feature | Fireblocks | BitGo | Copper.co | Cobo | Liminal | MG Custody |
|---------|-----------|-------|-----------|------|---------|------------|
| **API maturity** | Full REST + webhooks + SDK (multiple languages) | Full REST + SDK | REST + full API | REST + SDK (WaaS) | REST + WaaS API | REST (5 endpoints) + webhooks |
| **Webhook support** | Comprehensive event system | Yes | Yes | Yes | Yes | 4 event types, HMAC-signed |
| **Auto-sweeping** | Yes (configurable) | Yes | Via ClearLoop | Auto-sweep to MPC addresses | Not specified | Planned (Phase 4) |
| **Fee management** | Gas station / fee sponsoring | Built-in | Included | Fee Station (auto gas top-up) | Not specified | EIP-1559 estimator (ETH only) |
| **Settlement network** | Fireblocks Network (instant) | BitGo Settlement | ClearLoop (T+4 hours) | Exchange Wallet integration | N/A | Direct blockchain broadcast |

### 1.7 Insurance & Risk

| Feature | Fireblocks | BitGo | Copper.co | Cobo | Liminal | MG Custody |
|---------|-----------|-------|-----------|------|---------|------------|
| **Insurance coverage** | Up to $1B total; $42M per-incident cyber/fraud | $250M standard; up to $1B for large clients | Not publicly specified | Not publicly specified | Insured (amount not public) | None |
| **Circuit breaker** | Policy engine enforced | Policy engine enforced | ClearLoop settlement limits | Policy engine enforced | Firewall rules | Basic (per-tx $500K, hourly $10M, daily $50M) |
| **Incident history** | Zero-loss track record claimed | Zero-loss track record | Zero-loss track record | Zero-incident since 2017 | Not specified | N/A (not in production) |

---

## 2. Gap Analysis: MG Custody vs. Industry

### CRITICAL GAPS (P0 -- Must fix before production)

| # | Gap | Industry Standard | MG Custody Current | Impact | Effort |
|---|-----|-------------------|-------------------|--------|--------|
| P0-1 | **No MPC/multi-party signing** | All 5 providers use MPC or multi-sig; single-seed = single point of compromise | Single HD seed, software signing | A compromised server means total fund loss. This is the #1 reason institutional clients reject self-custody solutions. | 8-12 weeks (GG20 for secp256k1, FROST for Ed25519) |
| P0-2 | **No HSM integration** | All providers use HSM or SGX for key protection in production | PKCS#11 stub only | Private keys exist in process memory unprotected by hardware. Any memory dump or side-channel attack exposes all keys. | 3-4 weeks (PKCS#11 or cloud HSM SDK) |
| P0-3 | **No AML/KYT screening** | Universal -- Chainalysis, Elliptic, or TRM Labs integration | Not implemented | Regulatory non-compliance. Processing a sanctioned address = legal liability, potential criminal exposure. | 2-3 weeks (Chainalysis API integration) |
| P0-4 | **No address whitelisting** | Universal across all providers | Not implemented | Any withdrawal destination is accepted. Insider or attacker can drain to arbitrary address. | 1-2 weeks |
| P0-5 | **Approval workflow not functional** | All providers have multi-approver workflows with FIDO2/hardware auth | 5 tiers defined, only auto-approve coded | Tiers 2-4 ($10K+) require human approval but have no mechanism for it. All withdrawals effectively auto-approve. | 3-4 weeks (approval UI/API + FIDO2) |
| P0-6 | **No OFAC/sanctions screening** | Real-time screening universal | Planned but not implemented | Same legal exposure as P0-3. OFAC violations carry strict liability. | 1-2 weeks (OFAC SDN list + API) |

### IMPORTANT GAPS (P1 -- Required for institutional credibility)

| # | Gap | Industry Standard | MG Custody Current | Impact | Effort |
|---|-----|-------------------|-------------------|--------|--------|
| P1-1 | **Limited chain support (4 chains)** | 21-150+ chains | 4 chains (BTC, ETH, TRX, SOL) | Cannot serve multi-chain institutional portfolios. Missing L2s (Arbitrum, Optimism, Base), Cosmos ecosystem, Polkadot, etc. | 2-4 weeks per chain |
| P1-2 | **Live TRUST validation** | Counterparty data exchange with traceable delivery | 21 Analytics TRUST flow implemented and verified with local PostgreSQL and an HTTPS mock. | Verify delivery and handling of unconfirmed sends with real counterparties before rollout. | Depends on TRUST onboarding and test access |
| P1-3 | **No SOC 2 or ISO 27001 certification** | SOC 2 Type 2 is table stakes; CCSS Level 3 is gold standard | No certifications | Institutional clients require audit reports before onboarding. No cert = no enterprise sales. | 3-6 months (external audit engagement) |
| P1-4 | **No policy management API** | All providers offer API-driven policy CRUD | Hardcoded circuit breaker limits | Operators cannot adjust limits without code changes and redeployment. | 2-3 weeks |
| P1-5 | **No admin quorum for config changes** | SGX-enforced or multi-admin approval for policy changes | Single admin / env var changes | A single operator can modify limits, disable circuit breaker, or change withdrawal rules unilaterally. | 2-3 weeks |
| P1-6 | **No multi-chain fee estimation** | Per-chain gas/fee management with caps and queuing | ETH only (EIP-1559) | BTC, TRX, SOL withdrawals have no fee estimation. Cannot calculate withdrawal costs or enforce gas caps. | 1-2 weeks per chain |
| P1-7 | **No Proof of Reserves** | Merkle-tree or on-chain attestation | Planned (Phase 5) | Cannot prove solvency to auditors or users. Increasingly expected post-FTX. | 3-4 weeks |
| P1-8 | **Hot/warm/cold tiering not implemented** | All providers: automated sweeps between tiers | Planned (Phase 4) | All funds sit in hot wallet. Maximizes attack surface and potential loss. | 3-4 weeks |

### NICE-TO-HAVE GAPS (P2 -- Competitive differentiation)

| # | Gap | Industry Standard | MG Custody Current | Impact | Effort |
|---|-----|-------------------|-------------------|--------|--------|
| P2-1 | **No staking support** | All major providers offer native staking | Not planned | Cannot earn yield on custodied assets. Missed revenue opportunity for exchange. | 4-6 weeks per chain |
| P2-2 | **No DeFi connectivity** | Fireblocks, Copper, Cobo offer DeFi access | Not planned | Cannot participate in DeFi from custody. Limited to basic transfers. | 8+ weeks |
| P2-3 | **No settlement network** | Fireblocks Network, ClearLoop, BitGo Settlement | Direct blockchain only | Every transfer requires on-chain settlement. No instant off-chain settlement with counterparties. | Not feasible for self-hosted |
| P2-4 | **No NFT/RWA custody** | Fireblocks, BitGo, Cobo support NFTs | Not planned | Cannot custody tokenized assets or NFTs. Increasingly relevant with RWA tokenization. | 2-3 weeks per chain |
| P2-5 | **No insurance coverage** | $250M-$1B coverage standard | None | No financial backstop for loss events. Self-insurance only option for self-hosted. | N/A (commercial arrangement) |
| P2-6 | **No smart contract wallet support** | Cobo offers as primary option; others support | Not planned | Cannot leverage account abstraction, gasless txs, batched operations. | 6-8 weeks |
| P2-7 | **No Wallet-as-a-Service API** | Cobo, Liminal offer WaaS | 5 basic endpoints | Cannot serve as infrastructure for other platforms. API is exchange-internal only. | 4-6 weeks |
| P2-8 | **Limited token support** | 700-3,000+ tokens | 4 tokens (ETH, USDT, USDC per chain) | Cannot custody long-tail assets. Every new token requires code changes. | 2-3 weeks (token registry system) |

---

## 3. Unique Differentiators by Provider

### Fireblocks
- **SGX-enforced policy engine**: Policy rules are signed and executed inside Intel SGX enclaves. Even Fireblocks employees cannot modify active policies. No self-hosted equivalent exists.
- **Fireblocks Network**: 2,400+ counterparties with instant off-chain settlement. Network effect moat.
- **$10T+ processed**: Scale validates architecture.

### BitGo
- **OCC National Bank Charter (Dec 2025)**: First crypto-native company with a US bank charter. Can serve as a qualified custodian under SEC rules without third-party bank dependency.
- **Stablecoin-as-a-Service**: Turnkey stablecoin issuance (used by World Liberty Financial for USD1).
- **$250M-$1B insurance**: Underwritten by Lloyd's syndicates, largest in industry.

### Copper.co
- **ClearLoop**: Trade on exchanges without transferring assets out of custody. Eliminates exchange counterparty risk (post-FTX, this is a major selling point). T+4 hour settlement.
- **DeFi Connectivity**: 15,000+ dApps across 10 chains from within custody. Unique breadth.

### Cobo
- **4-wallet-type unified platform**: Custodial + MPC + Smart Contract + Exchange wallets under one API. Most flexible architecture.
- **3,000+ tokens on 80+ chains**: Broadest asset coverage among mid-tier providers.
- **Auto-sweep + Fee Station**: Operational automation that reduces manual intervention.

### Liminal
- **CCSS Level 3 Full-System certification**: Highest possible CryptoCurrency Security Standard rating. Only a handful of providers hold this.
- **HSM Vaults (March 2026)**: MPC authorization protocol running inside Securosys HSMs. Hardware-enforced multi-party authorization, not just key storage.
- **$100B+ processed**: Strong in Asian institutional market, especially India.

---

## 4. Priority Recommendations for MG Custody

### Immediate (Before any production deployment)

1. **Implement MPC signing (P0-1)** -- This is the single most important gap. A single-seed architecture is fundamentally unsuitable for production custody. Implement GG20 threshold ECDSA for Bitcoin/Ethereum/Tron and FROST for Solana/Ed25519. Target: 2-of-3 threshold with one share on HSM, one on application server, one offline backup.

2. **Integrate HSM for key storage (P0-2)** -- Finish the PKCS#11 integration. At minimum, use AWS CloudHSM or Azure Dedicated HSM for the seed/key shares. The software signing backend must be dev/test only.

3. **Add AML/KYT + OFAC screening (P0-3, P0-6)** -- Integrate Chainalysis KYT or TRM Labs. Screen all deposit source addresses and withdrawal destination addresses before processing. Integrate OFAC SDN list with daily refresh. This is a legal requirement, not a feature.

4. **Implement address whitelisting (P0-4)** -- No withdrawal should go to a non-whitelisted address. Require multi-admin approval to add new addresses. This is a basic operational control that every provider enforces.

5. **Complete approval workflows (P0-5)** -- The 5-tier system is well-designed. Implement Tiers 2-4 with FIDO2/WebAuthn hardware key authentication. The signing service must verify approval chain before signing any transaction.

### Short-term (First 3 months of operation)

6. **Hot/warm/cold tiering + sweeps (P1-8)** -- Move 80% of funds to cold storage, 15% warm, 5% hot. Automated sweep when hot wallet drops below threshold.

7. **Travel Rule integration (P1-2)** -- Integrate Notabene for FATF Recommendation 16 compliance. Required for EU operations under TFR 2023/1113.

8. **Policy management API (P1-4, P1-5)** -- Expose circuit breaker limits, approval tiers, and whitelists via authenticated API. Require admin quorum (2-of-3) for any policy modification.

9. **SOC 2 Type 2 audit engagement (P1-3)** -- Begin the audit process immediately. The audit cycle takes 3-6 months. This is a prerequisite for institutional client acquisition.

10. **Multi-chain fee estimation (P1-6)** -- Complete fee estimators for BTC (estimatesmartfee), TRX (Energy/bandwidth), SOL (priority fees).

### Medium-term (6-12 months)

11. **Proof of Reserves (P1-7)** -- Implement Merkle-tree based PoR with periodic attestation.
12. **Expand chain support (P1-1)** -- Prioritize Arbitrum, Optimism, Base (L2s share ETH signing), then Polygon, Avalanche.
13. **Token registry system (P2-8)** -- Dynamic ERC-20/TRC-20/SPL token addition without code changes.
14. **Staking support (P2-1)** -- Start with ETH staking (largest demand), then SOL.

---

## 5. Competitive Positioning

### Where MG Custody can compete (self-hosted advantages)

| Advantage | Detail |
|-----------|--------|
| **No vendor lock-in** | Full control over infrastructure, no per-transaction fees, no API rate limits |
| **Data sovereignty** | Keys and transaction data never leave your infrastructure. Critical for some jurisdictions. |
| **Customization** | Approval workflows, limits, and chain integrations can be tailored exactly to exchange needs |
| **Cost at scale** | Fireblocks/BitGo charge per-transaction or AUM-based fees. Self-hosted has fixed infrastructure cost. |
| **Latency** | Co-located with exchange. No external API round-trips for signing. |

### Where MG Custody cannot compete (structural disadvantages)

| Disadvantage | Detail |
|--------------|--------|
| **No settlement network** | Fireblocks Network and ClearLoop are network-effect businesses. Cannot replicate. |
| **No regulatory license** | BitGo's OCC charter and Fireblocks' NYDFS trust are multi-year, multi-million-dollar investments. |
| **Insurance** | Cannot self-insure at $250M-$1B levels. Would need separate commercial policy. |
| **Chain coverage breadth** | 150 chains requires dedicated team. Realistic target for self-hosted: 10-15 chains. |
| **Security certifications** | SOC 2 achievable, but CCSS Level 3 requires dedicated security program. |

---

## 6. Cost-Benefit Summary

| Path | Annual Cost (est.) | Time to Production | Risk Profile |
|------|-------------------|-------------------|--------------|
| **Use Fireblocks/BitGo** | $200K-$2M+ (volume-dependent) | 4-8 weeks integration | Low (proven, insured) |
| **Self-hosted (current plan)** | $50K-$150K infra + team cost | 6-12 months to production-grade | Medium-High (must close P0 gaps) |
| **Hybrid: self-hosted hot + Fireblocks cold** | $100K-$500K | 3-6 months | Medium (best of both) |

The hybrid approach deserves serious consideration: use MG Custody for hot wallet operations (deposits, small withdrawals) where latency and cost matter, and a provider like Fireblocks or BitGo for cold storage and large withdrawals where security and insurance matter most.

---

## Sources

- [Fireblocks: 150 Public Blockchains Supported](https://www.fireblocks.com/blog/leader-in-public-blockchain-support-coverage)
- [Fireblocks: Governance and Policy Engine](https://www.fireblocks.com/platforms/governance-and-policies)
- [Fireblocks: Configure Transaction Authorization Policy (TAP)](https://developers.fireblocks.com/reference/configure-transaction-authorization-policy)
- [Fireblocks: Key Features and Capabilities](https://developers.fireblocks.com/docs/capabilities)
- [Fireblocks: DeFi Suite](https://www.fireblocks.com/platforms/defi)
- [Fireblocks: Digital Asset Custody Strategy for Banks](https://www.fireblocks.com/blog/digital-asset-custody-strategy-banks)
- [Fireblocks: Next Generation Full Stack Protection](https://www.fireblocks.com/blog/the-next-generation-of-full-stack-protection)
- [BitGo: Institutional Standard for Digital Asset Custody](https://www.bitgo.com/resources/blog/the-institutional-standard-for-digital-asset-custody/)
- [BitGo: Multi-Sig Wallets for Institutions](https://www.bitgo.com/resources/blog/multi-sig-wallets-for-institutions/)
- [BitGo: SOC 2 Type 2 Commitment](https://www.bitgo.com/resources/blog/bitgos-commitment-to-security-and-trust-the-soc-2-advantage/)
- [BitGo: 2025 Year in Review](https://www.bitgo.com/resources/blog/2025-year-in-review/)
- [BitGo: 80% of Top 100 Digital Assets Supported](https://www.bitgo.com/resources/blog/bitgo-reaches-new-milestone-80-of-the-top-100-digital-assets-now-supported/)
- [BitGo IPO Signals Institutional Readiness (2026)](https://www.ainvest.com/news/strategic-regulated-crypto-infrastructure-2026-bitgo-ipo-signals-institutional-readiness-2601/)
- [Copper.co: ClearLoop](https://copper.co/en/products/clearloop)
- [Copper.co: Notabene Travel Rule Partnership](https://copper.co/en/insights/company-news/copper-partners-with-notabene-to-streamline-travel-rule-compliance)
- [Copper.co: ClearLoop Trust and Collateral Structure](https://copper.co/en/insights/company-news/clearloops-trust-and-collateral-structure-strengthens-asset-security)
- [Cobo: MPC Wallet Security Guide](https://www.cobo.com/post/what-is-an-mpc-wallet-the-complete-security-guide)
- [Cobo: Smart Contract Wallets Guide 2026](https://www.cobo.com/post/smart-contract-wallets)
- [Cobo: Crypto Custody Solutions Complete 2026 Guide](https://www.cobo.com/post/crypto-custody-solutions-complete-guide)
- [Cobo: Top 10 Crypto Custody Firms 2025](https://www.cobo.com/post/top-10-crypto-custody-firms-with-proven-security-frameworks)
- [Liminal: Institutional Digital Asset Custody](https://www.liminalcustody.com/digital-asset-custody/)
- [Liminal: HSM Vaults Launch with Securosys (March 2026)](http://www.prnewswire.com/news-releases/liminal-custody-and-securosys-launch-liminal-hsm-vaults--a-new-security-standard-for-digital-assets-302722979.html)
- [Liminal: $100 Billion in Transaction Volume](https://www.prnewswire.com/news-releases/liminal-custody-crosses-100-billion-in-total-transaction-volume-302706024.html)
- [Liminal: Protocol and Token Support](https://www.liminalcustody.com/protocol-token-support/)
- [MPC vs HSM Custody Architecture Divide (2026)](https://blockeden.xyz/blog/2026/02/27/mpc-hsm-custody-architecture/)
- [Multi-Sig and MPC in Enterprise Crypto Custody 2026](https://www.chainupad.com/blog/multi-sig-mpc-enterprise-crypto-custody-2026/)
- [Top MPC Wallet Solutions Compared 2025](https://safeheron.com/blog/mpc-solutions-comparison-2025-top-wallets-security-features/)
