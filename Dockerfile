FROM rust:1.77 AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
RUN cargo build --release

FROM ubuntu:24.04

RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

ENV RUST_BACKTRACE=1

COPY --from=builder /app/target/release/pqkd-relay /usr/local/bin/pqkd-relay