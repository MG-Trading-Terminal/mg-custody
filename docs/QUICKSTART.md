# Quickstart: Secure Deployment in 30 Minutes

> This guide gets you from zero to a running custody service with basic security.

## Prerequisites

- Rust 1.85+ (`rustup update`)
- PostgreSQL 16+ (or Docker)
- ~2GB disk space for build

## Step 1: Clone and Build (5 min)

```bash
git clone <repo-url> mg-custody
cd mg-custody
cargo build --release
```

This produces 4 binaries:
- `custody` — main service (API + scanners + pipeline)
- `custody-signer` — isolated signing service (Phase 17B)
- `custody-admin` — admin CLI tools
- `custody-seed` — seed generation + backup

## Step 2: Database (3 min)

```bash
# Option A: Docker
docker run -d --name custody-pg \
  -e POSTGRES_USER=custody \
  -e POSTGRES_PASSWORD=<STRONG_PASSWORD> \
  -e POSTGRES_DB=custody \
  -p 5432:5432 \
  postgres:16-alpine

# Option B: Existing PostgreSQL
createdb custody
```

Run all migrations:
```bash
export DATABASE_URL=postgres://custody:<PASSWORD>@localhost:5432/custody
for f in crates/db/migrations/*.sql; do
  psql "$DATABASE_URL" -f "$f"
done
```

## Step 3: Generate Seed (5 min)

```bash
# Generate encrypted seed (will prompt for password)
./target/release/custody-seed generate --output seed.enc

# IMPORTANT: Back up seed.enc and remember the password
# For Shamir backup (recommended):
./target/release/custody-seed split --input seed.enc --threshold 3 --shares 5
```

**Store shares in separate secure locations. Any 3 of 5 can reconstruct.**

## Step 4: Configuration (5 min)

```bash
cp config/default.toml config/local.toml
```

Edit `config/local.toml`:
```toml
[api]
bind = "127.0.0.1:8443"  # NEVER bind 0.0.0.0 without firewall

[chains.ethereum]
enabled = true
rpc_url = "https://eth-mainnet.g.alchemy.com/v2/YOUR_KEY"

[chains.bitcoin]
enabled = true
rpc_url = "http://localhost:8332"
rpc_user = "bitcoinrpc"
rpc_password = "<STRONG_PASSWORD>"
```

## Step 5: Set Environment Variables (2 min)

```bash
export DATABASE_URL=postgres://custody:<PASSWORD>@localhost:5432/custody
export CUSTODY_SEED_FILE=./seed.enc
export CUSTODY_SEED_PASSWORD=<YOUR_SEED_PASSWORD>
export CUSTODY_HMAC_SECRET=$(openssl rand -hex 32)
export CUSTODY_ADMIN_KEY=$(openssl rand -hex 16)
```

## Step 6: Start Services (2 min)

### Development Mode (single process)
```bash
./target/release/custody
```

### Production Mode (split signer)
```bash
# Terminal 1: Start signer (isolated, no network)
export SIGNER_SOCKET=/tmp/custody-signer.sock
export SIGNER_IPC_SECRET=$(openssl rand -hex 32)
./target/release/custody-signer

# Terminal 2: Start main service
export SIGNER_SOCKET=/tmp/custody-signer.sock
export SIGNER_IPC_SECRET=<SAME_SECRET>
./target/release/custody
```

## Step 7: Verify (3 min)

```bash
# Health check
curl http://localhost:8443/health
# → "ok"

# Deep health (includes DB check)
curl http://localhost:8443/health/deep

# Create a test address
curl -X POST http://localhost:8443/v1/addresses \
  -H "Content-Type: application/json" \
  -d '{"user_id":"550e8400-e29b-41d4-a716-446655440000","asset":"ETH","chain":"ethereum"}'

# Check system dashboard
curl http://localhost:8443/v1/dashboard/health
```

## Step 8: Security Checklist

Before handling real funds:

- [ ] Change all default passwords
- [ ] Set `CUSTODY_HMAC_SECRET` (random 32 bytes)
- [ ] Set `CUSTODY_ADMIN_KEY` (random 16 bytes)
- [ ] Verify seed backup works (test reconstruct from Shamir shares)
- [ ] Configure firewall (API on internal network only)
- [ ] Set up monitoring (Prometheus at `/metrics`)
- [ ] Configure webhook endpoints for deposit/withdrawal notifications
- [ ] Review circuit breaker limits in `system_config` table
- [ ] Read [THREAT_MODEL.md](../THREAT_MODEL.md) — know what's protected and what's NOT
- [ ] Read [DEPLOYMENT_MODES.md](DEPLOYMENT_MODES.md) — pick your mode

## What's Running

After startup, these background tasks are active:

| Task | Interval | Purpose |
|------|----------|---------|
| ETH Scanner | 12s | Detect deposits |
| BTC Scanner | 60s | Detect deposits |
| TRX Scanner | 3s | Detect deposits |
| SOL Scanner | 2s | Detect deposits |
| Confirmation Tracker | 10s | Track confirmations |
| Withdrawal Pipeline | 5s | Process state machine |
| Webhook Retry | 30s | Retry failed deliveries |
| Reconciliation | 10min | Verify balances |
| Gas Station | 2min | Auto-fund addresses |
| Sweep Scheduler | 60s | Tier rebalancing |

## Next Steps

- [Full API documentation](/v1/openapi.json) — auto-generated OpenAPI spec
- [125+ endpoint catalog](/v1/sdk/endpoints) — programmatic endpoint list
- Set up webhook endpoints for your exchange integration
- Configure vault policies for spending limits
