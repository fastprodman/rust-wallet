FROM rust:1.97-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY .sqlx ./.sqlx
COPY src ./src

ENV SQLX_OFFLINE=true

RUN cargo build --release --locked

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/rust-wallet /usr/local/bin/rust-wallet

USER 10001:10001

EXPOSE 3000

ENTRYPOINT ["rust-wallet"]
