FROM rust:1.97-trixie as planner

RUN cargo install cargo-chef

WORKDIR /srv
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM rust:1.97-trixie as builder

RUN cargo install cargo-chef

WORKDIR /srv
COPY --from=planner /srv/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN cargo build --release

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y libssl3t64 iptables ca-certificates
COPY --from=builder /srv/target/release/aerogramme /bin/aerogramme
ENTRYPOINT ["/bin/aerogramme"]
