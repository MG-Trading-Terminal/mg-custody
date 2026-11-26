# References

Every formula, mechanism, and design decision must trace to a documented source.

## Key Management

| Mechanism | Reference | Used In |
|-----------|-----------|---------|
| HD Wallet Derivation | [BIP-32](https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki) | address-factory |
| Key Derivation Paths | [BIP-44](https://github.com/satoshilabs/slips/blob/master/slip-0044.md) | address-factory |
| Ed25519 HD Derivation | [SLIP-0010](https://github.com/satoshilabs/slips/blob/master/slip-0010.md) | address-factory (Solana) |
| Mnemonic Seeds | [BIP-39](https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki) | signing |
| Shamir Secret Sharing | [Shamir 1979](https://dl.acm.org/doi/10.1145/359168.359176) | signing (backup) |
| MPC Threshold ECDSA | [GG20](https://eprint.iacr.org/2020/540) | signing (Phase 3) |
| FROST EdDSA | [FROST RFC](https://www.ietf.org/archive/id/draft-irtf-cfrg-frost-15.html) | signing (Solana, Phase 3) |

## Bitcoin

| Mechanism | Reference | Used In |
|-----------|-----------|---------|
| P2WPKH (SegWit) | [BIP-141](https://github.com/bitcoin/bips/blob/master/bip-0141.mediawiki) | address-factory |
| Bech32 Encoding | [BIP-173](https://github.com/bitcoin/bips/blob/master/bip-0173.mediawiki) | address-factory |
| Replace-By-Fee | [BIP-125](https://github.com/bitcoin/bips/blob/master/bip-0125.mediawiki) | broadcaster |
| Coin Selection (BnB) | [Bitcoin Core BnB](https://murch.one/erhardt2016coinselection.pdf) | utxo |
| Fee Estimation | Bitcoin Core `estimatesmartfee` | fee-estimator |
| Dust Limit | 546 satoshis (Bitcoin Core DEFAULT_DUST_RELAY_TX_FEE) | utxo |

## Ethereum

| Mechanism | Reference | Used In |
|-----------|-----------|---------|
| EIP-1559 Fees | [EIP-1559](https://eips.ethereum.org/EIPS/eip-1559) | fee-estimator, broadcaster |
| EIP-55 Checksum | [EIP-55](https://eips.ethereum.org/EIPS/eip-55) | address-factory |
| ERC-20 Standard | [EIP-20](https://eips.ethereum.org/EIPS/eip-20) | scanner, broadcaster |
| Replay Protection | [EIP-155](https://eips.ethereum.org/EIPS/eip-155) | signing |
| Typed Transactions | [EIP-2718](https://eips.ethereum.org/EIPS/eip-2718) | signing |

## Tron

| Mechanism | Reference | Used In |
|-----------|-----------|---------|
| Address Format | [Tron Docs: Accounts](https://developers.tron.network/docs/account) | address-factory |
| TRC-20 Transfers | [Tron Docs: TRC-20](https://developers.tron.network/docs/trc20) | scanner, broadcaster |
| Energy Model | [Tron Docs: Resource Model](https://developers.tron.network/docs/resource-model) | fee-estimator |
| USDT Contract | `TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t` | config |

## Solana

| Mechanism | Reference | Used In |
|-----------|-----------|---------|
| Associated Token Account | [SPL ATA](https://spl.solana.com/associated-token-account) | address-factory |
| SPL Token Program | [SPL Token](https://spl.solana.com/token) | scanner, broadcaster |
| Commitment Levels | [Solana Docs](https://docs.solana.com/cluster/commitments) | scanner |
| Transaction Lifecycle | [Solana Docs](https://docs.solana.com/developing/transaction_confirmation) | broadcaster |

## Wallet Management

| Mechanism | Reference | Used In |
|-----------|-----------|---------|
| Hot/Warm/Cold Tiers | Industry standard (Coinbase, Gemini custody docs) | wallet-manager |
| Hot wallet max 5% AUM | Bybit/Binance custody disclosures | wallet-manager |
| Sweep logic | Fireblocks architecture docs | wallet-manager |

## Circuit Breaker

| Mechanism | Reference | Used In |
|-----------|-----------|---------|
| Per-tx/hourly/daily limits | Exchange custody/src/circuit_breaker.rs | withdrawal-pipeline |
| New-address spike detection | Exchange custody/src/circuit_breaker.rs | withdrawal-pipeline |

## Compliance

| Mechanism | Reference | Used In |
|-----------|-----------|---------|
| MiCA Custody (Art.70, 75) | [MiCA Regulation](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX:32023R1114) | compliance |
| Travel Rule | [FATF Rec.16](https://www.fatf-gafi.org/en/topics/virtual-assets.html), [TFR 2023/1113](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX:32023R1113) | compliance |
| TRUST counterparty lookup, post-broadcast dispatch and receipt reconciliation | [21 Analytics 8.4.0 API guide](https://docs.21analytics.ch/8.4.0/api/getting-started.html), [outgoing workflow](https://docs.21analytics.ch/8.4.0/api/outgoing_integration.html), 8.4.0 GraphQL schema and DTI registry | `custody-compliance`, `custody-withdrawal-pipeline`, `custody-admin` |
| MG Custody Travel Rule setup and provider comparison | [Setup and workflow](README.md#21-analytics-trust), [custody provider gap analysis](docs/gap-analysis-custody-providers.md#14-compliance--aml) | Deployment and onboarding documentation |
| OFAC Sanctions | [OFAC SDN List](https://www.treasury.gov/ofac/downloads/sdnlist.txt) | withdrawal-pipeline |
| Proof of Reserves | [Vitalik PoR](https://vitalik.eth.limo/general/2022/11/19/proof_of_solvency.html) | reconciliation |
