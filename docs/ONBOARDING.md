# Onboarding Walkthrough

> Hands-on guide for your first day with MG Custody. Pick your archetype and follow the steps.

**Prerequisites**: You've completed the [Quickstart](QUICKSTART.md) and have a running instance.

---

## Archetype 1: Crypto Startup (< $10M assets)

**You are**: Building an exchange, neobank, or payments product. You need custody but Fireblocks costs $50K+/year. You want to own your keys.

### Day 1 Goals

1. Generate deposit addresses for test users
2. See a deposit detected (testnet)
3. Process a withdrawal (testnet)
4. Set up webhook notifications to your backend

### Step 1: Connect to Testnet

Edit `config/local.toml` to use testnet RPCs:

```toml
[chains.ethereum]
enabled = true
rpc_url = "https://rpc.sepolia.org"  # Free, no API key needed
chain_id = 11155111

[chains.bitcoin]
enabled = true
rpc_url = "http://localhost:18332"   # Bitcoin testnet3 or signet
network = "testnet"
```

Start in development mode:
```bash
./target/release/custody
```

### Step 2: Generate Deposit Addresses

```bash
# Create addresses for a test user
USER_ID="550e8400-e29b-41d4-a716-446655440000"

# ETH address
curl -s -X POST http://localhost:8443/v1/addresses \
  -H "Content-Type: application/json" \
  -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  -d "{\"user_id\":\"$USER_ID\",\"asset\":\"ETH\",\"chain\":\"ethereum\"}"

# BTC address
curl -s -X POST http://localhost:8443/v1/addresses \
  -H "Content-Type: application/json" \
  -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  -d "{\"user_id\":\"$USER_ID\",\"asset\":\"BTC\",\"chain\":\"bitcoin\"}"
```

Each call is **idempotent** — same (user_id, asset, chain) always returns the same address.

### Step 3: Send a Test Deposit

Send Sepolia ETH to the generated address using a faucet:
- https://sepoliafaucet.com/
- https://www.alchemy.com/faucets/ethereum-sepolia

Then watch the scanner pick it up:
```bash
# Check scanner status
curl -s http://localhost:8443/health/chains | jq .

# List detected deposits
curl -s http://localhost:8443/v1/deposits?user_id=$USER_ID | jq .
```

The deposit goes through: **Detected** → **Confirming** (12 blocks) → **Confirmed** → webhook fires.

### Step 4: Set Up Webhooks

Your backend needs to receive deposit/withdrawal notifications:

```bash
# Register webhook endpoint
curl -s -X POST http://localhost:8443/v1/webhooks \
  -H "Content-Type: application/json" \
  -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  -d '{
    "url": "https://your-exchange.com/api/custody-webhook",
    "secret": "your-webhook-hmac-secret",
    "events": ["deposit_confirmed", "withdrawal_broadcast", "withdrawal_confirmed", "withdrawal_failed"]
  }'
```

Webhook payloads are signed with HMAC-SHA256. Verify in your backend:
```python
import hmac, hashlib

def verify_webhook(body: bytes, signature: str, secret: str) -> bool:
    expected = hmac.new(secret.encode(), body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(expected, signature)
```

### Step 5: Process a Withdrawal

```bash
# Submit a withdrawal
curl -s -X POST http://localhost:8443/v1/withdrawals \
  -H "Content-Type: application/json" \
  -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  -d '{
    "withdrawal_id": "wd-001",
    "asset": "ETH",
    "chain": "ethereum",
    "address": "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18",
    "amount": "0.01"
  }'
# → 202 Accepted with custody_ref

# Track status
curl -s http://localhost:8443/v1/withdrawals/wd-001 | jq .status
# Received → Validated → Signed → Broadcast → Confirmed
```

### Step 6: Monitor Health

```bash
# Overall health
curl -s http://localhost:8443/health
# → "ok"

# Per-chain health
curl -s http://localhost:8443/health/chains | jq .

# Dashboard
curl -s http://localhost:8443/v1/dashboard/health | jq .

# Circuit breaker status
curl -s http://localhost:8443/v1/system/circuit-breaker | jq .
```

### What's Next for Startups

1. **Integrate with your exchange backend** — implement the [CustodyProvider trait](../README.md) using HTTP client
2. **Set up vault policies** — spending limits, approval requirements per amount tier
3. **Configure address whitelists** — prevent withdrawals to unknown addresses
4. **Add monitoring** — Prometheus scrapes `/metrics`, Grafana dashboards
5. **Graduate to Mode 2** — split signer binary when you handle real funds (see [DEPLOYMENT_MODES.md](DEPLOYMENT_MODES.md))

---

## Archetype 2: Trading Desk ($10M–$500M assets)

**You are**: Running OTC or prop trading. Currently using Fireblocks/BitGo. Want lower costs, lower latency, full control.

### Day 1 Goals

1. Set up vault structure mirroring your Fireblocks setup
2. Configure approval workflows matching your compliance policy
3. Test raw signing for OTC settlement
4. Verify circuit breaker limits match your risk parameters

### Step 1: Create Vault Structure

MG Custody uses the same vault/account abstraction as Fireblocks:

```bash
# Create main vault
curl -s -X POST http://localhost:8443/v1/vaults \
  -H "Content-Type: application/json" \
  -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  -d '{"name": "Trading Hot Wallet", "description": "OTC settlement"}'
# → vault_id

# Create vault accounts per asset
for ASSET in ETH BTC USDT USDC; do
  curl -s -X POST http://localhost:8443/v1/vaults/$VAULT_ID/accounts \
    -H "Content-Type: application/json" \
    -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
    -d "{\"asset\":\"$ASSET\",\"chain\":\"ethereum\"}"
done

# Create a cold vault
curl -s -X POST http://localhost:8443/v1/vaults \
  -H "Content-Type: application/json" \
  -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  -d '{"name": "Cold Storage", "description": "Long-term holdings"}'
```

### Step 2: Configure Wallet Tiers

Set up hot/warm/cold allocation matching your risk appetite:

```bash
# View current tier config
curl -s http://localhost:8443/v1/wallets/ETH | jq .

# Default tiers: hot 5%, warm 15%, cold 80%
# Adjust via system config if needed
```

| Tier | Purpose | % of Total | Access |
|------|---------|------------|--------|
| Hot | Immediate withdrawals | 5% | Automated |
| Warm | Intraday rebalancing | 15% | 1 approval |
| Cold | Long-term storage | 80% | Multi-approval |

### Step 3: Set Up Approval Workflows

Configure approval tiers matching your compliance policy:

```bash
# View default withdrawal tiers
curl -s http://localhost:8443/v1/policies/rules | jq .

# Add approval rule: >$10K requires 1 approver
curl -s -X POST http://localhost:8443/v1/policies/rules \
  -H "Content-Type: application/json" \
  -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  -d '{
    "name": "Large withdrawal approval",
    "conditions": {"min_amount_usd": "10000"},
    "action": "escalate",
    "required_approvals": 1,
    "priority": 100
  }'

# Add rule: >$100K requires 2 approvers + 30min delay
curl -s -X POST http://localhost:8443/v1/policies/rules \
  -H "Content-Type: application/json" \
  -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  -d '{
    "name": "Critical withdrawal approval",
    "conditions": {"min_amount_usd": "100000"},
    "action": "escalate",
    "required_approvals": 2,
    "delay_seconds": 1800,
    "priority": 200
  }'
```

### Step 4: Configure Circuit Breaker for Your Volume

Default limits may be too low for a trading desk:

```bash
# View current limits
curl -s http://localhost:8443/v1/system/circuit-breaker | jq .
# Default: $500K single, $10M hourly, $50M daily

# Adjust via system_config table if needed (requires admin)
```

Map these to your actual daily trading volume with a safety margin.

### Step 5: Test Raw Signing (OTC Settlement)

For OTC trades you may need to sign arbitrary messages:

```bash
# Raw message signing (ETH)
curl -s -X POST http://localhost:8443/v1/signing/raw \
  -H "Content-Type: application/json" \
  -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  -d '{
    "chain": "ethereum",
    "derivation_path": "m/44h/60h/0h/0/0",
    "message_hex": "0xdeadbeef",
    "algorithm": "secp256k1_ecdsa"
  }'
# → signature
```

### Step 6: Whitelist Counterparty Addresses

```bash
# Add counterparty address
curl -s -X POST http://localhost:8443/v1/policies/whitelist \
  -H "Content-Type: application/json" \
  -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  -d '{
    "chain": "ethereum",
    "address": "0xCounterpartyAddress...",
    "label": "Prime Broker A",
    "category": "external"
  }'

# List whitelisted addresses
curl -s http://localhost:8443/v1/policies/whitelist | jq .
```

### Step 7: Internal Transfers (Vault ↔ Vault)

```bash
# Move funds between vaults (hot → cold sweep)
curl -s -X POST http://localhost:8443/v1/vaults/transfers \
  -H "Content-Type: application/json" \
  -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  -d '{
    "from_vault_id": "'$HOT_VAULT_ID'",
    "to_vault_id": "'$COLD_VAULT_ID'",
    "asset": "ETH",
    "amount": "10.0",
    "idempotency_key": "sweep-2026-03-27-001"
  }'
```

### Step 8: Set Up Monitoring & Alerts

```bash
# Configure Telegram alerts
curl -s -X POST http://localhost:8443/v1/notifications/channels \
  -H "Content-Type: application/json" \
  -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  -d '{
    "type": "telegram",
    "config": {"bot_token": "...", "chat_id": "..."},
    "events": ["circuit_breaker_tripped", "withdrawal_failed", "large_deposit"]
  }'
```

### Migration Checklist (from Fireblocks/BitGo)

- [ ] Map Fireblocks vault IDs → MG Custody vault IDs
- [ ] Recreate approval policies (Fireblocks TAP → MG Custody rules)
- [ ] Recreate whitelisted addresses
- [ ] Update exchange backend to point to MG Custody API
- [ ] Run parallel for 2 weeks (shadow mode: both systems, compare results)
- [ ] Cut over hot wallet first, cold wallet last
- [ ] Update monitoring dashboards
- [ ] Notify counterparties of new deposit addresses

### What's Next for Trading Desks

1. **Deploy Mode 2** — split signer for real funds (see [DEPLOYMENT_MODES.md](DEPLOYMENT_MODES.md))
2. **Reconciliation** — set up scheduled on-chain vs ledger reconciliation
3. **Proof of Reserves** — generate Merkle proofs for auditors
4. **Compliance** — configure OFAC screening and 21 Analytics. Submit withdrawal identity data through the custody API and verify TRUST delivery in a test environment. [Travel Rule setup](../README.md#21-analytics-trust)
5. **Backup & DR** — test seed reconstruction from Shamir shares

---

## Common Post-Onboarding Tasks

### API Authentication

All API requests use HMAC-SHA256:

```bash
# Generate HMAC signature
TIMESTAMP=$(date +%s)
BODY='{"user_id":"...","asset":"ETH","chain":"ethereum"}'
SIGNATURE=$(echo -n "POST/v1/addresses${TIMESTAMP}${BODY}" | \
  openssl dgst -sha256 -hmac "$CUSTODY_HMAC_SECRET" | cut -d' ' -f2)

curl -X POST http://localhost:8443/v1/addresses \
  -H "Content-Type: application/json" \
  -H "X-Custody-Timestamp: $TIMESTAMP" \
  -H "X-Custody-Signature: $SIGNATURE" \
  -d "$BODY"
```

### Useful Diagnostic Commands

```bash
# Transaction history (unified across deposits + withdrawals + transfers)
curl -s "http://localhost:8443/v1/transactions?limit=20" | jq .

# Wallet balances per asset
curl -s http://localhost:8443/v1/wallets/ETH | jq .
curl -s http://localhost:8443/v1/wallets/BTC | jq .

# Audit log
curl -s "http://localhost:8443/v1/audit?limit=50" | jq .

# Export for tax/compliance
curl -s "http://localhost:8443/v1/reports/export?format=csv&from=2026-01-01&to=2026-12-31"
```

### Getting Help

- **Security concerns**: Read [THREAT_MODEL.md](../THREAT_MODEL.md)
- **Architecture decisions**: Read [DEPLOYMENT_MODES.md](DEPLOYMENT_MODES.md)
- **API reference**: `GET /v1/openapi.json` for full OpenAPI 3.0 spec
- **Issues**: https://github.com/anthropics/mg-custody/issues
