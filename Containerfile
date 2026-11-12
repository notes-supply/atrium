# A static musl binary on distroless: no shell or package manager, and non-root (UID 65532).
# The Wasm bundles come from `mise run build:wasm` on the host, which `mise run image` runs
# first: it needs the pinned nightly and its wasm32 target, and a .wasm is the same file
# wherever it is built.
FROM docker.io/library/rust:1.99.1-alpine3.24@sha256:44819b27be4c97b0e4d7203bd2e02c4820f97700211b6a37fb4c246a2de59383 AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY web web
COPY target/web target/web
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target/release \
    cargo build --release --locked -p atrium \
    && cp target/release/atrium /atrium

FROM gcr.io/distroless/static-debian13:nonroot@sha256:25ac8f0c3b2334a9eb05452266bd4697864a8c8ef0ef71e5e6480d7f92868329
COPY --from=build /atrium /usr/local/bin/atrium
USER 65532:65532
# A container's port is published from its own network, so the image listens on every
# address of it. The binary alone listens on loopback.
ENV ATRIUM_ADDR=0.0.0.0:8080
LABEL org.opencontainers.image.description="Atrium, a shared homepage and dashboard for a team"
ENTRYPOINT ["/usr/local/bin/atrium"]
