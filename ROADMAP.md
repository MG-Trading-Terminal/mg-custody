# Roadmap

## Phase 0 — Foundation (2-3 weeks) ✅ DONE
- [x] Cargo workspace with 14 crates
- [x] Core types (Chain, Asset, Deposit, Withdrawal, CustodyEvent)
- [x] Error types (CustodyError)
- [x] DB layer stubs (sqlx — addresses, deposits, withdrawals)
- [x] REST API skeleton (axum, HMAC auth)
- [x] Configuration management (figment, config/default.toml)
- [x] Binary entry point (custody-server)
- [x] CI-ready (cargo test, cargo check pass)
- [x] Agent setup (CLAUDE.md, 6 agents, 6 commands)

## Phase 1 — Key Management (3-4 weeks) 🔜 NEXT
- [ ] HD wallet key generation (BIP32/BIP44, CSPRNG entropy)
- [ ] Encrypted key storage at rest (AES-256-GCM)
- [ ] Ethereum address derivation (secp256k1 → Keccak-256 → EIP-55)
- [ ] Bitcoin address derivation (P2WPKH, Bech32)
- [ ] Tron address derivation (secp256k1 → Base58Check)
- [ ] Solana address derivation (Ed25519, SLIP-0010)
- [ ] Address allocator (idempotent per user+asset+chain)
- [ ] Key backup ceremony tooling (Shamir 3-of-5)
- [ ] HSM integration stub (SoftHSM2 for dev)
- [ ] Key management audit logging

## Phase 2 — Ethereum MVP (4-5 weeks)
- [ ] Ethereum JSON-RPC client (alloy)
- [ ] Block scanner — ETH native transfers
- [ ] ERC-20 log scanner (Transfer events)
- [ ] Confirmation tracking (12 blocks)
- [ ] Deposit credit webhook to exchange
- [ ] Gas price oracle (EIP-1559)
- [ ] ETH withdrawal signing
- [ ] ERC-20 withdrawal signing
- [ ] Nonce manager (single-threaded, persistent)
- [ ] Broadcast + status tracking + fee bumping
- [ ] Sepolia testnet integration test
- [ ] Wire SelfHostedCustodyProvider in exchange

## Phase 3 — Multi-Chain (6-8 weeks)
- [ ] Bitcoin: Core RPC, deposit scanner, UTXO management, withdrawal signing, fee estimation
- [ ] Tron: HTTP client, deposit scanner, Energy management, TRC-20 signing
- [ ] Solana: RPC client, deposit detection (ATA), SPL withdrawal, blockhash retry
- [ ] ChainAdapter trait unification

## Phase 4 — Production Hardening (4-5 weeks)
- [ ] Hot/warm/cold wallet tiering with sweeps
- [ ] Outflow circuit breaker (persistent, alerting)
- [ ] Per-user velocity limits
- [ ] Address screening (OFAC)
- [ ] Large withdrawal approval workflow
- [ ] Prometheus metrics
- [ ] Health endpoints per chain
- [ ] Stuck withdrawal recovery CLI
- [ ] Historical deposit rescan tool
- [ ] Load testing (1000 deposits + 100 withdrawals concurrent)

## Phase 5 — Compliance & Audit (3-4 weeks)
- [ ] Blockchain analytics integration (Chainalysis KYT)
- [ ] Travel Rule (Notabene or TRISA)
- [ ] On-chain Proof of Reserves attestation
- [ ] Tamper-evident audit log (hash chain)
- [ ] SOC 2 control mapping
- [ ] External penetration test
