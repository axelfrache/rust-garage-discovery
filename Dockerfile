FROM rust:latest as builder

WORKDIR /app

RUN cargo new --lib shared
RUN cargo new --bin sender
RUN cargo new --bin receiver

COPY Cargo.toml ./
COPY shared/Cargo.toml shared/
COPY sender/Cargo.toml sender/
COPY receiver/Cargo.toml receiver/

RUN cargo build --release

COPY shared/src shared/src
COPY sender/src sender/src
COPY receiver/src receiver/src

RUN touch shared/src/lib.rs sender/src/main.rs receiver/src/main.rs

RUN cargo build --release

FROM debian:sid-slim

RUN apt-get update && apt-get install -y ca-certificates libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/sender /usr/local/bin/sender
COPY --from=builder /app/target/release/receiver /usr/local/bin/receiver

EXPOSE 3000

CMD ["/usr/local/bin/sender"]
