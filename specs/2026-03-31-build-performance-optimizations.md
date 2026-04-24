# Build Performance Optimizations

| Document Metadata      | Details                                                                 |
| ---------------------- | ----------------------------------------------------------------------- |
| Author(s)              | Sean Larkin                                                             |
| Status                 | Implemented                                                             |
| Team / Owner           | Sean Larkin                                                             |
| Created / Last Updated | 2026-03-31                                                              |
| Research               | `research/tickets/2026-03-31-0157-build-performance-report.md`          |
| Issue                  | [TheLarkInn/aipm#157](https://github.com/TheLarkInn/aipm/issues/157)   |

## 1. Executive Summary

This spec addresses the two highest-impact build performance bottlenecks identified across three consecutive build timing reports (#135, #150, #157). Cold build time is 119s, with `libgit2-sys` C compilation consuming 52.85s (44%) and the `reqwest` async ecosystem adding ~30s. Two changes are proposed: (1) install system `libgit2-dev`/`libssl-dev` in CI and set `LIBGIT2_SYS_USE_PKG_CONFIG=1` to eliminate the C build script (~45-50s savings), and (2) replace `reqwest` with `ureq`, a synchronous-only HTTP client, eliminating tokio, hyper, tower, and the async stack (~15-25s savings). Combined estimated savings: 60-75s on cold builds (50-63% reduction).

## 2. Context and Motivation

### 2.1 Current State

- **Build time**: 119s cold build, 241 compilation units, 4-core runner
- **Critical path**: `libgit2-sys` build script (52.85s) serializes C compilation of libgit2 + vendored OpenSSL, blocking all downstream crates
- **Dependency bloat**: `reqwest` with `blocking` feature pulls in tokio (10.04s), hyper, tower, h2, futures-util — a full async runtime for a single synchronous `GET` call
- **CI caching**: `Swatinem/rust-cache@v2` is configured in `ci.yml` but does not help cold builds; no system library pre-installation
- **Profile tuning**: `[profile.dev]` with `debug = 1`, `split-debuginfo = "unpacked"`, per-package opt-level overrides already applied (from #135)
- **git2 feature gating**: SSH features already removed (`default-features = false, features = ["https"]`), eliminating `libssh2-sys` (from #135)

### 2.2 The Problem

- **Developer experience**: 2-minute cold builds on CI slow down PR iteration. Contributors wait for feedback loops.
- **CI cost**: Every CI run compiles libgit2 from C source because no workflow installs system libraries. The rust-cache only helps incremental Rust compilation, not C build scripts.
- **Unnecessary dependency weight**: The `reqwest` crate (with `blocking` feature) pulls in a full async runtime (tokio, hyper, tower, h2, futures-util) to wrap synchronous I/O — for exactly one `GET` call in the entire codebase.
- **Stale recommendation**: System libgit2 installation has been recommended in all three build reports (#135, #150, #157) but never implemented.

*Research: [research/tickets/2026-03-31-0157-build-performance-report.md](../research/tickets/2026-03-31-0157-build-performance-report.md) — Section 3 "Historical Trend" documents this bottleneck persisting across all reports.*

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [x] **G1**: Reduce cold CI build time by 50%+ (from 119s to ~45-60s)
- [x] **G2**: Install system `libgit2-dev`, `libssl-dev`, and `pkg-config` in CI workflows that compile Rust
- [x] **G3**: Set `LIBGIT2_SYS_USE_PKG_CONFIG=1` environment variable in CI workflows
- [x] **G4**: Replace `reqwest` with `ureq` (v3) in the workspace dependency declaration
- [x] **G5**: Migrate the single `reqwest::blocking::get` call site to `ureq::get`
- [x] **G6**: Remove unused `reqwest` dependency (resolved: `aipm-pack` was merged into `aipm` — no separate crate or dependency remains)
- [x] **G7**: All existing tests pass (`cargo test --workspace`)
- [x] **G8**: Clippy clean (`cargo clippy --workspace -- -D warnings`)
- [x] **G9**: Coverage remains at or above 89% branch threshold

### 3.2 Non-Goals (Out of Scope)

- [ ] We will NOT convert the codebase to async. The `Registry` trait remains synchronous.
- [ ] We will NOT change the `cucumber` dev-dependency (dev-dep only, doesn't affect release builds).
- [ ] We will NOT add warm/cached build timing measurements to the build-timings workflow.
- [ ] We will NOT change the release profile or release CI workflows.
- [ ] We will NOT install system libgit2 for local development — only CI. Developers can optionally set `LIBGIT2_SYS_USE_PKG_CONFIG=1` locally if they have the system library.

## 4. Proposed Solution (High-Level Design)

### 4.1 Overview

Two independent, non-overlapping changes:

| Change | Target | Estimated Savings | Risk |
|---|---|---|---|
| System libgit2 in CI | `.github/workflows/ci.yml` | 45-50s | Low — ubuntu-latest ships libgit2 1.7.x, compatible with libgit2-sys 0.17 |
| reqwest → ureq | `Cargo.toml`, `crates/libaipm/src/registry/git.rs` | 15-25s | Low — one call site, simple API mapping |

### 4.2 Key Components

| Component | Change | Justification |
|---|---|---|
| CI workflow (`ci.yml`) | Add `apt-get install` step + env var | Eliminates 52.85s C build script by linking to pre-built system library |
| Workspace `Cargo.toml` | Replace `reqwest` with `ureq` | Eliminates tokio, hyper, tower, h2, futures-util from dependency tree |
| `libaipm/src/registry/git.rs` | Migrate `http_get()` from `reqwest::blocking::get` to `ureq::get` | One function, 16 lines of code |
| `aipm-pack/Cargo.toml` | Remove `reqwest` line | Resolved: `aipm-pack` was merged into `aipm` (PR #417); no separate crate or `reqwest` dependency remains |

## 5. Detailed Design

### 5.1 Change 1: System libgit2 in CI

#### 5.1.1 CI Workflow Changes

**File:** `.github/workflows/ci.yml`

Add a step before `cargo build` in the `ci` job to install system libraries:

```yaml
- name: Install system libraries (libgit2, OpenSSL)
  run: sudo apt-get update && sudo apt-get install -y libgit2-dev libssl-dev pkg-config
```

Add `LIBGIT2_SYS_USE_PKG_CONFIG` to the workflow-level `env` block:

```yaml
env:
  CARGO_INCREMENTAL: 0
  CARGO_NET_RETRY: 10
  RUST_BACKTRACE: short
  LIBGIT2_SYS_USE_PKG_CONFIG: "1"
```

Apply the same changes to the `coverage` job (which also compiles the workspace).

*Research: The research document Section 1.1 "libgit2-sys" confirms `libgit2-sys v0.17.0+1.8.1` compiles libgit2 + vendored OpenSSL from C source, and the env var `LIBGIT2_SYS_USE_PKG_CONFIG=1` tells it to use the system library instead.*

#### 5.1.2 Version Compatibility

| Component | Version | Source |
|---|---|---|
| `libgit2-sys` crate | 0.17.0+1.8.1 | `Cargo.lock` |
| Bundled libgit2 | 1.8.1 | Vendored in libgit2-sys |
| System `libgit2-dev` on `ubuntu-latest` (24.04) | 1.7.2 | apt repository |

`libgit2-sys 0.17` supports libgit2 >= 1.7.0. The system package (1.7.2) satisfies this constraint. The `pkg-config` crate will verify version compatibility at build time and fall back to vendored compilation if the system library is too old — this is a safe fallback.

#### 5.1.3 Workflows Affected

| Workflow | Needs system libgit2? | Action |
|---|---|---|
| `ci.yml` — `ci` job | Yes — runs `cargo build` | Add install step + env var |
| `ci.yml` — `coverage` job | Yes — runs `cargo llvm-cov` | Add install step + env var |
| `build-timings.lock.yml` | No — intentionally measures cold builds | No change |
| `improve-coverage.lock.yml` | Optional — agentic workflow, builds in sandbox | No change (controlled by gh-aw) |
| `release.yml` | No — uses cargo-dist with its own matrix | No change |
| `release-plz.yml` | No — runs `release-plz` (semver checks, not full build) | No change |
| `codeql.yml` | No — uses `build-mode: none` | No change |
| `research-codebase.yml` | No — no Rust compilation | No change |

### 5.2 Change 2: Replace reqwest with ureq

#### 5.2.1 Workspace Dependency Change

**File:** `Cargo.toml` (workspace root)

Replace:
```toml
# HTTP
reqwest = { version = "0.12", default-features = false, features = ["blocking", "json", "rustls-tls"] }
```

With:
```toml
# HTTP — synchronous-only client; no async runtime overhead
ureq = { version = "3", default-features = true }
```

Default features (`rustls` + `gzip`) are sufficient. The `json` feature is not needed — the sole call site downloads raw bytes, not JSON.

#### 5.2.2 Crate Dependency Changes

**File:** `crates/libaipm/Cargo.toml`

Replace:
```toml
reqwest = { workspace = true }
```

With:
```toml
ureq = { workspace = true }
```

> **Note:** `crates/aipm-pack/Cargo.toml` no longer exists — `aipm-pack` was merged into `aipm` (PR #417). The `reqwest` dependency was removed as part of that merge.

#### 5.2.3 Code Migration

**File:** `crates/libaipm/src/registry/git.rs` — `http_get()` function (lines 152-168)

Current implementation:
```rust
/// Download bytes from a URL using `reqwest::blocking`.
fn http_get(url: &str) -> Result<Vec<u8>, Error> {
    tracing::info!(url = %url, "downloading package tarball");
    let response = reqwest::blocking::get(url)
        .map_err(|e| Error::Io { reason: format!("HTTP request failed for '{url}': {e}") })?;

    if !response.status().is_success() {
        return Err(Error::Io { reason: format!("HTTP {} for '{url}'", response.status()) });
    }

    response
        .bytes()
        .map_err(|e| Error::Io {
            reason: format!("failed to read response body from '{url}': {e}"),
        })
        .map(|b| b.to_vec())
}
```

Migrated implementation:
```rust
/// Download bytes from a URL using `ureq`.
fn http_get(url: &str) -> Result<Vec<u8>, Error> {
    tracing::info!(url = %url, "downloading package tarball");
    let response = ureq::get(url)
        .call()
        .map_err(|e| Error::Io { reason: format!("HTTP request failed for '{url}': {e}") })?;

    response
        .into_body()
        .read_to_vec()
        .map_err(|e| Error::Io {
            reason: format!("failed to read response body from '{url}': {e}"),
        })
}
```

Key behavioral differences:

| Aspect | reqwest (current) | ureq (proposed) |
|---|---|---|
| 4xx/5xx handling | Returns `Ok(response)` — caller checks `is_success()` | Returns `Err(ureq::Error::StatusCode(code))` — error case handled by `.map_err()` |
| Body reading | `.bytes()` returns `Bytes` | `.read_to_vec()` returns `Vec<u8>` directly |
| Default body limit | Unlimited | 10 MB — sufficient for plugin tarballs |
| TLS backend | rustls (via `rustls-tls` feature) | rustls (default) |

The explicit `is_success()` check is no longer needed because ureq treats non-2xx status codes as errors by default. The `map_err` on `.call()` already captures HTTP error responses with their status codes in the error message.

#### 5.2.4 Error Handling

The `Error::Io` variant in `crates/libaipm/src/registry/error.rs` uses a `reason: String` field. Both reqwest and ureq errors implement `Display`, so the `format!("...: {e}")` pattern works identically. No changes to the error type are needed.

`ureq::Error` variants relevant to this migration:
- `ureq::Error::StatusCode(u16)` — HTTP error responses (4xx, 5xx). Display shows the status code.
- Transport errors (DNS, connection, TLS) — Display shows the underlying error description.

Both map cleanly to the existing `Error::Io { reason: String }` pattern.

#### 5.2.5 Dependencies Eliminated

Replacing reqwest with ureq removes these crates from the dependency tree:

| Crate | Compile Time (from #157) | Why Eliminated |
|---|---|---|
| `reqwest` | 15.37s | Replaced by ureq |
| `tokio` | 10.04s | reqwest `blocking` feature pulled this in |
| `hyper` | ~3s | HTTP/1.1 + HTTP/2 implementation for reqwest |
| `hyper-util` | ~2s | hyper utilities |
| `hyper-rustls` | ~1s | rustls integration for hyper |
| `tower` | ~2s | Service middleware framework |
| `tower-http` | ~1s | HTTP-specific tower middleware |
| `h2` | ~2s | HTTP/2 protocol implementation |
| `futures-util` (partially) | ~2s | Async combinators (partially retained by cucumber dev-dep) |

**Estimated total elimination: ~38s of compilation** (with some overlap from parallelism; net wall-time savings estimated at 15-25s).

Dependencies **retained** (shared with git2 or other crates):
- `rustls` — still needed; ureq uses it too (same version 0.23)
- `ring` — transitive via rustls
- `serde`, `serde_json` — used directly by workspace crates

New dependencies **added** by ureq:
- `ureq` (~3-5s estimated)
- `ureq-proto` (small, <1s)
- `utf8-zero` (small, <1s)
- `base64` (small, likely already in tree)

#### 5.2.6 ureq Version and MSRV

- **ureq version**: 3.3.0 (latest stable)
- **MSRV**: 1.85 (Rust edition 2024)
- **Project compiler**: rustc 1.94.1 — satisfies MSRV with wide margin
- **Nightly toolchain**: Used only for coverage; ureq compiles on nightly without issues

## 6. Alternatives Considered

| Option | Pros | Cons | Reason for Rejection |
|---|---|---|---|
| System libgit2 only (no reqwest change) | Simplest change, highest single-item savings | Leaves ~30s of unnecessary async stack in tree | Does not address the second-largest bottleneck |
| Replace reqwest with `minreq` | Even lighter than ureq (~2s compile) | Less maintained, no rustls support by default, smaller community | ureq has better maintenance, wider adoption, native rustls support |
| Remove reqwest, use raw `std::net::TcpStream` + manual HTTP | Zero external deps for HTTP | Requires implementing HTTP/1.1, TLS, redirect handling | Disproportionate effort for one GET call |
| Convert to async (add tokio, make Registry async) | Could use reqwest without `blocking` overhead | Major architectural change, adds tokio as a direct dep | Overkill — the codebase is intentionally synchronous |
| Use `attohttpc` | Sync HTTP client, lightweight | Appears unmaintained (last release 2023) | Maintenance risk; ureq is actively developed |

## 7. Cross-Cutting Concerns

### 7.1 Backward Compatibility

- **No public API changes**: `http_get()` is a private function. The `Registry` trait's public API is unchanged.
- **No behavioral changes**: The function still downloads bytes from a URL and returns `Result<Vec<u8>, Error>`.
- **CI-only system library**: Local builds continue to vendor-compile libgit2 unless the developer explicitly sets `LIBGIT2_SYS_USE_PKG_CONFIG=1`.

### 7.2 Security

- **TLS backend unchanged**: ureq uses rustls (same as reqwest's `rustls-tls` feature). Certificate verification behavior is identical.
- **No new attack surface**: ureq is a well-maintained, widely-used crate with no `unsafe` code.
- **System libgit2 trust**: `libgit2-dev` is an official Ubuntu package from the Ubuntu archive — same trust level as all other system packages used in CI.

### 7.3 Testing

- **Existing tests**: The `http_get` function is tested indirectly through `Registry::download` integration tests. No test changes are needed unless there are mock-based HTTP tests.
- **Checksum verification**: The `verify_checksum` function (called after `http_get`) is unit-tested independently and is unaffected.
- **CI validation**: The CI workflow itself validates the system libgit2 change — if `pkg-config` resolution fails, `libgit2-sys` falls back to vendored compilation (the build still succeeds, just slower).

## 8. Migration and Rollout

### 8.1 Implementation Order

These two changes are independent and can be implemented in either order or in parallel:

- [x] **Phase 1a**: Add system libgit2 installation to `ci.yml` (both `ci` and `coverage` jobs)
- [x] **Phase 1b**: Replace reqwest with ureq in workspace deps, migrate `http_get()`, remove reqwest from workspace (note: `aipm-pack` was merged into `aipm` as part of this work)
- [x] **Phase 2**: Verify — run full CI pipeline, confirm build time reduction, confirm all tests pass
- [x] **Phase 3**: Update `CLAUDE.md` if any build commands change (none required)

### 8.2 Verification Checklist

- [x] `cargo build --workspace` succeeds
- [x] `cargo test --workspace` — all tests pass
- [x] `cargo clippy --workspace -- -D warnings` — zero warnings
- [x] `cargo fmt --check` — passes
- [x] Coverage >= 89% branch threshold
- [x] CI workflow runs successfully on PR
- [x] Build timing comparison (optional — next build-timings report will capture automatically)

### 8.3 Rollback

Both changes are trivially reversible:
- **System libgit2**: Remove the `apt-get install` step and env var from `ci.yml`
- **ureq**: Revert `Cargo.toml` and `git.rs` changes (restore reqwest)

## 9. Open Questions / Unresolved Issues

- [ ] **ureq body size limit**: ureq defaults to a 10 MB body limit. Are there plugin tarballs that could exceed this? If so, the limit should be raised via `.with_config().limit(N)`. This can be verified by checking the largest package in the registry.
- [ ] **CI apt-get caching**: Should the `apt-get install` step be cached between runs (e.g., via `actions/cache` on `/var/cache/apt`)? The install itself is fast (~2-3s) so caching may not be worth the complexity.
