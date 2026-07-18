# ---- build stage: compile the Rust binaries ----
FROM rust:1-slim AS build
WORKDIR /app
COPY . .
RUN cargo build --release

# ---- runtime stage: a tiny image with just the binaries ----
FROM debian:stable-slim
COPY --from=build /app/target/release/white-lotus /usr/local/bin/white-lotus
COPY --from=build /app/target/release/tracker /usr/local/bin/tracker
