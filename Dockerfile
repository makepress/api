FROM rust:1.57 AS chef
RUN apt-get update \
    && apt-get install -y lld \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef

WORKDIR /usr/src/app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /usr/src/app/recipe.json recipe.json
RUN CARGO_INCREMENTAL=0 \
    CARGO_PROFILE_RELEASE_LTO=thin \
    RUSTFLAGS="-C link-arg=-fuse-ld=lld -C link-arg=-Wl,--compress-debug-sections=zlib -C force-frame-pointers=yes" \
    cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN CARGO_INCREMENTAL=0 \
    CARGO_PROFILE_RELEASE_LTO=thin \
    RUSTFLAGS="-C link-arg=-fuse-ld=lld -C link-arg=-Wl,--compress-debug-sections=zlib -C force-frame-pointers=yes" \
    cargo install --path .

FROM gcr.io/distroless/cc-debian11
COPY --from=builder /usr/src/app/target/release/api /usr/local/bin/api
ENTRYPOINT [ "/usr/local/bin/api" ]