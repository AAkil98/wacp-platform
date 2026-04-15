# WACP Console — Multi-stage Docker build (ADR-009: OCI-only)
#
# Build context: umbrella root (wacp-platform/). Three stages:
#   1. frontend-builder (node:22-alpine) — pnpm build → frontend/dist/
#   2. cargo-builder   (rust:1.85-bookworm) — cargo build --release with
#      rust-embed picking up the dist/ from stage 1
#   3. runtime         (debian:bookworm-slim) — binary + non-root user

# ── Frontend stage ───────────────────────────────────────────────────
FROM node:22-alpine AS frontend-builder

WORKDIR /build/wacp-console/frontend

COPY wacp-console/frontend/package.json wacp-console/frontend/pnpm-lock.yaml ./
RUN corepack enable && pnpm install --frozen-lockfile

COPY wacp-console/frontend/ ./
# openapi.yaml is referenced by `pnpm gen:api` via ../openapi.yaml; include
# it for completeness even though `pnpm build` alone doesn't invoke gen:api.
COPY wacp-console/openapi.yaml ../openapi.yaml

RUN pnpm build
# Emits /build/wacp-console/frontend/dist/

# ── Cargo stage ──────────────────────────────────────────────────────
FROM rust:1.85-bookworm AS cargo-builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo/ .cargo/
COPY wacp/ wacp/
COPY wacp-console/ wacp-console/

# Pull the built SPA into the exact path rust-embed expects
# ($CARGO_MANIFEST_DIR/../../frontend/dist/ from crates/console/).
COPY --from=frontend-builder /build/wacp-console/frontend/dist /build/wacp-console/frontend/dist

RUN cargo build --release -p wacp-console \
    && strip target/release/wacp-console

# ── Runtime stage ────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --gid 1000 console \
    && useradd --uid 1000 --gid console --no-create-home --shell /sbin/nologin console

RUN mkdir -p /var/lib/wacp-console \
    && chown console:console /var/lib/wacp-console \
    && chmod 755 /var/lib/wacp-console

COPY --from=cargo-builder /build/target/release/wacp-console /usr/local/bin/wacp-console
RUN chmod 755 /usr/local/bin/wacp-console

# Container defaults: bind all interfaces (loopback is host-unreachable).
ENV WACP_CONSOLE_LISTEN=[::]:8080
ENV WACP_RUNTIME_AGENT_ADDRESS=wacp-runtime:9090
ENV WACP_RUNTIME_HIGHWAY_ADDRESS=wacp-runtime:9091
ENV WACP_RUNTIME_COORDINATOR_ADDRESS=wacp-runtime:9092
ENV WACP_RUNTIME_REST_ADDRESS=http://wacp-runtime:9093

EXPOSE 8080

VOLUME ["/var/lib/wacp-console"]

USER console

ENTRYPOINT ["wacp-console"]
CMD ["serve", "--database", "/var/lib/wacp-console/console.db"]
