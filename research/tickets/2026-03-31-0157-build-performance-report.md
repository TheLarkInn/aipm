---
date: 2026-03-31
researcher: Claude (Opus 4.6)
git_commit: 1b8483daae7b50608a93a114404330d1e235d222
branch: main
repository: aipm
topic: "Build Performance Report — Issue #157: Analysis of build times, dependency bottlenecks, CI caching, and feature audit"
tags: [research, build-performance, libgit2, reqwest, syn, ci-cd, dependencies, caching]
status: complete
last_updated: 2026-03-31
last_updated_by: Claude (Opus 4.6)
---

# Research: Build Performance Report #157

## Research Question

Comprehensive analysis of [TheLarkInn/aipm#157](https://github.com/TheLarkInn/aipm/issues/157) — the third build timings report (2026-03-31). Covers dependency configuration, CI/CD pipeline, historical trends across reports #135/#150/#157, and a feature-level audit of the top bottleneck crates (`libgit2-sys`, `syn`, `reqwest`).

## Summary

The cold build time is **119s** (241 compilation units) on a 4-core runner. The critical-path bottleneck remains **`libgit2-sys` at 52.85s** (44% of wall time), compiling libgit2 and vendored OpenSSL from C source. This bottleneck has been identified in all three reports (#135, #150, #157) and the recommended fix (system library installation in CI) has not been implemented.

Two optimizations from #135 are confirmed retained: SSH feature removal from `git2` (eliminated `libssh2-sys` — saved ~8.7s) and `[profile.dev]` tuning (line-table-only debug, split debuginfo, per-package opt-level overrides). No new optimizations have been applied since #150.

The `syn` crate compiles with all features (`extra-traits`, `fold`, `full`, `visit`, `visit-mut`) because 10+ transitive proc-macro dependencies request them — this is not controllable from the workspace. The `reqwest` `blocking` feature is actively used (one call site) and is required because the codebase has no async runtime.

---

## Detailed Findings

### 1. Dependency Configuration

#### 1.1 libgit2-sys — 52.85s (44% of wall time)

The full dependency chain:

```
libaipm (workspace crate)
  → git2 v0.19 [default-features = false, features = ["https"]]
    → libgit2-sys v0.17.0+1.8.1 [features: default, https, openssl-sys]
      → (compiles libgit2 1.8.1 + vendored OpenSSL from C source)
```

- **Workspace declaration:** [`Cargo.toml:44`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/Cargo.toml#L44)
  ```toml
  git2 = { version = "0.19", default-features = false, features = ["https"] }
  ```
- **Used by:** `libaipm` only ([`crates/libaipm/Cargo.toml:27`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/Cargo.toml#L27))
- **SSH already removed:** The comment at `Cargo.toml:43` notes SSH features were removed to avoid `libssh2-sys` C compilation (~8.7s savings), applied after #135
- **System library env var not set:** No workflow sets `LIBGIT2_SYS_USE_PKG_CONFIG=1`
- **No system packages installed:** No workflow runs `apt-get install libgit2-dev libssl-dev pkg-config`

#### 1.2 syn — 28.20s

A single `syn v2.0.117` is compiled with **all** features activated:

| Feature | Pulled by |
|---|---|
| `extra-traits` | `cucumber-codegen`, `synstructure`, `synthez`, `synthez-core`, `tracing-attributes`, `typed-builder-macro`, `zerovec-derive`, `derive_more` v0.99 |
| `fold` | `yoke-derive`, `zerofrom-derive` (ICU/Unicode chain via `url` → `reqwest`/`git2`) |
| `full` | `clap_derive`, `cucumber-codegen`, `futures-macro`, `lazy-regex-proc_macros`, `pin-project-internal`, `sealed`, `tracing-attributes`, `typed-builder-macro` |
| `visit` | `synstructure` |
| `visit-mut` | `pin-project-internal`, `tracing-attributes` |

**Root cause analysis:** The `extra-traits` feature (adds `Debug`, `Eq`, `Hash` impls to all AST types) and `fold` feature (adds fold visitor infrastructure) are the most costly additions. Both are requested by transitive proc-macro dependencies — not controllable from the workspace `Cargo.toml`. The `extra-traits` requestors are primarily `cucumber` and its ecosystem crates (`cucumber-codegen`, `synthez`, `sealed`, etc.), plus `tracing-attributes` which is a direct dependency. The `fold` requestors are ICU Unicode normalization crates in the `url` → `idna` → `icu_*` chain, which is pulled by both `reqwest` and `git2`.

#### 1.3 reqwest — 15.37s

**Feature chain:**
```
reqwest v0.12.28
  ├── blocking    ← aipm-pack, libaipm
  ├── json        ← aipm-pack, libaipm
  └── rustls-tls  ← aipm-pack, libaipm
       └── rustls-tls-webpki-roots → __rustls → __rustls-ring → __tls
```

**Usage audit:** There is exactly **one** call site for reqwest in the entire codebase:

- [`crates/libaipm/src/registry/git.rs:155`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/registry/git.rs#L155) — `reqwest::blocking::get(url)` inside `fn http_get(url: &str) -> Result<Vec<u8>, Error>`
- Called from the `Registry::download` trait implementation at [line 211](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/registry/git.rs#L211)

**`blocking` feature is required:** The `Registry` trait ([`crates/libaipm/src/registry/mod.rs:86`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/registry/mod.rs#L86)) defines synchronous `fn` signatures. Both binaries (`aipm` and `aipm-pack`) have synchronous `fn main()` entry points. There is no async runtime (no tokio, async-std, or smol) in production code. Converting to async would require adding tokio and making `Registry` an async trait — a significant architectural change.

**`aipm-pack` declares reqwest but does not use it:** [`crates/aipm-pack/Cargo.toml:19`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/aipm-pack/Cargo.toml#L19) lists `reqwest = { workspace = true }` but zero source files under `crates/aipm-pack/src/` reference reqwest. This does not affect build time since `libaipm` already requires it, but it is a dead dependency declaration.

#### 1.4 Profile Configuration

[`Cargo.toml:147-183`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/Cargo.toml#L147-L183) — Applied after #135:

| Profile | Setting | Value | Purpose |
|---|---|---|---|
| `dev` | `opt-level` | 0 | Fast workspace builds |
| `dev` | `debug` | 1 | Line tables only (faster than full symbols) |
| `dev` | `split-debuginfo` | "unpacked" | Faster linking on Linux |
| `dev.package."*"` | `opt-level` | 1 | Third-party crates at opt-level 1 for cache efficiency |
| `dev.package.{libaipm,aipm,aipm-pack}` | `opt-level` | 0 | Workspace crates stay at zero opt |
| `release` | `opt-level` | 3 | Full optimization |
| `release` | `lto` | "thin" | Link-time optimization |
| `release` | `codegen-units` | 1 | Single codegen unit for smaller binaries |
| `release` | `strip` | "symbols" | Strip debug symbols |

#### 1.5 Duplicate Crate Versions

5 families duplicated — stable across all 3 reports. All trace to `cucumber` (dev-dep):

| Crate | Versions | Root Cause |
|---|---|---|
| `thiserror` | v1.0.69 + v2.0.18 | `cucumber` → `gherkin` pulls v1; workspace uses v2 |
| `derive_more` | v0.99.20 + v2.1.1 | `cucumber` pulls v0.99; `crossterm`/`inquire` use v2 |
| `getrandom` | v0.2.17 + v0.4.2 | `ring` needs v0.2; `tempfile` uses v0.4 |
| `heck` | v0.4.1 + v0.5.0 | `sealed` (via `cucumber`) v0.4; `clap_derive` v0.5 |
| `regex-syntax` | v0.7.5 + v0.8.10 | `cucumber-expressions` v0.7; `globset`/`regex` v0.8 |

---

### 2. CI/CD Pipeline Analysis

#### 2.1 Caching Configuration

| Workflow | Job | `Swatinem/rust-cache` | Notes |
|---|---|---|---|
| `ci.yml` | `ci` | Yes (line 27) | Caches `target/` and cargo registry |
| `ci.yml` | `coverage` | Yes (line 60) | Separate cache (nightly toolchain) |
| `build-timings.lock.yml` | `agent` | **No** | By design — measures cold builds |
| `improve-coverage.lock.yml` | `agent` | Yes (line 306) | Only agentic workflow with cache |
| `release.yml` | all jobs | **No** | Uses cargo-dist binary; matrix-driven |
| `release-plz.yml` | all jobs | **No** | Quick release operations |
| `codeql.yml` | `analyze` | **No** | No Rust compilation (build-mode: none) |
| `research-codebase.yml` | `research` | **No** | No Rust compilation |

#### 2.2 Environment Variables Affecting Builds

| Variable | Value | Set In |
|---|---|---|
| `CARGO_INCREMENTAL` | `0` | `ci.yml` (both jobs) |
| `CARGO_NET_RETRY` | `10` | `ci.yml` |
| `RUST_BACKTRACE` | `short` | `ci.yml` |
| `CARGO_TERM_COLOR` | `always` | `ci.yml` (coverage job only) |

The agentic workflows (`build-timings`, `improve-coverage`) do not set `CARGO_INCREMENTAL` explicitly — the agent sandbox inherits the runner default.

#### 2.3 System Library Installation

**No workflow** installs system development libraries (`libgit2-dev`, `libssl-dev`, `pkg-config`). The `release.yml` workflow uses `${{ matrix.packages_install }}` from the cargo-dist plan matrix, but the actual packages installed are determined at runtime and are for cross-compilation support, not build optimization.

#### 2.4 No `.cargo/config.toml`

No `.cargo/config.toml` or `.cargo/config` file exists. There are no linker overrides, registry mirrors, or custom build flags configured at the cargo level.

#### 2.5 No Dockerfile or Devcontainer

No `Dockerfile`, `.devcontainer/`, or container configuration exists in the repository.

---

### 3. Historical Trend (Issues #135 → #150 → #157)

| Metric | #135 (2026-03-28) | #150 (2026-03-30) | #157 (2026-03-31) | Trend |
|---|---|---|---|---|
| Total build time (cold) | 47.97s | 113.5s | 119s | Stable (env-dependent) |
| Compilation units | 243 | 241 | 241 | Stable |
| `libgit2-sys` | 19.80s | 51.06s | 52.85s | Still #1 bottleneck |
| `libssh2-sys` | 8.72s | **Gone** | **Gone** | Eliminated after #135 |
| `syn` | 4.93s | 26.81s | 28.20s | Grew with `extra-traits`/`fold` |
| `rustls` | 5.89s | 22.28s | 23.16s | Stable (env-dependent) |
| `reqwest` | 4.10s | 16.16s | 15.37s | Stable (env-dependent) |
| `clap_builder` | 3.80s | 13.11s | 13.86s | Stable |
| `[profile.dev]` tuning | Missing | Applied | Retained | Applied after #135 |
| Duplicate crate families | 5 | 5 | 5 | Stable |
| System libgit2 in CI | Not done | Not done | Not done | Pending across all reports |

**Note on time variation:** The jump from 47.97s (#135) to 113.5s (#150) and 119s (#157) is likely due to different runner hardware/load, not code changes. The relative ranking and proportions remain consistent — `libgit2-sys` is always 40-45% of wall time.

#### Optimizations Applied (after #135)

1. `git2` switched to `default-features = false, features = ["https"]` — eliminated `libssh2-sys` build script (saved ~8.7s)
2. `[profile.dev]` section added: `debug = 1`, `split-debuginfo = "unpacked"`, per-package `opt-level` overrides

#### Optimizations NOT Yet Applied

1. **System libgit2/openssl in CI** — recommended in all 3 reports, estimated savings 45-50s on cold builds
2. **Audit/remove unused reqwest dep from aipm-pack** — dead declaration, no build time impact (already compiled for libaipm)
3. **cucumber upgrade** — no newer version available that resolves duplicate crates

---

### 4. Feature Audit Results

#### 4.1 syn Feature Provenance

| Feature | Compile-time cost | Controllable from workspace? | Requestors |
|---|---|---|---|
| `extra-traits` | High (Debug/Eq/Hash for all AST types) | No — transitive proc-macros | 7+ crates, primarily `cucumber-codegen`, `tracing-attributes`, `synstructure` |
| `fold` | Medium (fold visitor infrastructure) | No — ICU/Unicode chain | `yoke-derive`, `zerofrom-derive` (via `url` → `idna` → `icu_*`) |
| `full` | High (full AST parsing) | No — proc-macro crates | `clap_derive`, `cucumber-codegen`, `futures-macro`, `tracing-attributes` |
| `visit` | Low | No — `synstructure` | `synstructure` (via `yoke-derive`, `zerofrom-derive`, `cucumber`) |
| `visit-mut` | Low | No — proc-macro crates | `pin-project-internal`, `tracing-attributes` |

**Conclusion:** All `syn` features are requested by transitive proc-macro dependencies. None can be disabled from the workspace `Cargo.toml`. The `extra-traits` feature is the most impactful and is primarily driven by the `cucumber` test framework ecosystem and `tracing-attributes` (a direct prod dependency). The `fold` feature comes from the ICU Unicode normalization chain (`url` → `idna` → `icu_normalizer` → `yoke`/`zerofrom` → `yoke-derive`/`zerofrom-derive`), which is pulled by both `reqwest` and `git2`.

#### 4.2 reqwest `blocking` Feature Necessity

| Question | Answer |
|---|---|
| Is `blocking` feature used in code? | Yes — 1 call site at `crates/libaipm/src/registry/git.rs:155` |
| Could it be replaced with async? | Requires adding tokio, making `Registry` an async trait, and converting both binaries to `#[tokio::main]` |
| Is there an async runtime in prod? | No — no tokio, async-std, or smol in production code |
| Is the `blocking` feature the expensive part? | `blocking` pulls in `tokio` as a transitive dep (10.04s), plus `futures-util` — adds ~15-20s of transitive compilation |
| Is reqwest unused in `aipm-pack`? | Yes — declared but no source references |

**Conclusion:** The `blocking` feature is actively needed for the current synchronous architecture. Removing it would require a significant refactor to async. The `reqwest` dependency in `aipm-pack` is unused and could be removed from its `Cargo.toml`, though this has no build time impact since `libaipm` already requires it.

---

## Code References

- [`Cargo.toml:41`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/Cargo.toml#L41) — reqwest workspace dependency with `blocking`, `json`, `rustls-tls`
- [`Cargo.toml:44`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/Cargo.toml#L44) — git2 workspace dependency with `default-features = false, features = ["https"]`
- [`Cargo.toml:147-165`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/Cargo.toml#L147-L165) — `[profile.dev]` configuration
- [`crates/libaipm/Cargo.toml:27-28`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/Cargo.toml#L27-L28) — git2 and reqwest deps in libaipm
- [`crates/libaipm/src/registry/git.rs:152-168`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/registry/git.rs#L152-L168) — `http_get()` — sole reqwest usage
- [`crates/libaipm/src/registry/git.rs:199-216`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/registry/git.rs#L199-L216) — `Registry::download` calling `http_get`
- [`crates/libaipm/src/registry/mod.rs:83-86`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/registry/mod.rs#L83-L86) — `Registry` trait (synchronous)
- [`crates/aipm-pack/Cargo.toml:19`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/aipm-pack/Cargo.toml#L19) — reqwest declared but unused
- [`.github/workflows/ci.yml`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/.github/workflows/ci.yml) — Main CI pipeline
- [`.github/workflows/build-timings.lock.yml`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/.github/workflows/build-timings.lock.yml) — Build timings analyzer workflow

## Architecture Documentation

### Build Pipeline Architecture

The project uses a workspace with 3 crates (`aipm`, `aipm-pack`, `libaipm`). All crates inherit lint configuration and most dependencies from the workspace root. The build pipeline in CI consists of 4 sequential steps: build → test → clippy → format check. Coverage runs as a parallel job on the nightly toolchain.

### Dependency Architecture

- **`libaipm`** is the core library — it pulls in the heavy dependencies: `git2` (→ `libgit2-sys`), `reqwest` (→ `tokio`, `rustls`, `hyper`), `rayon`, `ignore`, `toml_edit`
- **`aipm`** and **`aipm-pack`** are thin CLI binaries — primarily `clap`, `inquire`, `serde`, and `tracing` on top of `libaipm`
- **`cucumber`** is a dev-dependency of `libaipm` only — drives BDD tests, pulls in 5 duplicate crate families and contributes heavily to `syn` feature unification

### Critical Path Analysis

The critical path for a cold build is dominated by sequential C compilation:
1. `libgit2-sys` build script (52.85s) — compiles libgit2 + vendored OpenSSL
2. While that runs, Rust crates compile in parallel, but the C build script serializes
3. After `libgit2-sys` completes, `git2` and dependent crates can finish
4. Total parallelism ratio: 434.66s sum / 119s wall = 3.65x (limited by C build scripts)

## Historical Context (from research/)

No existing research documents in `research/` cover build performance, build timings, or `libgit2` optimization. The following documents provide related context:

- `research/docs/2026-03-16-rust-cross-platform-release-distribution.md` — Cross-platform binary distribution and CI/CD release automation (cargo-dist, build matrix)
- `research/docs/2026-03-19-cargo-dist-installer-github-releases.md` — cargo-dist integration for installers and GitHub Releases
- `research/docs/2026-03-22-rust-code-coverage-implementation.md` — Code coverage as correctness gate (cargo-llvm-cov, CI gating)
- `research/tickets/2026-03-28-129-workspace-dependencies-linking.md` — Workspace dependency linking (resolver, lockfile)
- `research/docs/2026-03-09-cargo-core-principles.md` — Cargo architectural principles (registry model, workspace dependencies)

## Related Research

- [TheLarkInn/aipm#135](https://github.com/TheLarkInn/aipm/issues/135) — First build timings report (2026-03-28, closed) — baseline at 47.97s
- [TheLarkInn/aipm#150](https://github.com/TheLarkInn/aipm/issues/150) — Second build timings report (2026-03-30, closed) — confirmed SSH removal and profile.dev applied
- [TheLarkInn/aipm#157](https://github.com/TheLarkInn/aipm/issues/157) — Third build timings report (2026-03-31, open) — current report under research

## Open Questions

1. **System libgit2 in CI**: The `LIBGIT2_SYS_USE_PKG_CONFIG=1` approach has been recommended in all 3 reports. Is there a specific reason it has not been implemented? Are there concerns about version compatibility between system `libgit2-dev` and `libgit2-sys v0.17.0+1.8.1`?

2. **reqwest alternative**: Could `ureq` (a synchronous-only HTTP client, ~5s compile time) replace `reqwest` for the single `GET` call? This would eliminate reqwest (15.37s), tokio (10.04s from `blocking` feature), rustls (23.16s — though still needed by git2?), and hyper from the dependency tree. However, `ureq` may pull in its own TLS stack.

3. **cucumber alternatives**: The `cucumber v0.21` dev-dep is the source of all 5 duplicate crate families and contributes heavily to `syn` feature bloat. Is there a newer version or alternative BDD framework with fewer transitive dependencies?

4. **Build timings workflow design**: The `build-timings.lock.yml` workflow intentionally runs `cargo clean` for cold build measurement. Should there also be a warm/cached build timing measurement for comparison?
