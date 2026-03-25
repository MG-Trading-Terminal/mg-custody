FROM rust:latest AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

RUN cargo build --release --bin custody --bin custody-admin --bin custody-seed

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/custody /usr/local/bin/custody
COPY --from=builder /app/target/release/custody-admin /usr/local/bin/custody-admin
COPY --from=builder /app/target/release/custody-seed /usr/local/bin/custody-seed
COPY --from=builder /app/config /app/config

WORKDIR /app
EXPOSE 8443

CMD ["custody"]
