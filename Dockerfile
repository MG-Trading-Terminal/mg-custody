FROM rust:1.85-alpine AS builder

RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconfig

WORKDIR /app
COPY . .

RUN cargo build --release --bin custody

FROM alpine:3.20

RUN apk add --no-cache ca-certificates

COPY --from=builder /app/target/release/custody /usr/local/bin/custody
COPY --from=builder /app/config /app/config

WORKDIR /app
EXPOSE 8443

CMD ["custody"]
