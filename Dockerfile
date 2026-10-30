FROM oven/bun:1.3.14 AS web-builder

WORKDIR /web

COPY server/web/package.json server/web/bun.lock server/web/.npmrc ./
RUN bun install --frozen-lockfile

COPY server/web ./
RUN bun run generate

FROM rust:1.98-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY client ./client
COPY server ./server

RUN cargo build --release --locked -p anchor-server

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/anchor-server /usr/local/bin/anchor-server
COPY --from=web-builder /web/.output/public /app/web

EXPOSE 8080

ENV ANCHOR_WEB_DIR=/app/web

ENTRYPOINT ["/usr/local/bin/anchor-server"]
