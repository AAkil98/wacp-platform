---
id: wcon-adr-009
type: impl
status: accepted
created: 2026-04-15T04:00:00
authors: [AAkil98, Claude Opus 4.6]
tags: [adr, distribution, console, oci, supersedes-adr-004]
depends_on: [wcon-merge-plan, wcon-architecture]
---

# ADR-009 — OCI-only Distribution for `wacp-console`

**Supersedes:** ADR-004 (single binary via `rust-embed` + `cargo-dist`).

## Table of Contents

- 1. Context
- 2. Decision
- 3. Consequences
- 4. Alternatives Considered
- 5. Open Items

---

## 1. Context

ADR-004 committed the console to a single-binary distribution model: `rust-embed` embeds the built frontend (`frontend/dist/`) into the `wacp-console` binary; `cargo-dist` produces platform-native archives (macOS / Linux / Windows × amd64 / arm64) on each `wacp-console-v*` tag.

Three signals collected during the merge reframe the pick:

1. **Console is a server, not a desktop tool.** It exposes an HTTP listener (default `[::1]:8080`), persists to SQLite (default XDG data dir or `--database`), and connects out to the runtime over gRPC + REST. The natural deploy target is a container image sitting next to the runtime — not a user-local binary on macOS/Windows.
2. **Infrastructure consumers already speak OCI.** `docker-compose.yml` at umbrella root brings up the runtime + console as a pair (D12). Kubernetes, Nomad, Fly.io, Docker Swarm, Caddy sidecars, systemd-nspawn — all consume OCI images directly. Native archives need an extra repackaging step for every deployment.
3. **`cargo-dist` sunk cost is low.** It was declared as a dep but never wired. No scripts depend on it; no release has shipped via it. Removing it costs nothing in regressions.

## 2. Decision

**Ship `wacp-console` as an OCI image only.** The `wacp-console-v*` release tag triggers a single CI job (`release-console.yml`) that:

- Builds a multi-platform image (`linux/amd64`, `linux/arm64`) from `wacp-console/Dockerfile`.
- Pushes to `ghcr.io/madahub-dev/wacp-console:<tag>` and `ghcr.io/madahub-dev/wacp-console:latest`.
- Publishes a GitHub Release containing only the image digest + changelog (no binary artifacts).

**Retain `rust-embed` inside the cargo build stage** of `wacp-console/Dockerfile`. Stage 1 (node) builds `frontend/dist/`; stage 2 (rust) compiles with `rust-embed` embedding those assets into the release binary; stage 3 (distroless/slim) runs the embedded binary as non-root. The resulting image is one layer thick with a single `CMD ["wacp-console", "serve"]`.

**Defer `cargo-dist`.** Remove from the `Cargo.toml` metadata and release tooling. If a native-archive use case surfaces later (e.g., offline deploy, macOS-only eval flow), revisit with a new ADR — not a silent wire-up.

## 3. Consequences

### Positive

- **One artifact, one publish path.** Image push is the only thing `release-console.yml` does; failure modes collapse to "docker build failed" or "ghcr auth failed," both loud.
- **Deployment parity with the runtime.** `wacp/Dockerfile` already ships runtime as OCI; console now matches. `docker-compose.yml` can pin both to the same tag convention (`:<version>` or `:latest`).
- **No macOS/Windows CI infrastructure.** `cargo-dist`'s strength (cross-platform native archives) required GitHub-hosted macOS + Windows runners on every release tag. OCI-only keeps CI on ubuntu-latest.
- **Compiled frontend travels with the binary.** `rust-embed` retention means the image doesn't need a separate "static assets" layer or volume mount; the binary serves its own SPA.

### Negative

- **Users who wanted a desktop-installable console don't get one.** Mitigation: the image is 30–80 MB distroless; running `docker run -p 8080:8080 …` is one command. For users without Docker, `cargo install --path wacp-console/crates/console` still works from a cloned checkout.
- **Native-archive release workflow won't exist.** Mitigation: deliberate — revisit only if a concrete use case appears.

### Neutral

- **Runtime distribution is unaffected.** `release-runtime.yml` keeps its 4-target matrix (`linux-gnu`, `linux-musl`, `darwin-arm64`, `darwin-x86_64`) for the `wacp-runtime-v*` tags (§10.4 of the merge plan). Runtime is a server too, but it already shipped native archives and has consumers that want them; no regression.

## 4. Alternatives Considered

| Option | Why rejected |
|--------|--------------|
| Keep ADR-004 (native archives + OCI) | Doubles release surface: two workflow jobs, two artifact types, two cleanup runbooks. No consumer has asked for the native archives. |
| OCI + single-binary `cargo install` crates.io release | `wacp-console` depends on `frontend/dist/` at `rust-embed`-compile-time, which is not on crates.io. Publishing would require vendoring the built SPA into the crate — brittle and bloats the crates.io artifact. |
| Native archives only (no OCI) | Loses the compose-file scenario and Kubernetes deployability. Console is a server; not shipping an image is user-hostile. |
| Containerfile + CLI installer (e.g., `install.sh`) | Adds a second install path with its own bugs. The `docker run` one-liner is already the simplest install flow. |

## 5. Open Items

- **Image tagging convention.** `release-console.yml` tags `:<semver>` and `:latest`. If SemVer pre-release tags (`v0.1.0-rc.1`) need to NOT update `:latest`, add a guard. Deferred to first real release.
- **Multi-arch SBOM.** `docker/build-push-action` with `provenance: true` emits attestations; confirm at M7 whether to ingest them into a supply-chain tracker. Non-blocking for v0.
- **Signature verification.** `cosign` signing on publish would let consumers verify image provenance. Nice-to-have; revisit when the first external consumer asks for it.

## References

| ID | Title | Relationship |
|----|-------|--------------|
| ADR-004 | Single binary distribution (`rust-embed` + `cargo-dist`) | superseded by this ADR |
| wcon-merge-plan | Monorepo Merge Plan | informs (D11, §5.6, §5.7, §10.4 all reference this decision) |
| wcon-architecture | Console System Architecture | constrains (runtime-connection model implies a server deploy target) |

*WACP Platform — authored by AKIL Abderrahim and Claude Opus 4.6*
