# Task 17.4: Docker Image + Systemd Unit

## Scope

Create the Dockerfile (multi-stage build) and systemd unit file per deployment.md §9–10.

## Files

### `Dockerfile`

Multi-stage: `rust:1.85-bookworm` build → `debian:bookworm-slim` runtime.

- Build: `--release`, strip binary
- Runtime: non-root `wacp` user (UID 1000), ca-certificates + libssl3
- Entrypoint: `["wacp-runtime"]`, CMD: `["serve"]`
- Volumes: `/var/lib/wacp` (data), `/etc/wacp` (config)
- HEALTHCHECK: HTTP GET `:9093/healthz`
- Exposes: 9090 (agent), 9091 (highway), 9092 (metrics), 9093 (health)

### `deploy/wacp-runtime.service`

Systemd unit with:

- `Type=exec`, `User=wacp`, `Restart=on-failure`
- 17 security hardening directives (ProtectSystem=strict, NoNewPrivileges, etc.)
- `ReadWritePaths=/var/lib/wacp`, `ReadOnlyPaths=/etc/wacp`
- `LimitNOFILE=65536`, `TimeoutStopSec=60`
- Journal logging via `StandardOutput=journal`

## Acceptance Criteria

- Dockerfile follows deployment.md §9 exactly.
- Systemd unit follows deployment.md §10 exactly.
- Both are functional configuration files (not templates with placeholders).
