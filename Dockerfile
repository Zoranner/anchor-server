FROM rust:1.98-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY client ./client
COPY admin ./admin
COPY server ./server

RUN cargo build --release --locked -p anchor-server

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/anchor-server /usr/local/bin/anchor-server

EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/anchor-server"]
