# syntax=docker/dockerfile:1.7

FROM rust:slim-bookworm AS build

ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
ENV RUSTFLAGS=-Dwarnings

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/lince/Cargo.toml crates/lince/Cargo.toml
COPY crates/application/Cargo.toml crates/application/Cargo.toml
COPY crates/domain/Cargo.toml crates/domain/Cargo.toml
COPY crates/injection/Cargo.toml crates/injection/Cargo.toml
COPY crates/persistence/Cargo.toml crates/persistence/Cargo.toml
COPY crates/gui/Cargo.toml crates/gui/Cargo.toml
COPY crates/utils/Cargo.toml crates/utils/Cargo.toml
COPY crates/tui/Cargo.toml crates/tui/Cargo.toml
COPY crates/web/Cargo.toml crates/web/Cargo.toml
COPY xtask/Cargo.toml xtask/Cargo.toml
COPY crates/lince/build.rs crates/lince/build.rs
COPY crates/lince/src/main.rs crates/lince/src/main.rs
COPY crates/application/src/lib.rs crates/application/src/lib.rs
COPY crates/domain/src/lib.rs crates/domain/src/lib.rs
COPY crates/injection/src/lib.rs crates/injection/src/lib.rs
COPY crates/persistence/src/lib.rs crates/persistence/src/lib.rs
COPY crates/gui/src/lib.rs crates/gui/src/lib.rs
COPY crates/utils/src/lib.rs crates/utils/src/lib.rs
COPY crates/tui/src/lib.rs crates/tui/src/lib.rs
COPY crates/web/src/lib.rs crates/web/src/lib.rs
COPY xtask/src/main.rs xtask/src/main.rs

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo fetch --locked

COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked -p lince \
    && strip /app/target/release/lince

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 lince \
    && useradd --system --uid 10001 --gid lince --create-home --home-dir /var/lib/lince lince \
    && mkdir -p /var/lib/lince/.config \
    && chown -R lince:lince /var/lib/lince

COPY --from=build /app/target/release/lince /usr/local/bin/lince

ENV HOME=/var/lib/lince
ENV XDG_CONFIG_HOME=/var/lib/lince/.config

WORKDIR /var/lib/lince

VOLUME ["/var/lib/lince"]

EXPOSE 6174

USER lince:lince

ENTRYPOINT ["/usr/local/bin/lince"]
CMD ["--listen-addr", "0.0.0.0:6174"]
