# External API Analysis: Buy vs Build

## MUST USE EXTERNAL (невозможно/нецелесообразно строить самим)

| Service | Why External | Provider | Cost |
|---------|-------------|----------|------|
| **Blockchain RPC nodes** | Infrastructure, not logic | Alchemy/Infura/QuickNode or self-hosted | $200-2000/mo or self-hosted |
| **OFAC SDN list** | Government data source | US Treasury (free download) | Free |
| **EU Sanctions list** | Government data source | EU registry (free) | Free |
| **SSL certificates** | Trust infrastructure | Let's Encrypt | Free |
| **DNS** | Infrastructure | Cloudflare/Route53 | Minimal |

## CAN BUILD OURSELVES (заменяем свои сервисом)

| Service | External Option | Our Alternative | Complexity | Savings |
|---------|----------------|-----------------|------------|---------|
| **AML/KYT screening** | Chainalysis ($50K+/yr) | On-chain address clustering + heuristics | L | $50K+/yr |
| **Price oracle** | CoinGecko API (free) | Multi-source aggregator (Binance+CG+CMC) | S | Already done |
| **Alerting** | PagerDuty ($20/user/mo) | Telegram bot + Slack webhook + email | M | $500+/yr |
| **Monitoring dashboards** | Grafana Cloud ($50/mo) | Self-hosted Grafana (we have /metrics) | S | $600/yr |
| **Travel Rule** | Notabene ($$$) | Direct TRISA protocol implementation | L | $$$  |
| **DEX aggregation** | 1inch API | Direct DEX contract calls (Uniswap Router) | M | Free |
| **WalletConnect** | WalletConnect Cloud | Self-hosted relay (open protocol) | M | Free |
| **Load testing** | Grafana k6 Cloud | Local k6 / custom Rust bench | S | Free |

## HYBRID (используем free tier + свой fallback)

| Service | Strategy |
|---------|----------|
| **CoinGecko** | Free API for prices, cache aggressively, fallback to Binance API |
| **Blockchain nodes** | Primary: Alchemy/QuickNode free tier. Fallback: public RPCs |
| **Sanctions screening** | Download OFAC list directly + basic on-chain heuristics |

## Recommendation

Build our own for everything except blockchain nodes (use Alchemy free tier + self-hosted).
Total external cost: ~$200/month for premium RPC if needed. Everything else is free or self-built.
