# MG Custody — Deployment Runbook

> This document is the single source of truth for deploying, operating, and recovering MG Custody.
> An operator with no prior knowledge of this codebase should be able to complete a production
> deployment using only this document.
>
> Read [DEPLOYMENT_MODES.md](DEPLOYMENT_MODES.md) first to choose your deployment mode.
> Read [THREAT_MODEL.md](../THREAT_MODEL.md) to understand what is and is not protected.

---

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [Installation Methods](#2-installation-methods)
   - [Method A: Docker Compose](#method-a-docker-compose-recommended)
   - [Method B: Binary from Source](#method-b-binary-from-source)
3. [Initial Configuration](#3-initial-configuration)
4. [Security Hardening](#4-security-hardening)
5. [Operational Procedures](#5-operational-procedures)
   - [Key Rotation](#key-rotation)
   - [Backup and Recovery](#backup-and-recovery)
   - [Monitoring](#monitoring)
   - [Incident Response](#incident-response)
6. [Upgrades](#6-upgrades)
7. [Troubleshooting](#7-troubleshooting)

---

## 1. Prerequisites

### 1.1 Hardware Requirements

| Component | Minimum | Recommended (Production) |
|-----------|---------|--------------------------|
| CPU | 2 cores | 4+ cores |
| RAM | 2 GB | 8 GB |
| Disk (OS + binary) | 10 GB | 20 GB SSD |
| Disk (PostgreSQL data) | 50 GB | 500 GB SSD (RAID-1) |
| Network | 100 Mbps | 1 Gbps |

The custody service itself is lightweight. PostgreSQL is the dominant consumer of disk and RAM.
Budget 1–4 GB RAM for PostgreSQL depending on the number of chains and deposit volume.

### 1.2 Software Requirements

| Software | Minimum Version | Purpose |
|----------|----------------|---------|
| Rust (rustup) | 1.85 | Build binaries from source |
| PostgreSQL | 16 | All persistent state |
| Docker Engine | 24 | Docker Compose deployment |
| Docker Compose | 2.20 | Orchestration |
| Node.js | 22 | Dashboard build only (not needed at runtime) |
| OpenSSL | 3.x | Secret generation |
| curl | Any | Health checks |

Install Rust:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update
rustc --version
# rustc 1.85.0 (expected output)
```

Install PostgreSQL 16 (Debian/Ubuntu):
```bash
sudo apt-get install -y postgresql-16 postgresql-client-16
```

Install PostgreSQL 16 (RHEL/Rocky):
```bash
sudo dnf install -y postgresql16-server postgresql16
sudo postgresql-16-setup initdb
sudo systemctl enable --now postgresql-16
```

### 1.3 Network Requirements

#### Ports

| Port | Protocol | Direction | Purpose |
|------|----------|-----------|---------|
| 8443 | TCP | Inbound (from exchange only) | REST API |
| 5432 | TCP | Internal only | PostgreSQL |
| 443/80 | TCP | Outbound | Ethereum/Solana RPC (if using hosted nodes) |
| 8332 | TCP | Internal | Bitcoin Core RPC |
| 8545 | TCP | Internal | Ethereum node RPC |
| 8546 | TCP | Internal | Ethereum node WebSocket |

**Critical**: Port 8443 must only be reachable from the exchange backend IP range.
It must not be reachable from the public internet.

#### Firewall Rules (iptables example)

```bash
# Allow exchange backend to reach the API
iptables -A INPUT -p tcp --dport 8443 -s <EXCHANGE_IP_RANGE> -j ACCEPT

# Drop all other inbound to 8443
iptables -A INPUT -p tcp --dport 8443 -j DROP

# PostgreSQL: local only
iptables -A INPUT -p tcp --dport 5432 -s 127.0.0.1 -j ACCEPT
iptables -A INPUT -p tcp --dport 5432 -j DROP
```

#### DNS

The custody service makes outbound DNS queries for RPC node hostnames.
Ensure the host can reach your DNS resolver and resolve public domains if using hosted RPC providers
(Alchemy, QuickNode, Trongrid).

### 1.4 Secrets to Generate Before First Run

The following secrets must exist before the service starts. Do not start the service with empty values.

| Secret | How to Generate | Where Used |
|--------|----------------|------------|
| `POSTGRES_PASSWORD` | `openssl rand -hex 24` | PostgreSQL authentication |
| `CUSTODY_HMAC_SECRET` | `openssl rand -hex 32` | HMAC-SHA256 API authentication |
| `CUSTODY_ADMIN_KEY` | `openssl rand -hex 16` | Admin endpoint authentication |
| Seed file (`seed.enc`) | `custody-seed generate` | Master key for all HD derivation |
| `CUSTODY_SEED_PASSWORD` | Chosen by operator (strong passphrase) | Decrypt seed at startup |

Generate all at once and save to a `.env` file (chmod 600):

```bash
cat > /etc/custody/secrets.env << 'EOF'
POSTGRES_PASSWORD=REPLACE_ME
CUSTODY_HMAC_SECRET=REPLACE_ME
CUSTODY_ADMIN_KEY=REPLACE_ME
CUSTODY_SEED_PASSWORD=REPLACE_ME
EOF

# Generate the actual values
PG_PASS=$(openssl rand -hex 24)
HMAC=$(openssl rand -hex 32)
ADMIN_KEY=$(openssl rand -hex 16)

sed -i "s/POSTGRES_PASSWORD=REPLACE_ME/POSTGRES_PASSWORD=${PG_PASS}/" /etc/custody/secrets.env
sed -i "s/CUSTODY_HMAC_SECRET=REPLACE_ME/CUSTODY_HMAC_SECRET=${HMAC}/" /etc/custody/secrets.env
sed -i "s/CUSTODY_ADMIN_KEY=REPLACE_ME/CUSTODY_ADMIN_KEY=${ADMIN_KEY}/" /etc/custody/secrets.env

chmod 600 /etc/custody/secrets.env
```

The seed password is the one value you must choose and memorize (or store in a password manager).
It is not generated — it is set by the operator. It must be at least 20 characters.

---

## 2. Installation Methods

### Method A: Docker Compose (Recommended)

Docker Compose is the recommended method. It handles service ordering, health checks,
network isolation between the signer and API, and restart policies.

#### Step 1: Clone the repository

```bash
git clone <repo-url> /opt/mg-custody
cd /opt/mg-custody
```

#### Step 2: Create the environment file

```bash
cp .env.example .env
chmod 600 .env
```

Edit `.env` with the values generated in section 1.4:

```bash
# .env — all secrets for Docker Compose
POSTGRES_PASSWORD=<generated-in-1.4>
CUSTODY_HMAC_SECRET=<generated-in-1.4>
CUSTODY_ADMIN_KEY=<generated-in-1.4>
CUSTODY_SEED_PASSWORD=<your-chosen-passphrase>
CUSTODY_SEED_FILE=/opt/custody/seed.enc

# RPC endpoints — replace with your node URLs
ETH_RPC_URL=https://eth-mainnet.g.alchemy.com/v2/<YOUR_KEY>
BTC_RPC_URL=http://bitcoin-node:8332
TRON_RPC_URL=https://api.trongrid.io
SOLANA_RPC_URL=https://api.mainnet-beta.solana.com

# FIDO2 (for approver authentication)
FIDO2_RP_ID=custody.yourdomain.com
FIDO2_RP_ORIGIN=https://custody.yourdomain.com
```

#### Step 3: Generate the encrypted seed

This step runs the seed generator binary inside a temporary container:

```bash
docker compose run --rm --entrypoint custody-seed custody \
  generate --output /opt/custody/seed.enc
# You will be prompted for CUSTODY_SEED_PASSWORD
```

Expected output:
```
Generating BIP-39 compatible seed...
Seed generated successfully.
Encrypted with AES-256-GCM, PBKDF2 600K iterations.
Written to: /opt/custody/seed.enc
IMPORTANT: Back this file up immediately.
```

If `custody-seed` is not yet in the container (image not built), build first:
```bash
docker compose build
```

#### Step 4: Create Shamir backup shares

```bash
docker compose run --rm --entrypoint custody-seed custody \
  split --input /opt/custody/seed.enc --threshold 3 --shares 5
```

Expected output:
```
Share 1/5: [64 hex chars]
Share 2/5: [64 hex chars]
Share 3/5: [64 hex chars]
Share 4/5: [64 hex chars]
Share 5/5: [64 hex chars]

Threshold: 3-of-5 (any 3 shares reconstruct the seed)
Store each share in a separate, secure location.
```

Write each share to paper. Store in separate physical locations. Any 3 of 5 reconstruct.
Do not store shares digitally together.

#### Step 5: Start the service stack

```bash
docker compose up -d
```

Watch startup logs:
```bash
docker compose logs -f custody
```

Expected output within 30 seconds:
```
custody  | INFO custody: Starting MG Custody
custody  | INFO custody::db: Running migrations... 17 migrations applied
custody  | INFO custody::signing: Seed loaded from /opt/custody/seed.enc
custody  | INFO custody::scanner: ETH scanner started, block=21_400_000
custody  | INFO custody::scanner: BTC scanner started, block=876_234
custody  | INFO custody::scanner: SOL scanner started
custody  | INFO custody::scanner: TRX scanner started
custody  | INFO custody::gateway: Listening on 0.0.0.0:8443
```

#### Step 6: Verify health

```bash
# Basic liveness
curl -sf http://localhost:8443/health
# Expected: ok

# Deep health (includes DB, chain connectivity)
curl -s http://localhost:8443/health/deep | jq .
# Expected: {"status":"ok","database":"ok","chains":{...}}

# Per-chain health
curl -s http://localhost:8443/health/chains | jq .
```

If a chain shows `"status":"degraded"`, verify the RPC URL is correct and reachable from the container:
```bash
docker compose exec custody curl -sf $ETH_RPC_URL
```

#### Step 7: Verify all env vars are set

```bash
# The service should show HMAC secret is configured
curl -s http://localhost:8443/health/deep | jq '.hmac_configured'
# Expected: true

# If false, CUSTODY_HMAC_SECRET is not set — the service will reject all API calls
```

#### Reference: All Docker Compose environment variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `POSTGRES_PASSWORD` | Yes | `custody_dev_password` | PostgreSQL password |
| `CUSTODY_HMAC_SECRET` | Yes | (empty — breaks API auth) | 64-hex HMAC key |
| `CUSTODY_ADMIN_KEY` | Yes | (empty — admin disabled) | 32-hex admin key |
| `CUSTODY_SEED_FILE` | Yes | (empty — no signing) | Path to `seed.enc` |
| `CUSTODY_SEED_PASSWORD` | Yes | (empty — seed locked) | Seed decrypt passphrase |
| `ETH_RPC_URL` | If ETH enabled | `https://rpc.sepolia.org` | Ethereum RPC |
| `BTC_RPC_URL` | If BTC enabled | (empty — BTC disabled) | Bitcoin Core RPC |
| `TRON_RPC_URL` | If TRX enabled | (empty — TRX disabled) | Tron API |
| `SOLANA_RPC_URL` | If SOL enabled | Solana mainnet public | Solana RPC |
| `FIDO2_RP_ID` | For FIDO2 auth | `localhost` | Your domain |
| `FIDO2_RP_ORIGIN` | For FIDO2 auth | `http://localhost:8443` | Full origin URL |
| `RUST_LOG` | No | `info,custody=debug` | Log level |

---

### Method B: Binary from Source

Use this method when you need to run on bare metal, configure systemd, or customise the build.

#### Step 1: Install build dependencies

```bash
# Debian/Ubuntu
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config libssl-dev \
  curl git

# RHEL/Rocky
sudo dnf groupinstall -y "Development Tools"
sudo dnf install -y openssl-devel pkg-config curl git
```

Install Rust (if not present):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

Install Node.js 22 (for dashboard build only):
```bash
# Using nvm
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.7/install.sh | bash
source ~/.bashrc
nvm install 22
nvm use 22
```

#### Step 2: Clone and build

```bash
git clone <repo-url> /opt/mg-custody
cd /opt/mg-custody

# Build dashboard
cd dashboard
npm ci
npm run build
cd ..

# Build all Rust binaries (release mode)
cargo build --release \
  --bin custody \
  --bin custody-signer \
  --bin custody-admin \
  --bin custody-seed

# Expected build time: 5–15 minutes on first build
# Binaries land in: target/release/
```

#### Step 3: Install binaries

```bash
sudo install -m 755 target/release/custody        /usr/local/bin/custody
sudo install -m 755 target/release/custody-signer /usr/local/bin/custody-signer
sudo install -m 755 target/release/custody-admin  /usr/local/bin/custody-admin
sudo install -m 755 target/release/custody-seed   /usr/local/bin/custody-seed

# Install dashboard and config
sudo mkdir -p /opt/custody/dashboard /opt/custody/config
sudo cp -r dashboard/build /opt/custody/dashboard/
sudo cp config/default.toml /opt/custody/config/
```

#### Step 4: Create system user

```bash
sudo groupadd -r custody
sudo useradd -r -g custody -d /opt/custody -s /sbin/nologin custody
sudo chown -R custody:custody /opt/custody
```

#### Step 5: Create the configuration file

```bash
sudo cp /opt/mg-custody/config/default.toml /opt/custody/config/local.toml
sudo chown custody:custody /opt/custody/config/local.toml
sudo chmod 640 /opt/custody/config/local.toml
```

Edit `/opt/custody/config/local.toml`:
```toml
[api]
bind = "127.0.0.1:8443"  # Bind to loopback; nginx handles TLS termination

[database]
url = "postgres://custody:<POSTGRES_PASSWORD>@localhost:5432/custody"
max_connections = 20

[exchange]
webhook_url = "https://your-exchange.com/api/custody-events"

[key_manager]
backend = "software"
seed_file = "/opt/custody/seed.enc"

[limits]
max_single_withdrawal_usd = 500000
max_hourly_outflow_usd = 10000000
max_daily_outflow_usd = 50000000
auto_approve_threshold_usd = 1000

[wallets]
hot_max_pct = 5
warm_max_pct = 15

[chains.bitcoin]
enabled = true
rpc_url = "http://127.0.0.1:8332"
required_confirmations = 3

[chains.ethereum]
enabled = true
rpc_url = "https://eth-mainnet.g.alchemy.com/v2/<YOUR_KEY>"
required_confirmations = 12
max_gas_price_gwei = 200

[chains.ethereum.tokens]
"0xdAC17F958D2ee523a2206206994597C13D831ec7" = { symbol = "USDT", decimals = 6 }
"0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48" = { symbol = "USDC", decimals = 6 }

[chains.tron]
enabled = true
api_url = "https://api.trongrid.io"
required_confirmations = 20

[chains.tron.tokens]
"TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t" = { symbol = "USDT", decimals = 6 }

[chains.solana]
enabled = true
rpc_url = "https://api.mainnet-beta.solana.com"
commitment = "finalized"

[chains.solana.tokens]
"Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB" = { symbol = "USDT", decimals = 6 }
"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" = { symbol = "USDC", decimals = 6 }
```

#### Step 6: Create the environment file

```bash
sudo tee /etc/custody/environment << 'EOF'
DATABASE_URL=postgres://custody:<PASSWORD>@localhost:5432/custody
CUSTODY_HMAC_SECRET=<64-hex>
CUSTODY_ADMIN_KEY=<32-hex>
CUSTODY_SEED_FILE=/opt/custody/seed.enc
CUSTODY_SEED_PASSWORD=<passphrase>
CUSTODY_DASHBOARD_DIR=/opt/custody/dashboard
RUST_LOG=info,custody=debug
EOF
sudo chmod 600 /etc/custody/environment
sudo chown custody:custody /etc/custody/environment
```

#### Step 7: Run database migrations

```bash
export DATABASE_URL="postgres://custody:<PASSWORD>@localhost:5432/custody"

# Create database
createdb -U postgres custody
createuser -U postgres custody
psql -U postgres -c "GRANT ALL ON DATABASE custody TO custody;"

# Apply all migrations in order
for f in /opt/mg-custody/crates/db/migrations/*.sql; do
  echo "Applying $f..."
  psql "$DATABASE_URL" -f "$f"
done

# Verify tables exist
psql "$DATABASE_URL" -c "\dt" | head -40
```

Expected: 30+ tables including `addresses`, `deposits`, `withdrawals`, `ledger_entries`,
`circuit_breaker_state`, `audit_log`, etc.

#### Step 8: Generate seed

```bash
sudo -u custody custody-seed generate --output /opt/custody/seed.enc
# Enter CUSTODY_SEED_PASSWORD when prompted

# Shamir backup
sudo -u custody custody-seed split \
  --input /opt/custody/seed.enc \
  --threshold 3 --shares 5

sudo chmod 600 /opt/custody/seed.enc
sudo chown custody:custody /opt/custody/seed.enc
```

#### Step 9: Install systemd service

Create `/etc/systemd/system/custody.service`:

```ini
[Unit]
Description=MG Custody Service
Documentation=https://github.com/your-org/mg-custody
After=network.target postgresql.service
Requires=postgresql.service

[Service]
Type=simple
User=custody
Group=custody
WorkingDirectory=/opt/custody
EnvironmentFile=/etc/custody/environment
ExecStart=/usr/local/bin/custody
Restart=on-failure
RestartSec=5s
TimeoutStartSec=60s
TimeoutStopSec=30s

# Security hardening
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=strict
ReadWritePaths=/opt/custody
ProtectHome=yes
CapabilityBoundingSet=
AmbientCapabilities=
SystemCallFilter=@system-service
LockPersonality=yes
RestrictRealtime=yes
MemoryDenyWriteExecute=yes

# Logs go to journald
StandardOutput=journal
StandardError=journal
SyslogIdentifier=custody

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl daemon-reload
sudo systemctl enable custody
sudo systemctl start custody
sudo systemctl status custody
```

Check logs:
```bash
sudo journalctl -u custody -f
```

#### Step 10: Reverse proxy (nginx)

Install nginx:
```bash
sudo apt-get install -y nginx certbot python3-certbot-nginx
```

Create `/etc/nginx/sites-available/custody`:

```nginx
upstream custody_backend {
    server 127.0.0.1:8443;
    keepalive 32;
}

server {
    listen 443 ssl http2;
    server_name custody.yourdomain.com;

    ssl_certificate     /etc/letsencrypt/live/custody.yourdomain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/custody.yourdomain.com/privkey.pem;
    ssl_protocols       TLSv1.3;
    ssl_ciphers         ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384;
    ssl_prefer_server_ciphers on;

    # Only allow exchange backend IPs
    allow <EXCHANGE_IP>;
    deny all;

    location / {
        proxy_pass         http://custody_backend;
        proxy_http_version 1.1;
        proxy_set_header   Host $host;
        proxy_set_header   X-Real-IP $remote_addr;
        proxy_set_header   Connection "";
        proxy_read_timeout 60s;
        client_max_body_size 1m;
    }
}

server {
    listen 80;
    server_name custody.yourdomain.com;
    return 301 https://$server_name$request_uri;
}
```

Get TLS certificate and activate:
```bash
sudo certbot --nginx -d custody.yourdomain.com
sudo ln -s /etc/nginx/sites-available/custody /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

---

## 3. Initial Configuration

### 3.1 Generate HMAC Secret

The HMAC secret authenticates every API call from the exchange. Generate it once and record it in
both the custody service and the exchange configuration.

```bash
openssl rand -hex 32
# Example output: a3f9e2c1b4d8f7a0e5c9d2b6f1a4e7c0d3b8f2a5e1c4d9b7f0a3e6c2b5d8f1a4
```

Set this same value in:
1. `CUSTODY_HMAC_SECRET` on the custody host
2. The exchange service's custody client configuration (secret used to sign requests)

### 3.2 Generate and Back Up the Seed

This is the most critical step. The seed is the master key for all funds on all chains.
Loss of the seed = loss of all funds. Compromise of the seed = loss of all funds.

```bash
# Generate seed (prompts for password)
custody-seed generate --output /secure/path/seed.enc

# Verify the seed is readable with the correct password
custody-seed verify --input /secure/path/seed.enc
# Expected: "Seed file valid. Decryption successful."

# Create 3-of-5 Shamir shares
custody-seed split --input /secure/path/seed.enc --threshold 3 --shares 5

# Test reconstruction from 3 shares
custody-seed reconstruct \
  --share1 "<share1>" \
  --share2 "<share2>" \
  --share3 "<share3>" \
  --output /tmp/reconstructed.enc

# Verify the reconstructed seed matches the original
custody-seed verify --input /tmp/reconstructed.enc
rm /tmp/reconstructed.enc
```

Store each Shamir share in a separate location. Recommended:
- Share 1: Primary data center (sealed envelope in server room safe)
- Share 2: Secondary data center or office safe
- Share 3: Key custodian's personal safe (not on-site)
- Share 4: Bank safety deposit box
- Share 5: Trusted legal counsel or escrow

### 3.3 Configure Chain RPC Endpoints

Each chain requires a reliable RPC endpoint. Options:

**Ethereum**: Alchemy, Infura, QuickNode, or your own `geth`/`reth` node.
```bash
# Test your Ethereum RPC
curl -X POST $ETH_RPC_URL \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
# Expected: {"jsonrpc":"2.0","id":1,"result":"0x..."}
```

**Bitcoin**: Your own Bitcoin Core node (strongly recommended for production).
```bash
# Test Bitcoin Core RPC
curl -u $BTC_RPC_USER:$BTC_RPC_PASSWORD \
  -X POST http://localhost:8332 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"1.0","method":"getblockcount","params":[]}'
# Expected: {"result":876234,"error":null,"id":null}
```

**Tron**: Trongrid (public) or a private Tron Full Node.
```bash
curl https://api.trongrid.io/wallet/getnowblock | jq '.block_header.raw_data.number'
# Expected: a large integer (current block)
```

**Solana**: Solana public RPC is acceptable for low volume. For production, use Helius, QuickNode,
or your own validator.
```bash
curl -X POST https://api.mainnet-beta.solana.com \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"getSlot"}'
# Expected: {"jsonrpc":"2.0","result":323456789,"id":1}
```

### 3.4 Set Circuit Breaker Limits

Default limits are in `config/default.toml`. Adjust them to match your risk parameters.
The limits are also stored in the `system_config` PostgreSQL table and take effect on restart.

```bash
# View current limits
curl -s -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  http://localhost:8443/v1/system/circuit-breaker | jq .

# Expected:
# {
#   "max_single_withdrawal_usd": "500000",
#   "max_hourly_outflow_usd": "10000000",
#   "max_daily_outflow_usd": "50000000",
#   "current_hourly_outflow": "0",
#   "current_daily_outflow": "0",
#   "status": "open"
# }
```

To change limits, update `config/local.toml` and restart the service.
The circuit breaker state (counters) is persisted in PostgreSQL and survives restarts.

### 3.5 Create the First Admin User

```bash
# Create admin user via CLI
custody-admin user create \
  --email admin@yourdomain.com \
  --role admin

# Expected output includes a one-time setup token
# Use the token to set a password via the dashboard at https://custody.yourdomain.com
```

### 3.6 Register a FIDO2 Key for Approvals

FIDO2/WebAuthn is required for approvals of withdrawals in Tier 2 ($10K–$100K).

Ensure `FIDO2_RP_ID` and `FIDO2_RP_ORIGIN` match your production domain before registering any keys.
Keys registered with `localhost` cannot be used on the production domain.

```bash
# Verify FIDO2 config
curl -s http://localhost:8443/v1/auth/fido2/config | jq .rp_id
# Expected: "custody.yourdomain.com"

# Register via the web dashboard
# Navigate to: https://custody.yourdomain.com/settings/security
# Click "Add Security Key" and follow the browser prompt
```

---

## 4. Security Hardening

> Read [THREAT_MODEL.md](../THREAT_MODEL.md) before this section.
> The mitigations here are only as effective as their deployment status.
> Items marked NOT YET AVAILABLE are tracked in Phase 17 of the roadmap.

### 4.1 Signer Isolation

**Current status**: Phase 17B — NOT YET AVAILABLE in production.
The signer runs inside the same `custody` binary as the API gateway.

**Target architecture** (when 17B ships):

Run `custody-signer` on a separate host with no network interface except a Unix socket
or a private internal link to the main `custody` host.

```bash
# On the signer host (no internet access):
export SIGNER_SOCKET=/var/run/custody/signer.sock
export SIGNER_IPC_SECRET=$(openssl rand -hex 32)  # shared with main host
custody-signer

# On the main custody host:
export SIGNER_SOCKET=/var/run/custody/signer.sock
export SIGNER_IPC_SECRET=<SAME_SECRET>
custody
```

Until 17B ships, do not expose the API on a public interface. Use firewall rules to restrict
access to the exchange backend IP range only (section 1.3).

### 4.2 HSM Mode

**Current status**: Phase 4C — NOT YET AVAILABLE. `hsm` backend returns `unimplemented!()`.

When 4C ships, configure PKCS#11:

```toml
# config/local.toml
[key_manager]
backend = "hsm"
pkcs11_library = "/usr/lib/softhsm/libsofthsm2.so"  # or Thales library
pkcs11_slot = 0
pkcs11_pin = ""  # set via CUSTODY_HSM_PIN env var
```

For development testing with SoftHSM2:
```bash
sudo apt-get install -y softhsm2
softhsm2-util --init-token --slot 0 --label "custody" --pin 1234 --so-pin 1234
```

### 4.3 Network Zone Firewall Rules

Implement the three-zone architecture from [THREAT_MODEL.md](../THREAT_MODEL.md):

```bash
# Zone 1 → Zone 2: allow API to reach internal services
iptables -A FORWARD -i eth0 -o eth1 -p tcp --dport 8443 -j ACCEPT

# Zone 2 → Zone 3: allow pipeline to reach signer only on the IPC socket port
iptables -A FORWARD -i eth1 -o eth2 -p tcp --dport 9001 -j ACCEPT

# Block Zone 3 from initiating any outbound connections
iptables -A OUTPUT -o eth2 -j DROP
iptables -A FORWARD -i eth2 -j DROP
```

### 4.4 TLS Certificate Management

Use Let's Encrypt for external-facing TLS (nginx) or a self-signed cert for internal communication.

```bash
# Self-signed cert for internal use (custody ↔ signer TLS if needed)
openssl req -x509 -nodes -days 365 \
  -newkey rsa:4096 \
  -keyout /etc/custody/tls/server.key \
  -out /etc/custody/tls/server.crt \
  -subj "/CN=custody.internal"

chmod 600 /etc/custody/tls/server.key
chown custody:custody /etc/custody/tls/server.key
```

Renew Let's Encrypt certificates automatically:
```bash
# Add to crontab
0 12 * * * certbot renew --quiet && systemctl reload nginx
```

### 4.5 OS-Level Hardening

```bash
# Disable core dumps for the custody process (prevents seed exposure)
echo "custody soft core 0" >> /etc/security/limits.conf
echo "custody hard core 0" >> /etc/security/limits.conf

# Lock memory to prevent swap (requires CAP_IPC_LOCK or running as root — use with care)
echo "custody soft memlock unlimited" >> /etc/security/limits.conf

# Restrict ptrace (prevents debugger attachment)
echo "kernel.yama.ptrace_scope = 2" >> /etc/sysctl.d/99-custody.conf
sysctl -p /etc/sysctl.d/99-custody.conf
```

### 4.6 FIDO2 Domain Configuration

Do not use `localhost` for FIDO2 in production. Once users register FIDO2 keys for a given
`rp_id`, they cannot use those keys with a different domain.

```bash
# Set before first user registers a FIDO2 key
FIDO2_RP_ID=custody.yourdomain.com
FIDO2_RP_ORIGIN=https://custody.yourdomain.com
```

### 4.7 Pre-Deployment Security Checklist

- [ ] `CUSTODY_HMAC_SECRET` is set to a random 64-hex string (not the default empty value)
- [ ] `CUSTODY_ADMIN_KEY` is set to a random 32-hex string
- [ ] `POSTGRES_PASSWORD` is set (not `custody_dev_password`)
- [ ] `CUSTODY_SEED_PASSWORD` is a strong passphrase (20+ characters)
- [ ] `seed.enc` exists and Shamir backup shares are stored in separate locations
- [ ] Seed reconstruction was tested successfully before live deployment
- [ ] Port 8443 is reachable only from exchange backend IP
- [ ] Port 5432 is reachable only from localhost
- [ ] `FIDO2_RP_ID` matches production domain
- [ ] TLS is configured on the reverse proxy
- [ ] Monitoring is collecting `/metrics` (section 5.3)
- [ ] Alert channels are configured (section 5.3)
- [ ] Read [THREAT_MODEL.md](../THREAT_MODEL.md) and accepted the known limitations
- [ ] Read [DEPLOYMENT_MODES.md](DEPLOYMENT_MODES.md) and chose appropriate mode

---

## 5. Operational Procedures

### Key Rotation

#### Quarterly Schedule

Key rotation should happen on a fixed schedule, not only when a compromise is suspected.
Recommended schedule: every 90 days.

The master seed itself is not rotated — that would require sweeping all funds to new addresses.
What rotates: HMAC secrets, admin keys, database passwords.

#### Rotate HMAC Secret (API Auth)

Coordinate with the exchange team — both sides must update at the same time.

```bash
# 1. Generate new secret
NEW_HMAC=$(openssl rand -hex 32)

# 2. Inform exchange team of cutover time (T+0)

# 3. At T+0 on custody host:
# Update /etc/custody/environment (or .env for Docker)
# Change CUSTODY_HMAC_SECRET to $NEW_HMAC
# Restart service
sudo systemctl restart custody   # or: docker compose restart custody

# 4. At T+0 on exchange side:
# Update the custody client configuration with the new HMAC secret

# 5. Verify API calls succeed from exchange
# Watch custody logs for any 401 Unauthorized errors
sudo journalctl -u custody -f | grep -E "401|Unauthorized|hmac"
```

**Note**: There is a brief window between restart and exchange cutover where API calls fail.
Coordinate the timing to minimize this window. The service can handle replayed requests within
the 30-second window, so both sides do not need to be updated to the exact same second.

#### Rotate Admin Key

```bash
NEW_ADMIN_KEY=$(openssl rand -hex 16)

# Update environment and restart
# No coordination needed — admin key is only used by operators
sudo sed -i "s/CUSTODY_ADMIN_KEY=.*/CUSTODY_ADMIN_KEY=${NEW_ADMIN_KEY}/" /etc/custody/environment
sudo systemctl restart custody
```

#### Rotate Database Password

```bash
# 1. Generate new password
NEW_PG_PASS=$(openssl rand -hex 24)

# 2. Update PostgreSQL
sudo -u postgres psql -c "ALTER USER custody PASSWORD '${NEW_PG_PASS}';"

# 3. Update environment file
sudo sed -i "s|postgres://custody:.*@|postgres://custody:${NEW_PG_PASS}@|" /etc/custody/environment
sudo sed -i "s/POSTGRES_PASSWORD=.*/POSTGRES_PASSWORD=${NEW_PG_PASS}/" /etc/custody/environment

# 4. Restart service
sudo systemctl restart custody

# 5. Verify
curl -sf http://localhost:8443/health
```

#### Seed Rotation (Emergency Only)

Seed rotation requires sweeping all funds to new addresses. This is an emergency procedure —
do not perform it on a schedule.

Trigger conditions: seed file confirmed compromised, share holder reports loss of 3+ shares,
suspicion that the master key was exposed.

```bash
# This procedure requires stopping the service and involves real fund movement.
# Do not attempt without reading all steps first.

# 1. Freeze all withdrawals immediately
curl -X POST http://localhost:8443/v1/system/freeze \
  -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  -d '{"reason": "Seed rotation — emergency procedure"}'

# 2. Generate new seed on an air-gapped machine if possible
custody-seed generate --output /secure/new-seed.enc

# 3. Generate new Shamir shares for new seed
custody-seed split --input /secure/new-seed.enc --threshold 3 --shares 5

# 4. For each chain, derive all old and new HD paths
# 5. Construct sweep transactions moving all funds from old addresses to new addresses
# 6. Wait for all sweeps to confirm
# 7. Update CUSTODY_SEED_FILE to point to new seed
# 8. Restart service
# 9. Unfreeze withdrawals

# Contact support@mg-custody.dev before attempting this procedure.
```

### Backup and Recovery

#### PostgreSQL Backup

All custody state lives in PostgreSQL. The encrypted seed file must be backed up separately.

**Daily backup** (add to cron):
```bash
#!/bin/bash
# /usr/local/bin/custody-backup.sh
set -euo pipefail

BACKUP_DIR="/var/backups/custody"
DATE=$(date +%Y-%m-%d-%H%M)
DB_URL="${DATABASE_URL}"

mkdir -p "$BACKUP_DIR"

# Dump with compression
pg_dump "$DB_URL" \
  --format=custom \
  --compress=9 \
  --file="${BACKUP_DIR}/custody-${DATE}.pgdump"

# Keep 30 days of backups
find "$BACKUP_DIR" -name "*.pgdump" -mtime +30 -delete

echo "Backup complete: ${BACKUP_DIR}/custody-${DATE}.pgdump"
```

```bash
chmod +x /usr/local/bin/custody-backup.sh
echo "0 3 * * * custody /usr/local/bin/custody-backup.sh >> /var/log/custody-backup.log 2>&1" | crontab -
```

**Verify backup integrity daily**:
```bash
# Test restore to a temporary database
pg_restore --list "${BACKUP_DIR}/custody-${DATE}.pgdump" > /dev/null
echo "Backup valid: $(du -sh "${BACKUP_DIR}/custody-${DATE}.pgdump")"
```

**Offsite replication**: Copy backups to a separate host or object store.
```bash
# Example: rsync to backup host
rsync -az /var/backups/custody/ backup-host:/var/backups/custody/

# Example: AWS S3
aws s3 sync /var/backups/custody/ s3://your-backup-bucket/custody/
```

#### Seed File Backup

The seed file is already protected by AES-256-GCM encryption. Back it up to multiple locations.

```bash
# Copy seed to backup locations
rsync -a /opt/custody/seed.enc backup-host:/backups/custody/seed.enc
aws s3 cp /opt/custody/seed.enc s3://your-backup-bucket/custody/seed.enc

# The Shamir shares on paper are the primary physical backup.
# The seed.enc backup is a secondary digital copy.
```

#### Recovery Procedure

**Scenario 1: Lost PostgreSQL data (hardware failure)**

Recovery time objective: < 1 hour from most recent backup.

```bash
# 1. Install fresh PostgreSQL 16
# 2. Create user and database
sudo -u postgres psql << 'EOF'
CREATE USER custody WITH PASSWORD '<password>';
CREATE DATABASE custody OWNER custody;
EOF

# 3. Restore from backup
pg_restore \
  --dbname postgres://custody:<password>@localhost:5432/custody \
  --format=custom \
  /path/to/custody-YYYY-MM-DD.pgdump

# 4. Verify table count
psql "$DATABASE_URL" -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='public';"
# Expected: ~30

# 5. Start service
sudo systemctl start custody
curl -sf http://localhost:8443/health
```

**Scenario 2: Lost seed.enc file (disk failure, no backup)**

This requires Shamir share reconstruction. Collect any 3 of 5 shares.

```bash
# Reconstruct seed from shares
custody-seed reconstruct \
  --share1 "<share1>" \
  --share2 "<share2>" \
  --share3 "<share3>" \
  --output /opt/custody/seed.enc

# Set permissions
chmod 600 /opt/custody/seed.enc
chown custody:custody /opt/custody/seed.enc

# Verify reconstruction
custody-seed verify --input /opt/custody/seed.enc

# Start service
sudo systemctl start custody
```

**Scenario 3: Complete host loss (fire, flood)**

1. Provision new host with same OS.
2. Follow Method B installation (section 2) up through database migration.
3. Restore PostgreSQL backup (Scenario 1 above).
4. Reconstruct seed from Shamir shares (Scenario 2 above) or restore from seed backup.
5. Start service and verify.
6. Update exchange configuration if IP address changed.

### Monitoring

#### Prometheus Scrape Configuration

The service exposes metrics at `GET /metrics` (no authentication required).

Add to `prometheus.yml`:
```yaml
scrape_configs:
  - job_name: mg-custody
    static_configs:
      - targets: ['custody.internal:8443']
    scrape_interval: 15s
    metrics_path: /metrics
```

#### Key Metrics

| Metric | Type | Alert Threshold | Meaning |
|--------|------|----------------|---------|
| `custody_chain_scan_lag_blocks{chain}` | Gauge | > 50 blocks | Scanner is behind — deposits may be delayed |
| `custody_chain_rpc_latency_seconds{chain}` | Histogram | p99 > 5s | RPC node is slow |
| `custody_withdrawals_total{status="failed"}` | Counter | Any increase | Withdrawal failures |
| `custody_signing_requests_total{result="error"}` | Counter | Any increase | Signing failures |
| `custody_permission_denials_total` | Counter | Spike | Possible API abuse |
| `custody_wallet_balance{tier="hot"}` | Gauge | < 1% of AUM | Hot wallet underfunded |
| `custody_deposit_amount_total{chain}` | Counter | Sudden stop | Scanner down |

#### Grafana Dashboard Import

A pre-built Grafana dashboard JSON is available at `/opt/mg-custody/config/grafana-dashboard.json`.

Import via Grafana UI:
```
Dashboards → Import → Upload JSON file → select grafana-dashboard.json
```

Or via Grafana API:
```bash
curl -X POST \
  -H "Content-Type: application/json" \
  -d @/opt/mg-custody/config/grafana-dashboard.json \
  http://admin:admin@grafana.internal:3000/api/dashboards/import
```

#### Alert Rules (Prometheus Alertmanager)

```yaml
# custody-alerts.yml
groups:
  - name: custody
    rules:
      - alert: CustodyScanLag
        expr: custody_chain_scan_lag_blocks > 50
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Chain {{ $labels.chain }} scanner is {{ $value }} blocks behind"

      - alert: CustodyRpcDown
        expr: up{job="mg-custody"} == 0
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "MG Custody service is down"

      - alert: CustodyWithdrawalFailures
        expr: increase(custody_withdrawals_total{status="failed"}[5m]) > 0
        labels:
          severity: warning
        annotations:
          summary: "{{ $value }} withdrawal failures in last 5 minutes"

      - alert: CustodyCircuitBreakerTripped
        expr: custody_circuit_breaker_status == 1
        labels:
          severity: critical
        annotations:
          summary: "Circuit breaker has tripped — all withdrawals blocked"

      - alert: CustodyHotWalletLow
        expr: custody_wallet_balance{tier="hot"} / ignoring(tier) sum(custody_wallet_balance) < 0.01
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Hot wallet below 1% of AUM — sweep from warm may be needed"
```

Apply:
```bash
cp custody-alerts.yml /etc/prometheus/rules/
systemctl reload prometheus
```

#### PagerDuty Integration

Configure Alertmanager to page on critical alerts:
```yaml
# alertmanager.yml
route:
  group_by: ['alertname']
  receiver: 'custody-team'
  routes:
    - match:
        severity: critical
      receiver: pagerduty

receivers:
  - name: custody-team
    slack_configs:
      - channel: '#custody-alerts'
        api_url: 'https://hooks.slack.com/services/...'
  - name: pagerduty
    pagerduty_configs:
      - routing_key: '<PAGERDUTY_INTEGRATION_KEY>'
```

### Incident Response

#### Circuit Breaker Tripped

**Symptom**: All withdrawals are rejected with `circuit_breaker_tripped` error.
PagerDuty alert fires. Users cannot withdraw.

**What it means**: Outflow rate exceeded configured limits. This may indicate:
- Legitimate high-volume withdrawal day (trading desk, large client)
- Compromised API key submitting fraudulent withdrawals
- Bug in the withdrawal pipeline

**Steps**:

```bash
# 1. Check current circuit breaker state
curl -s -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  http://localhost:8443/v1/system/circuit-breaker | jq .

# 2. Review recent withdrawals — do they look legitimate?
curl -s -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  "http://localhost:8443/v1/withdrawals?limit=20&order=desc" | jq .

# 3. Check audit log for unusual patterns
curl -s -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  "http://localhost:8443/v1/audit?limit=50&action=withdrawal" | jq .

# 4. If legitimate (exchange business): reset the circuit breaker
# ONLY DO THIS if you have confirmed the withdrawals are not fraudulent
curl -X POST -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  http://localhost:8443/v1/system/circuit-breaker/reset \
  -d '{"reason": "Legitimate high volume day — confirmed by ops team"}'

# 5. If suspicious: keep circuit breaker tripped, investigate further
# Freeze all withdrawals manually for investigation
curl -X POST -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  http://localhost:8443/v1/system/freeze \
  -d '{"reason": "Investigating potential breach"}'
```

**Resolution time target**: 30 minutes to determine legitimate vs. attack.

#### Suspected Compromise — Emergency Freeze

**Trigger**: Unexplained large outflows, unauthorized API access, leaked HMAC secret,
suspected insider threat.

**Steps** (execute in order, do not skip):

```bash
# STEP 1: Freeze all withdrawals immediately (< 1 minute)
curl -X POST -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  http://localhost:8443/v1/system/freeze \
  -d '{"reason": "Security incident — investigating"}'

# STEP 2: Rotate HMAC secret immediately (< 2 minutes)
NEW_HMAC=$(openssl rand -hex 32)
# Update environment and restart
sudo sed -i "s/CUSTODY_HMAC_SECRET=.*/CUSTODY_HMAC_SECRET=${NEW_HMAC}/" /etc/custody/environment
sudo systemctl restart custody
# Coordinate with exchange team to update their HMAC secret

# STEP 3: Revoke all active API keys (< 5 minutes)
curl -X POST -H "X-Custody-Key: $NEW_ADMIN_KEY" \
  http://localhost:8443/v1/admin/api-keys/revoke-all

# STEP 4: Take a forensic snapshot of the database BEFORE any changes
pg_dump "$DATABASE_URL" \
  --format=custom \
  --file="/forensics/snapshot-$(date +%Y%m%d-%H%M%S).pgdump"

# STEP 5: Notify your security team and legal counsel
# STEP 6: Preserve logs before rotation
sudo journalctl -u custody > /forensics/custody-logs-$(date +%Y%m%d).txt

# STEP 7: Assess what was accessed
# Check audit log for signing operations during the suspected window
curl -s -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  "http://localhost:8443/v1/audit?action=signing&from=<incident_start>&limit=1000" | jq .

# STEP 8: Do NOT restart or wipe the server until forensics are complete
```

Do not unfreeze withdrawals until:
1. Attack vector is understood
2. Leaked credentials are rotated
3. Security review is complete

#### Lost FIDO2 Key

**Symptom**: Approver cannot authenticate for Tier 2+ withdrawals.

```bash
# Use backup recovery code generated at FIDO2 registration
curl -X POST http://localhost:8443/v1/auth/fido2/recover \
  -H "Content-Type: application/json" \
  -d '{"email": "approver@yourdomain.com", "recovery_code": "<code>"}'

# After authentication, register a new FIDO2 key via the dashboard
# Then revoke the lost key
curl -X DELETE -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  http://localhost:8443/v1/auth/fido2/keys/<lost_key_id>
```

If recovery codes are also lost, an admin can manually approve the specific withdrawal:
```bash
curl -X POST -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  http://localhost:8443/v1/withdrawals/<withdrawal_id>/admin-approve \
  -d '{"reason": "FIDO2 key lost — admin override, recorded in audit log"}'
```

Admin overrides are written to the audit log and should be reviewed.

#### Database Corruption — Point-in-Time Recovery

**Symptom**: Postgres errors in custody logs, balance discrepancies detected by reconciliation.

```bash
# 1. Stop the custody service
sudo systemctl stop custody

# 2. Identify the last known-good state
# Check reconciliation log for last successful reconciliation timestamp
psql "$DATABASE_URL" -c "
  SELECT created_at, status, details
  FROM reconciliation_runs
  ORDER BY created_at DESC
  LIMIT 5;"

# 3. Restore from backup to a target time
pg_restore \
  --dbname postgres://custody:<password>@localhost:5432/custody_recovery \
  --format=custom \
  /var/backups/custody/custody-<date>.pgdump

# 4. Compare ledger balances between current and recovered DB
# If current DB has corruption, switch DATABASE_URL to custody_recovery

# 5. Start service and verify
sudo systemctl start custody
curl -sf http://localhost:8443/health

# 6. Run manual reconciliation
curl -X POST -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  http://localhost:8443/v1/admin/reconcile-now
```

---

## 6. Upgrades

### Rolling Upgrade Procedure

MG Custody maintains all state in PostgreSQL, so upgrades are stateless: stop, replace binary,
run new migrations, start.

```bash
# 1. Pull new code
cd /opt/mg-custody
git fetch origin
git log --oneline HEAD..origin/main  # review what changed

# 2. Build new binaries (while service is still running)
cargo build --release --bin custody --bin custody-admin --bin custody-seed --bin custody-signer

# 3. Check if there are new migrations
ls crates/db/migrations/

# 4. Run new migrations (safe to run on live DB — migrations are idempotent)
export DATABASE_URL="postgres://custody:<password>@localhost:5432/custody"
for f in crates/db/migrations/*.sql; do
  echo "Applying $f..."
  psql "$DATABASE_URL" -f "$f" 2>&1 | grep -v "already exists" || true
done

# 5. Install new binaries
sudo systemctl stop custody
sudo install -m 755 target/release/custody /usr/local/bin/custody
sudo systemctl start custody

# 6. Verify health
curl -sf http://localhost:8443/health
sudo journalctl -u custody -n 50
```

#### Docker Compose Upgrade

```bash
cd /opt/mg-custody
git pull origin main

# Rebuild image
docker compose build

# Run migrations (against existing DB)
docker compose run --rm --entrypoint bash custody -c "
  for f in /app/crates/db/migrations/*.sql; do
    psql \$DATABASE_URL -f \$f 2>&1 || true
  done"

# Replace running container
docker compose up -d --no-deps custody

# Verify
docker compose ps
curl -sf http://localhost:8443/health
```

### Database Migrations

Each `crates/db/migrations/NNN_*.sql` file is numbered and must be applied in order.
They are designed to be idempotent (using `IF NOT EXISTS`, `CREATE OR REPLACE`).

```bash
# Apply all migrations
for f in crates/db/migrations/*.sql; do
  echo "=== $f ==="
  psql "$DATABASE_URL" -f "$f"
done

# Or apply only new ones (track applied migrations in DB or by filename)
LAST_APPLIED=009  # example: last applied was 009_circuit_breaker.sql
for f in crates/db/migrations/*.sql; do
  NUM=$(basename "$f" | cut -d_ -f1)
  if [ "$NUM" -gt "$LAST_APPLIED" ]; then
    echo "Applying $f..."
    psql "$DATABASE_URL" -f "$f"
  fi
done
```

### Configuration Changes

Environment variable additions are picked up on service restart.
Configuration file changes (`config/local.toml`) are also applied on restart.

No configuration is hot-reloaded — a restart is always required.

### Rollback Procedure

If an upgrade fails (health check degraded, errors in logs):

```bash
# 1. Stop service
sudo systemctl stop custody

# 2. Restore previous binary
# If you kept the old binary
sudo install -m 755 /tmp/custody.backup /usr/local/bin/custody

# Or rebuild from the previous git commit
git stash  # or checkout previous tag
cargo build --release --bin custody
sudo install -m 755 target/release/custody /usr/local/bin/custody

# 3. If the new migration added columns or tables (additive, safe to leave)
# the old binary will still work — PostgreSQL is forward-compatible for additive changes

# 4. If the migration removed columns the old binary needs:
# Restore database from pre-upgrade backup
pg_restore --dbname "$DATABASE_URL" --clean /var/backups/custody/pre-upgrade.pgdump

# 5. Start service
sudo systemctl start custody
curl -sf http://localhost:8443/health
```

Before every upgrade, create a pre-upgrade backup:
```bash
pg_dump "$DATABASE_URL" --format=custom --file="/var/backups/custody/pre-upgrade-$(date +%Y%m%d).pgdump"
cp /usr/local/bin/custody /tmp/custody.backup
```

---

## 7. Troubleshooting

### Common Errors and Solutions

#### `CUSTODY_HMAC_SECRET not set — API authentication disabled`

```
ERROR custody::gateway: CUSTODY_HMAC_SECRET not set — API authentication disabled
```

The `CUSTODY_HMAC_SECRET` environment variable is empty. All API calls will fail with 401.

```bash
# Fix: set the variable and restart
export CUSTODY_HMAC_SECRET=$(openssl rand -hex 32)
sudo systemctl restart custody
```

#### `Failed to decrypt seed: InvalidTag`

```
ERROR custody::signing: Failed to decrypt seed: InvalidTag
```

The seed password is wrong, the seed file is corrupt, or the seed file was replaced.

```bash
# Test decryption separately
custody-seed verify --input /opt/custody/seed.enc
# If this fails, the password is wrong or the file is corrupt

# If password forgotten, reconstruct from Shamir shares
custody-seed reconstruct --share1 X --share2 Y --share3 Z --output /opt/custody/seed.enc
```

#### `DB connection error: too many connections`

```
ERROR sqlx: DB connection error: too many connections
```

Increase PostgreSQL's `max_connections` or reduce the service's connection pool size.

```bash
# Check current connections
psql "$DATABASE_URL" -c "SELECT count(*) FROM pg_stat_activity WHERE datname='custody';"

# Increase max_connections in PostgreSQL config
sudo echo "max_connections = 200" >> /etc/postgresql/16/main/postgresql.conf
sudo systemctl restart postgresql

# Or reduce pool in config/local.toml
# [database]
# max_connections = 10  # reduce this
```

#### `RPC error: connection refused` for Bitcoin

```
ERROR custody::scanner::bitcoin: RPC error: connection refused http://localhost:8332
```

Bitcoin Core is not running or the RPC credentials are wrong.

```bash
# Check Bitcoin Core is running
bitcoin-cli -rpcuser=$BTC_RPC_USER -rpcpassword=$BTC_RPC_PASSWORD getblockcount

# Check RPC is enabled in bitcoin.conf
grep "rpcallowip\|server\|rpcuser" ~/.bitcoin/bitcoin.conf

# Test with curl
curl -u "$BTC_RPC_USER:$BTC_RPC_PASSWORD" \
  -X POST http://localhost:8332 \
  -d '{"jsonrpc":"1.0","method":"getblockcount","params":[]}'
```

#### `Ethereum nonce too low`

```
WARN custody::broadcaster::ethereum: Transaction rejected: nonce too low
```

The service has a nonce mismatch with the chain. This happens after restart if the nonce was
incremented on-chain but the service cached an old value.

```bash
# The nonce manager re-reads from chain on restart — restart the service
sudo systemctl restart custody

# Verify by checking pending transaction status
curl -s -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  "http://localhost:8443/v1/withdrawals?status=broadcast" | jq .
```

#### `Tron energy balance too low`

```
WARN custody::scanner::tron: Energy balance low: 12345 (minimum: 65000 for TRC-20)
```

TRC-20 transfers require Energy. The hot wallet address is out of Energy.

```bash
# Check energy balance
curl -s http://localhost:8443/health/chains | jq '.tron.energy_balance'

# Freeze TRX to get Energy (done via Tron wallet or custody admin)
curl -X POST -H "X-Custody-Key: $CUSTODY_ADMIN_KEY" \
  http://localhost:8443/v1/admin/tron/freeze-for-energy \
  -d '{"amount_trx": "1000", "address": "<hot_wallet_address>"}'
```

#### `Solana blockhash expired`

```
WARN custody::broadcaster::solana: Transaction expired: blockhash expired, retrying with fresh blockhash
```

Solana transactions expire after ~90 seconds. This is handled automatically — the broadcaster
fetches a fresh blockhash and retries. If it persists, the Solana RPC may be slow.

```bash
# Check Solana RPC latency
curl -s http://localhost:8443/metrics | grep 'custody_chain_rpc_latency.*solana'

# If high latency, switch to a faster RPC endpoint
# Update SOLANA_RPC_URL in environment and restart
```

### Log Analysis

```bash
# Follow live logs (systemd)
sudo journalctl -u custody -f

# Search for errors in the last hour
sudo journalctl -u custody --since "1 hour ago" | grep -E "ERROR|WARN"

# Search for specific withdrawal
sudo journalctl -u custody | grep "withdrawal_id=wd-001"

# Find circuit breaker events
sudo journalctl -u custody | grep "circuit_breaker"

# Find signing errors
sudo journalctl -u custody | grep -E "signing.*error|SigningError"

# Count errors by type
sudo journalctl -u custody --since today | grep "ERROR" | sort | uniq -c | sort -rn | head -20
```

Docker Compose logs:
```bash
docker compose logs custody --since 1h | grep -E "ERROR|WARN"
docker compose logs --follow custody
```

### Health Check Failures

```bash
# Full health check
curl -s http://localhost:8443/health/deep | jq .

# Per-chain status
curl -s http://localhost:8443/health/chains | jq .

# What each field means:
# "database": "ok"      — PostgreSQL connection works
# "chains.ethereum.rpc": "ok"   — RPC responds to eth_blockNumber
# "chains.ethereum.lag": 3      — scanner is 3 blocks behind (normal)
# "chains.ethereum.lag": 500    — scanner is 500 blocks behind (problem)
# "signer": "ok"        — signing service responds (when Mode 2)
# "circuit_breaker": "open"     — withdrawals are allowed
# "circuit_breaker": "tripped"  — withdrawals are blocked
```

### RPC Node Connectivity

Test each chain's RPC directly from the custody host:

```bash
# Ethereum
curl -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
  $ETH_RPC_URL | jq .result

# Bitcoin
curl -u "$BTC_RPC_USER:$BTC_RPC_PASSWORD" \
  -X POST http://localhost:8332 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"1.0","id":"test","method":"getblockcount","params":[]}' | jq .result

# Tron
curl "https://api.trongrid.io/wallet/getnowblock" | jq '.block_header.raw_data.number'

# Solana
curl -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"getSlot"}' \
  $SOLANA_RPC_URL | jq .result
```

If the chain RPC is reachable from the host but not from the Docker container:
```bash
# Check if Docker can reach external network
docker compose exec custody curl -sf https://rpc.sepolia.org

# Check Docker network config
docker network inspect mg-custody_frontend
```

### Useful Diagnostic Commands

```bash
# Check circuit breaker state from DB directly (bypass API)
psql "$DATABASE_URL" -c "
  SELECT key, value, updated_at
  FROM circuit_breaker_state
  ORDER BY key;"

# Check recent deposits
psql "$DATABASE_URL" -c "
  SELECT chain, asset, amount, status, created_at
  FROM deposits
  ORDER BY created_at DESC
  LIMIT 20;"

# Check stuck withdrawals (in flight > 30 minutes)
psql "$DATABASE_URL" -c "
  SELECT id, chain, asset, amount, status, updated_at
  FROM withdrawals
  WHERE status IN ('signed', 'broadcast')
    AND updated_at < NOW() - INTERVAL '30 minutes'
  ORDER BY updated_at;"

# Check double-entry balance (should always be zero)
psql "$DATABASE_URL" -c "
  SELECT SUM(CASE WHEN entry_type = 'credit' THEN amount ELSE -amount END) AS net_balance
  FROM ledger_entries;"
# Expected: 0.000000

# Check audit log hash chain integrity
psql "$DATABASE_URL" -c "
  SELECT id, action, created_at, hash
  FROM audit_log
  ORDER BY id DESC
  LIMIT 10;"
```

---

## Appendix: Background Tasks Reference

After startup, the following background tasks run continuously:

| Task | Interval | What It Does |
|------|----------|-------------|
| ETH Scanner | 12s | Polls new blocks for deposits to known addresses |
| BTC Scanner | 60s | Polls new blocks for UTXO deposits |
| TRX Scanner | 3s | Polls new Tron blocks for TRC-20/TRX deposits |
| SOL Scanner | 2s | Polls new Solana slots for SPL/SOL deposits |
| Confirmation Tracker | 10s | Advances deposit confirmation count; fires webhooks at threshold |
| Withdrawal Pipeline | 5s | Runs state machine: Received → Validated → Signed → Broadcast → Confirmed |
| Webhook Retry | 30s | Retries failed webhook deliveries with exponential backoff |
| Reconciliation | 10min | Compares on-chain balances to internal ledger; alerts on discrepancy |
| Gas Station | 2min | Auto-funds addresses below gas minimum (ETH/TRX/SOL) |
| Sweep Scheduler | 60s | Moves funds between hot/warm/cold tiers per configured percentages |

---

## Appendix: Related Documentation

| Document | Purpose |
|----------|---------|
| [DEPLOYMENT_MODES.md](DEPLOYMENT_MODES.md) | Choose between Dev / Minimal / HSM / MPC mode |
| [QUICKSTART.md](QUICKSTART.md) | 30-minute first deployment guide |
| [ONBOARDING.md](ONBOARDING.md) | Day 1 walkthrough for startups and trading desks |
| [SECURITY.md](../SECURITY.md) | Security architecture and current property status |
| [THREAT_MODEL.md](../THREAT_MODEL.md) | STRIDE analysis, what is and is not protected |
| [REFERENCES.md](../REFERENCES.md) | Source citations for all cryptographic and financial logic |
