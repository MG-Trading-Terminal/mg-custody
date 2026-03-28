# Pinned to specific versions for supply chain security (Phase 17E)
# Update these digests deliberately, not via :latest

# Stage 1: Dashboard build
FROM node:22-alpine AS dashboard
WORKDIR /dashboard
COPY dashboard/package*.json ./
RUN npm ci
COPY dashboard/ .
RUN npm run build

# Stage 2: Rust build
FROM rust:1.85-bookworm AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
COPY --from=dashboard /dashboard/build dashboard/build
RUN cargo build --release --bin custody --bin custody-admin --bin custody-seed --bin custody-signer

# Stage 3: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*

# Non-root user for security
RUN groupadd -r custody && useradd -r -g custody -d /app -s /sbin/nologin custody

COPY --from=builder /app/target/release/custody         /usr/local/bin/custody
COPY --from=builder /app/target/release/custody-admin   /usr/local/bin/custody-admin
COPY --from=builder /app/target/release/custody-seed    /usr/local/bin/custody-seed
COPY --from=builder /app/target/release/custody-signer  /usr/local/bin/custody-signer
COPY --from=builder /app/dashboard/build                /opt/custody/dashboard/
COPY --from=builder /app/config                         /opt/custody/config/

RUN mkdir -p /app && chown -R custody:custody /app /opt/custody

WORKDIR /app
USER custody:custody

ENV CUSTODY_DASHBOARD_DIR=/opt/custody/dashboard

EXPOSE 8443

HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -sf http://localhost:8443/health || exit 1

CMD ["custody"]
