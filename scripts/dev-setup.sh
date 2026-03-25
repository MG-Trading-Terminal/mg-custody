#!/bin/bash
# Dev environment setup script
set -euo pipefail

echo "=== MG Custody Dev Setup ==="

# Check prerequisites
command -v docker >/dev/null 2>&1 || { echo "Docker required. Install from https://docker.com"; exit 1; }
command -v cargo >/dev/null 2>&1 || { echo "Rust/Cargo required. Install from https://rustup.rs"; exit 1; }

# Copy env if not exists
if [ ! -f .env ]; then
    cp .env.example .env
    echo "Created .env from .env.example — edit with your values"
fi

# Start PostgreSQL
echo "Starting PostgreSQL..."
docker compose up -d postgres
echo "Waiting for PostgreSQL to be ready..."
sleep 3

# Run migrations
echo "Running migrations..."
export DATABASE_URL="postgres://custody:custody_dev_password@localhost:5432/custody"
psql "$DATABASE_URL" -f crates/db/migrations/001_initial_schema.sql 2>/dev/null || \
    docker compose exec -T postgres psql -U custody -d custody -f /docker-entrypoint-initdb.d/001_initial_schema.sql

echo "Checking tables..."
psql "$DATABASE_URL" -c "\dt" 2>/dev/null || \
    docker compose exec -T postgres psql -U custody -d custody -c "\dt"

# Build
echo "Building custody service..."
cargo build --release --bin custody

echo ""
echo "=== Setup Complete ==="
echo "Database: $DATABASE_URL"
echo "Run service: cargo run --release --bin custody"
echo "Run tests: cargo test"
echo "Docker: docker compose up"
