# Changelog

All notable changes to this project will be documented in this file.
Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added
- Initial 14-crate workspace structure (Phase 0 foundation)
- `custody-common`: Core types — Chain, Asset, Deposit, Withdrawal, CustodyEvent, WalletTier
- `custody-common`: Error types — CustodyError with chain, signing, compliance, and infra variants
- `custody-db`: PostgreSQL persistence stubs (sqlx) — addresses, deposits, withdrawals repos
- `custody-gateway`: axum REST API skeleton — POST /addresses, POST /withdrawals, GET /withdrawals/:id, GET /wallets/:asset, GET /health
- `custody-gateway`: HMAC-SHA256 request authentication with timestamp validation
- `custody-scanner`: ChainScanner trait for per-chain block polling
- `custody-signing`: SigningBackend trait for swappable key management (software → HSM → MPC)
- `custody-server`: Binary entry point with tracing + JSON structured logging
- `config/default.toml`: Full configuration with chain endpoints, token contracts, limits, wallet policy
- 7 tests passing (types serialization, HMAC auth)
- CLAUDE.md with project rules, security requirements, chain-specific rules
- 6 specialized agents: architect, developer, security, ops, blockchain, pm
- 6 slash commands matching agents

### References
- Architecture: ~/Projects/exchange/docs/roadmaps/self-hosted-custody-research.md
- Exchange CustodyProvider trait: ~/Projects/exchange/crates/custody/src/provider.rs
- BIP-44 coin types: https://github.com/satoshilabs/slips/blob/master/slip-0044.md
- EIP-1559: https://eips.ethereum.org/EIPS/eip-1559
- SLIP-0010 (Solana): https://github.com/satoshilabs/slips/blob/master/slip-0010.md
