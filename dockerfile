ARG RUST_VERSION=1.85.0
FROM rust:${RUST_VERSION}-slim-bullseye AS build
ARG APP_NAME=bm25-interface
WORKDIR /bm25-interface

RUN apt-get update && \
    apt-get install --no-install-recommends --assume-yes \
        protobuf-compiler

RUN --mount=type=bind,source=src,target=src \
    --mount=type=bind,source=proto,target=proto \
    --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
    --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
    --mount=type=bind,source=build.rs,target=build.rs \
    --mount=type=cache,target=/app/target/ \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    <<EOF
set -e 
cargo build --locked --release && 
cp -r ./target/release/$APP_NAME /bin/server
EOF

FROM debian:bullseye-slim AS final

ARG UID=10001
RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    appuser
USER appuser

COPY --from=build /bin/server /bin/
CMD ["/bin/server"]
