# WACP Runtime — Multi-stage Docker build
#
# Build context: umbrella root (wacp-platform/). Invoked from the umbrella
# Dockerfile path `wacp/Dockerfile` so `COPY` paths resolve against the
# unified Cargo workspace rather than the wacp subtree alone.

# ── Build stage ──────────────────────────────────────────────────────
FROM rust:1.85-bookworm AS build

RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo/ .cargo/
COPY wacp/ wacp/
COPY wacp-console/ wacp-console/

RUN cargo build --release -p wacp-runtime \
    && strip target/release/wacp-runtime

# ── Runtime stage ────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --gid 1000 wacp \
    && useradd --uid 1000 --gid wacp --no-create-home --shell /sbin/nologin wacp

RUN mkdir -p /var/lib/wacp && chown wacp:wacp /var/lib/wacp && chmod 755 /var/lib/wacp
RUN mkdir -p /etc/wacp/tls && chmod 755 /etc/wacp && chmod 750 /etc/wacp/tls

COPY --from=build /build/target/release/wacp-runtime /usr/local/bin/wacp-runtime
RUN chmod 755 /usr/local/bin/wacp-runtime

# Environment defaults — canonical port map (IMPLEMENTATION.md §4.1).
ENV WACP_STORAGE__DATA_DIR=/var/lib/wacp
ENV WACP_SERVER__AGENT_LISTEN=0.0.0.0:9090
ENV WACP_SERVER__HIGHWAY_LISTEN=0.0.0.0:9091
ENV WACP_SERVER__COORDINATOR_LISTEN=0.0.0.0:9092
ENV WACP_SERVER__REST_LISTEN=0.0.0.0:9093
ENV WACP_OBSERVABILITY__HEALTH__LISTEN=0.0.0.0:9094
ENV WACP_OBSERVABILITY__METRICS__LISTEN=0.0.0.0:9095

EXPOSE 9090 9091 9092 9093 9094

VOLUME ["/var/lib/wacp", "/etc/wacp"]

HEALTHCHECK --interval=10s --timeout=3s --start-period=30s --retries=3 \
    CMD ["wacp-runtime", "validate"] || exit 1

USER wacp

ENTRYPOINT ["wacp-runtime"]
CMD ["serve"]
