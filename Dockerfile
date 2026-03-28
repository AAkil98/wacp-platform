# WACP Runtime — Multi-stage Docker build
# deployment.md §9

# ── Build stage ──────────────────────────────────────────────────────
FROM rust:1.85-bookworm AS build

# Install protobuf compiler for tonic-build codegen.
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy manifests first for layer caching.
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY proto/ proto/

# Build release binary with LTO and stripped symbols.
RUN cargo build --release --bin wacp-runtime \
    && strip target/release/wacp-runtime

# ── Runtime stage ────────────────────────────────────────────────────
FROM debian:bookworm-slim

# Runtime dependencies.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Non-root user (deployment.md §9.3).
RUN groupadd --gid 1000 wacp \
    && useradd --uid 1000 --gid wacp --no-create-home --shell /sbin/nologin wacp

# Data directory — owned by wacp.
RUN mkdir -p /var/lib/wacp && chown wacp:wacp /var/lib/wacp && chmod 755 /var/lib/wacp

# Config directory — root-owned, read-only.
RUN mkdir -p /etc/wacp/tls && chmod 755 /etc/wacp && chmod 750 /etc/wacp/tls

# Copy binary from build stage.
COPY --from=build /build/target/release/wacp-runtime /usr/local/bin/wacp-runtime
RUN chmod 755 /usr/local/bin/wacp-runtime

# Environment defaults.
ENV WACP_STORAGE__DATA_DIR=/var/lib/wacp
ENV WACP_SERVER__AGENT_LISTEN=0.0.0.0:9090
ENV WACP_SERVER__HIGHWAY_LISTEN=0.0.0.0:9091
ENV WACP_OBSERVABILITY__METRICS__LISTEN=0.0.0.0:9092
ENV WACP_OBSERVABILITY__HEALTH__LISTEN=0.0.0.0:9093

# Expose ports: agent, highway, metrics, health.
EXPOSE 9090 9091 9092 9093

# Volumes: persistent data and config.
VOLUME ["/var/lib/wacp", "/etc/wacp"]

# Health check (deployment.md §9.5).
HEALTHCHECK --interval=10s --timeout=3s --start-period=30s --retries=3 \
    CMD ["wacp-runtime", "validate"] || exit 1

USER wacp

ENTRYPOINT ["wacp-runtime"]
CMD ["serve"]
