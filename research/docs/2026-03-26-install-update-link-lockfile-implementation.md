---
date: 2026-03-26 14:30:00 UTC
researcher: Claude (Opus 4.6)
git_commit: 13c6bf26200aa78d065910a8a3029c31285e0a97
branch: main
repository: aipm
topic: "Implementation readiness for aipm install, update, link/unlink, and lockfile generation"
tags: [research, codebase, install, update, link, lockfile, resolver, content-store, dependency-management]
status: complete
last_updated: 2026-03-26
last_updated_by: Claude (Opus 4.6)
---

# Research: Install, Update, Link, and Lockfile Implementation Readiness

## Research Question

Document the complete implementation state and requirements for `aipm install`, `aipm update`, `aipm link`/`aipm unlink`, and lockfile generation (`aipm.lock`). Coordinate all prior research, specs, and BDD scenarios into a single implementation-readiness reference.

## Summary

The AIPM codebase has a mature foundation for manifest parsing, validation, workspace initialization, and migration — approximately 7,500 lines of Rust across 33 source files. However, the entire package management pipeline (resolve, fetch, store, link, lockfile) remains unbuilt. Six major new modules are needed: dependency resolver, lockfile manager, content-addressable store, package installer, link manager, and registry client. The project has 83 BDD scenarios across 6 feature files that specify exact behavioral requirements for these features, and several workspace dependencies (`sha2`, `flate2`, `tar`, `junction`, `reqwest`) are already declared but unused — ready for implementation.

---

## Detailed Findings

### 1. Current Implementation State

#### Existing Modules (Fully Implemented)

| Module | Location | Lines | Purpose |
|--------|----------|-------|---------|
| `lib.rs` | [`crates/libaipm/src/lib.rs`](https://github.com/TheLarkInn/aipm/blob/13c6bf26200aa78d065910a8a3029c31285e0a97/crates/libaipm/src/lib.rs) | ~29 | Crate root; exports 6 public modules |
| `manifest/` | [`crates/libaipm/src/manifest/`](https://github.com/TheLarkInn/aipm/blob/13c6bf26200aa78d065910a8a3029c31285e0a97/crates/libaipm/src/manifest/mod.rs) | ~1,070 | Full `aipm.toml` parsing, types, and validation |
| `init.rs` | [`crates/libaipm/src/init.rs`](https://github.com/TheLarkInn/aipm/blob/13c6bf26200aa78d065910a8a3029c31285e0a97/crates/libaipm/src/init.rs) | ~620 | Package initialization (`aipm-pack init`) |
| `fs.rs` | [`crates/libaipm/src/fs.rs`](https://github.com/TheLarkInn/aipm/blob/13c6bf26200aa78d065910a8a3029c31285e0a97/crates/libaipm/src/fs.rs) | ~150 | Filesystem abstraction trait (`Fs`) for testability |
| `version.rs` | [`crates/libaipm/src/version.rs`](https://github.com/TheLarkInn/aipm/blob/13c6bf26200aa78d065910a8a3029c31285e0a97/crates/libaipm/src/version.rs) | ~336 | Semver `Version` and `Requirement` wrappers with `select_best()` |
| `workspace_init/` | [`crates/libaipm/src/workspace_init/`](https://github.com/TheLarkInn/aipm/blob/13c6bf26200aa78d065910a8a3029c31285e0a97/crates/libaipm/src/workspace_init/mod.rs) | ~750 | Workspace + marketplace scaffolding with tool adaptors |
| `migrate/` | [`crates/libaipm/src/migrate/`](https://github.com/TheLarkInn/aipm/blob/13c6bf26200aa78d065910a8a3029c31285e0a97/crates/libaipm/src/migrate/mod.rs) | ~4,500 | Full migration pipeline from `.claude/` to marketplace plugins |

#### CLI Binaries

**`aipm` (consumer)** — [`crates/aipm/src/main.rs`](https://github.com/TheLarkInn/aipm/blob/13c6bf26200aa78d065910a8a3029c31285e0a97/crates/aipm/src/main.rs): Implements `init` and `migrate` subcommands. Declared but unimplemented: `install`, `validate`, `doctor`, `link`, `update`, `uninstall`.

**`aipm-pack` (author)** — [`crates/aipm-pack/src/main.rs`](https://github.com/TheLarkInn/aipm/blob/13c6bf26200aa78d065910a8a3029c31285e0a97/crates/aipm-pack/src/main.rs): Implements `init` subcommand only. Declared but unimplemented: `pack`, `publish`, `yank`, `login`.

#### Missing Modules (0 Lines Each — Not Started)

| Module | Purpose | Key Dependencies |
|--------|---------|-----------------|
| **Dependency resolver** | Backtracking constraint solver, version unification, conflict reporting | `semver` (active), custom algorithm |
| **Lockfile manager** | Read/write `aipm.lock`, deterministic serialization, drift detection | `toml` + `serde` (active for manifests) |
| **Content-addressable store** | SHA-512 hashing, global store at `~/.aipm/store/`, hard-linking | `sha2` (declared, unused) |
| **Package installer** | Orchestrate resolve→fetch→store→link pipeline | All dependencies |
| **Link manager** | Symlinks (Unix), junctions (Windows), gitignore management | `junction` (declared, unused), `std::os::unix::fs` |
| **Registry client** | HTTP API for package metadata/downloads | `reqwest` (declared in aipm-pack only) |

---

### 2. Spec Requirements Mapped to Implementation Tasks

From [technical design spec sections 5.3–5.8](https://github.com/TheLarkInn/aipm/blob/13c6bf26200aa78d065910a8a3029c31285e0a97/specs/2026-03-09-aipm-technical-design.md):

#### 2.1 Dependency Resolution (Spec §5.3)

**Algorithm**: Backtracking constraint solver (Cargo/pubgrub-inspired):

1. Build dependency graph from root manifest + all workspace members
2. For each unresolved dep, try **highest compatible version** first
3. **Unify** within same semver-major (single version per major where possible)
4. Semver-**incompatible** ranges (e.g., `^1.0` and `^2.0`) → both coexist in graph (Cargo model)
5. Conflict within same major → **backtrack** to next candidate
6. Apply **overrides** before resolution (forced versions bypass solver)
7. Exclude **yanked** versions unless pinned in lockfile

**No peer dependencies** — AIPM uses aggressive version unification instead. Since AI plugins are markdown/JSON/config, coexisting major versions are safe.

**Existing building blocks**: `version::Requirement::select_best()` selects the highest matching version from a flat candidate list. This is the comparison primitive; the graph traversal and backtracking logic must be built.

#### 2.2 Lockfile Behavior (Spec §5.3.1 — Cargo Model)

| Command | Behavior |
|---------|----------|
| `aipm install` | If lockfile exists, uses locked versions. On manifest change, does **minimal reconciliation** — resolves only new/changed entries, keeps existing pins untouched. **Never upgrades**. |
| `aipm install --locked` | CI mode. Fails immediately if lockfile doesn't match manifest. Zero drift tolerance. |
| `aipm update [pkg]` | The **only** command that upgrades. `aipm update skill-a` updates only that dep. `aipm update` re-resolves everything. |

**Lockfile format** (Spec §5.7):
```toml
[metadata]
lockfile_version = 1
generated_by = "aipm 0.1.0"

[[package]]
name = "@company/code-review"
version = "1.2.0"
source = "registry+https://registry.aipm.dev"
checksum = "sha512-abc123..."
dependencies = ["shared-lint-skill@^1.0"]
```

**Key property**: Single workspace lockfile at the workspace root. All members share it.

#### 2.3 Content-Addressable Store (Spec §5.4)

- **Global store**: `~/.aipm/store/` (configurable). Files indexed by SHA-512 hash with 2-char prefix directories.
- **Project working set**: `.aipm/links/` with files **hard-linked** from global store.
- **Directory links**: `claude-plugins/<pkg>` → `.aipm/links/<pkg>` (symlinks on Unix, junctions on Windows).
- **Dedup**: Identical files stored exactly once. New version changing 1 of 100 files stores only 1 new file.

**Gotcha**: Hard links require source and target on the same filesystem volume. Cross-volume situations need fallback (copy) or a warning.

#### 2.4 Install Flow (Spec §5.5)

Seven-step pipeline:
1. **Resolve**: Find highest compatible version, resolve transitive deps
2. **Fetch**: Download `.aipm` archives not in global store
3. **Store**: Extract files into global store by content hash (skip existing = dedup)
4. **Link**: Assemble `.aipm/links/<pkg>/` with hard links from store
5. **Link into plugins dir**: Create `claude-plugins/<pkg>` symlink/junction → `.aipm/links/...` + add to `.gitignore`
6. **Manifest**: Add to `[dependencies]` in `aipm.toml`
7. **Lock**: Update `aipm.lock` with exact versions + integrity hashes

#### 2.5 Linking Strategy (Spec §5.5.1)

| Link Type | Used For | Platform | Elevation |
|-----------|----------|----------|-----------|
| Hard link | Files: `.aipm/links/foo/SKILL.md` → `~/.aipm/store/ab/cd12...` | All | No (same volume) |
| Symlink | Directory: `claude-plugins/foo` → `.aipm/links/foo` | macOS/Linux | No |
| Junction | Directory: `claude-plugins/foo` → `.aipm/links/foo` | Windows | **No** |

Implementation pattern:
```rust
#[cfg(unix)]     std::os::unix::fs::symlink(source, target)?;
#[cfg(windows)]  junction::create(source, target)?;
```

#### 2.6 Local Dev Overrides — `aipm link` (Spec §5.8)

- Replaces registry dep's symlink with symlink to local directory
- Does **not** modify lockfile or manifest (local and ephemeral)
- `aipm install --locked` removes all links and restores registry versions
- `aipm unlink <pkg>` restores original registry symlink
- Link state stored locally (gitignored), likely `.aipm/link-state.toml`

#### 2.7 Workspace Protocol (Spec §5.9)

```toml
[dependencies]
core-hooks = { workspace = "^" }  # link to workspace sibling
```

On publish: `workspace = "^"` → `"^2.3.0"` (actual version). Catalogs provide shared version ranges via `[catalog]` and `[catalogs.stable]` sections.

---

### 3. BDD Scenario Inventory

**Total: 83 scenarios across 6 feature files (77 P0, 6 P1)**

#### 3.1 `tests/features/registry/install.feature` — 23 scenarios (all P0)

| Rule | Scenarios | Coverage |
|------|-----------|----------|
| Top-level | 10 | Install by name, specific version, latest compatible, scoped, alt registry, nonexistent, install-all, `--locked`, integrity check, yanked exclusion |
| Content-addressable store | 4 | CAS storage, cross-version dedup, multi-project sharing, custom store location |
| Strict dependency isolation | 2 | Only declared deps accessible, phantom dep prevention |
| Side-effects cache | 4 | Lifecycle script caching, skip recompilation, blocked-by-default, explicit allowlist |
| Cross-platform linking | 3 | Symlink on macOS/Linux, junction on Windows, no-elevation Windows |

#### 3.2 `tests/features/dependencies/lockfile.feature` — 12 scenarios (all P0)

| Rule | Scenarios | Coverage |
|------|-----------|----------|
| Lockfile creation | 4 | First install creates lockfile, respects existing pins, records tree structure, deterministic across platforms |
| Minimal reconciliation | 3 | Add dep only resolves new entry, remove dep prunes lockfile, changed range only re-resolves changed entry |
| Locked install (CI) | 2 | Aborts on mismatch, succeeds on match |
| Explicit update | 3 | Update specific dep, update all, update respects version ranges |

#### 3.3 `tests/features/dependencies/resolution.feature` — 13 scenarios (all P0)

| Rule | Scenarios | Coverage |
|------|-----------|----------|
| Top-level | 7 | Simple tree, diamond unification, multi-major coexistence, circular detection, highest-first, backtracking, conflict reporting |
| Overrides | 3 | Global override, scoped override, fork replacement |
| Version coexistence | 3 | Same-major unification, cross-major coexistence, independent CAS storage |

#### 3.4 `tests/features/registry/link.feature` — 10 scenarios (all P0)

| Rule | Scenarios | Coverage |
|------|-----------|----------|
| Link override | 6 | Link local dir, preserve lockfile, validate manifest match, no-manifest error, name-mismatch error, link without install |
| Unlink restore | 3 | Restore registry link, unlink link-only package, `--locked` removes all links |
| List linked | 1 | Show currently linked packages |

#### 3.5 `tests/features/registry/local-and-registry.feature` — 19 scenarios (all P0)

| Rule | Scenarios | Coverage |
|------|-----------|----------|
| Registry linking into plugins dir | 5 | Directory link creation, auto-gitignore, local plugins preserved, uninstall cleanup, Claude Code discovery |
| Local plugins with registry deps | 4 | Local plugin registry deps, workspace deps, mixed deps, transitive deps |
| Non-workspace mode | 3 | No `[workspace]` section, default plugins dir, custom plugins dir |
| Vendored plugins | 3 | Vendor copy, workspace member, outdated detection |
| Gitignore management | 4 | First create, append, scoped packages, preserve manual entries |

#### 3.6 `tests/features/dependencies/features.feature` — 6 scenarios (all P1)

| Scenarios | Coverage |
|-----------|----------|
| 6 | Default features, opt-out, enable specific, additive across graph, optional dep activation, feature-conditional components |

#### Scenarios by Implementation Component

| Component | Scenario Count | Key Feature Files |
|-----------|---------------|-------------------|
| Resolver | 21 | resolution.feature, features.feature, lockfile.feature, install.feature |
| Lockfile | 15 | lockfile.feature, install.feature, link.feature |
| Installer (end-to-end) | 14 | install.feature, local-and-registry.feature |
| Linker | 13 | install.feature, local-and-registry.feature, link.feature, resolution.feature |
| Link CLI (link/unlink/list) | 10 | link.feature |
| Gitignore manager | 6 | local-and-registry.feature |
| Content store | 6 | install.feature, resolution.feature, local-and-registry.feature |
| Side-effects cache | 4 | install.feature |
| Vendor | 3 | local-and-registry.feature |
| Registry client | 2 | install.feature |

---

### 4. Dependency/Crate Analysis

#### Active Dependencies (Already Used in Source Code)

| Crate | Version | Current Usage | Install/Update/Link Role |
|-------|---------|---------------|--------------------------|
| `semver` | 1 (+ `serde` feature) | `version.rs` — `Version` + `Requirement` newtypes with `matches()` and `select_best()`. `validate.rs` — validates version requirement strings. | Core resolver primitive. `serde` feature enables direct lockfile serialization. |
| `toml` | 0.8 | `manifest/mod.rs` — `toml::from_str()` for manifest parsing. `migrate/emitter.rs` — `toml::to_string_pretty()` for manifest generation. | Lockfile read/write serialization. |
| `serde` | 1 (+ `derive`) | `manifest/types.rs` — `#[derive(Deserialize)]` on all manifest types. `migrate/emitter.rs` — `#[derive(Serialize)]` on emit types. | `Serialize` derives on lockfile structs. |
| `serde_json` | 1 (+ `preserve_order`) | `migrate/mcp_detector.rs`, `migrate/registrar.rs`, `workspace_init/adaptors/claude.rs` — JSON parsing for MCP configs, marketplace.json, Claude settings. | Registry API JSON responses. |
| `ignore` | 0.4 | `migrate/discovery.rs` — gitignore-aware directory walking. | Pack file walking (`.aipm` archive creation). |
| `rayon` | 1 | `migrate/mod.rs` — parallel iteration during detection/emission. | Parallel fetch, extraction, and linking. |
| `thiserror` | 2 | 5 error types across init, migrate, workspace_init, manifest, version. | New error types for resolver, store, lockfile, link, registry. |

#### Declared but Unused Dependencies (Ready for Implementation)

| Crate | Version | Planned Role |
|-------|---------|-------------|
| `sha2` | 0.10 | SHA-512 content hashing for store and integrity verification. In `libaipm` Cargo.toml. |
| `flate2` | 1 | Gzip decompression of `.aipm` archives. In `libaipm` Cargo.toml. |
| `tar` | 0.4 | Tar extraction of `.aipm` archives. In `libaipm` Cargo.toml. |
| `junction` | 1 | Windows directory junctions (no elevation). In `libaipm` `[target.'cfg(windows)'.dependencies]`. |
| `reqwest` | 0.12 (`json` + `rustls-tls`) | HTTP client for registry API. **Currently only in `aipm-pack`**, not `libaipm` or `aipm`. |
| `tracing` | 0.1 | Structured logging for all subsystems. Declared in all 3 crates. |
| `tracing-subscriber` | 0.3 | Log subscriber initialization. In `aipm` and `aipm-pack`. |

#### Gaps — Dependencies That May Need Adding

| Need | Crate(s) | Reason |
|------|----------|--------|
| `reqwest` in `libaipm` or `aipm` | `reqwest` | Consumer CLI needs to fetch packages. Currently only in `aipm-pack`. |
| Async runtime | `tokio` (or `reqwest` `blocking` feature) | `reqwest` 0.12 requires async runtime unless `blocking` feature is enabled. |
| TOML-preserving editor | `toml_edit` | `aipm install <pkg>` must add `[dependencies]` entries without destroying comments/formatting. Current types only have `Deserialize`, not `Serialize`. |
| File locking | `fs2` or `fd-lock` | Concurrent access to global store by multiple `aipm` processes. |
| Progress bars | `indicatif` | Download and extraction progress reporting. |

---

### 5. Cross-Cutting Concerns

#### 5.1 Filesystem Abstraction (`fs.rs`)

The `Fs` trait has 5 methods: `exists`, `create_dir_all`, `write_file`, `read_to_string`, `read_dir`. This is sufficient for manifest operations but **missing** operations needed by install/link:

- Symlink/junction creation and removal
- Hard link creation
- File/directory removal
- File copy (for cross-volume fallback)
- Rename/move (for atomic lockfile writes)
- File metadata (permissions, timestamps)
- SHA-512 hashing

The trait is `Send + Sync`, enabling parallel use.

#### 5.2 Gitignore Management

**Current state**: Workspace init creates `.ai/.gitignore` with managed-section markers (`=== aipm managed start/end ===`) but there is no code to read, parse, or update these markers.

**Required for install**: Create/update `<plugins_dir>/.gitignore` when installing registry packages. Must handle:
- First-time creation with managed header
- Appending new entries between markers
- Removing entries on uninstall
- Preserving manual user entries
- Scoped package entries (e.g., `@company/review-plugin`)

#### 5.3 Manifest Modification

**Problem**: All manifest types derive only `Deserialize`. The `init` and `workspace_init` modules generate TOML as format strings. `aipm install <pkg>` needs to add `[dependencies]` entries to existing manifests without destroying comments or formatting.

**Options**:
1. Add `Serialize` derives and round-trip through `toml::to_string_pretty()` — loses comments and formatting
2. Use `toml_edit` crate for comment-preserving edits — preferred approach

#### 5.4 Manifest Types Ready for Resolver

`DependencySpec` enum already handles both simple (`"^1.0"`) and detailed (`{ version = "^1.0", optional = true }`) dependency declarations. `DetailedDependency` supports `workspace` protocol, `optional` flag, `default_features`, and `features` — all needed by the resolver. The `Workspace` struct has `members`, `plugins_dir`, and shared `dependencies` catalog.

The `Manifest` type has `overrides`, `catalog`, `catalogs`, and `features` fields — all used during resolution.

#### 5.5 Version Utilities Ready for Resolver

`version::Requirement::select_best()` selects the highest non-prerelease matching version from a candidate list. `Requirement::matches()` checks if a version satisfies a requirement. These are the core building blocks for the backtracking solver. `Version` implements `Ord` for sorting.

#### 5.6 CLI Pattern for New Commands

Both CLIs follow a consistent pattern: `Cli` struct with `#[command(subcommand)]`, `Commands` enum with `#[derive(Subcommand)]`, and a `run() -> Result<(), Box<dyn Error>>` function dispatching to `libaipm` functions. New commands (`install`, `update`, `link`, `unlink`, `list`) should follow this same pattern.

---

### 6. Implementation Component Architecture

Based on the spec and existing code, here is the module structure for the new components:

```
crates/libaipm/src/
  resolver/
    mod.rs              — Public API: resolve(manifest, lockfile?, registry) -> Resolution
    graph.rs            — Dependency graph construction and traversal
    solver.rs           — Backtracking constraint solver
    override.rs         — Override application logic
    error.rs            — Conflict reporting
  lockfile/
    mod.rs              — Public API: read, write, validate, reconcile
    types.rs            — LockedPackage, LockfileMetadata, Lockfile
    diff.rs             — Minimal reconciliation: detect added/removed/changed deps
  store/
    mod.rs              — Public API: store_file, retrieve, has_content
    hash.rs             — SHA-512 content hashing
    layout.rs           — Global store directory layout (2-char prefix)
  installer/
    mod.rs              — Public API: install(manifest, options) -> InstallResult
    fetch.rs            — Download .aipm archives from registry
    extract.rs          — Decompress + extract archives to store
    pipeline.rs         — Orchestrate resolve → fetch → store → link → lockfile
  linker/
    mod.rs              — Public API: link_package, unlink_package, list_linked
    directory_link.rs   — Symlink (Unix) / junction (Windows) creation
    hard_link.rs        — Hard link files from store to .aipm/links/
    gitignore.rs        — Managed gitignore section read/write
    link_state.rs       — Persistent link override tracking (.aipm/link-state.toml)
  registry/
    mod.rs              — Public API: search, get_metadata, download
    client.rs           — HTTP client wrapping reqwest
    types.rs            — PackageMetadata, VersionInfo, RegistryConfig
```

---

## Code References

### Existing Implementation
- `crates/libaipm/src/lib.rs` — Module declarations (6 public modules)
- `crates/libaipm/src/manifest/types.rs` — Full manifest schema types including `DependencySpec`, `DetailedDependency`
- `crates/libaipm/src/manifest/validate.rs` — Validation including dependency version requirement checking
- `crates/libaipm/src/version.rs` — `Version` and `Requirement` wrappers with `select_best()` and `matches()`
- `crates/libaipm/src/fs.rs` — `Fs` trait (5 methods) and `Real` implementation
- `crates/libaipm/src/workspace_init/mod.rs:203-211` — Gitignore marker creation (write-only, no update logic)
- `crates/aipm/src/main.rs:21-77` — Consumer CLI `Commands` enum (only `Init` and `Migrate` implemented)
- `crates/aipm-pack/src/main.rs:23-42` — Author CLI `Commands` enum (only `Init` implemented)

### BDD Feature Files
- `tests/features/registry/install.feature` — 23 scenarios covering full install flow
- `tests/features/dependencies/lockfile.feature` — 12 scenarios covering lockfile lifecycle
- `tests/features/dependencies/resolution.feature` — 13 scenarios covering resolver behavior
- `tests/features/registry/link.feature` — 10 scenarios covering link/unlink/list
- `tests/features/registry/local-and-registry.feature` — 19 scenarios covering coexistence and gitignore
- `tests/features/dependencies/features.feature` — 6 scenarios (P1) covering optional features

### Spec
- `specs/2026-03-09-aipm-technical-design.md` — Sections 5.3-5.9 define all algorithms and formats

### Workspace Dependencies
- `Cargo.toml:37` — `semver` (active)
- `Cargo.toml:43` — `sha2` (declared, unused)
- `Cargo.toml:49-50` — `flate2` + `tar` (declared, unused)
- `Cargo.toml:53` — `junction` (declared, unused)
- `Cargo.toml:40` — `reqwest` (in `aipm-pack` only)

---

## Architecture Documentation

### Existing Patterns

1. **Filesystem abstraction**: All I/O goes through the `Fs` trait for testability. New modules should follow this.
2. **Error aggregation**: `manifest::Error::Multiple(Vec<Error>)` collects multiple validation errors. The resolver should use a similar pattern for conflict reporting.
3. **Parallel processing**: `rayon::par_iter()` is used in the migrate module for parallel detection/emission. The installer should use this for parallel fetch/extract/link.
4. **Tool adaptor pattern**: `workspace_init` uses a `ToolAdaptor` trait for extensibility. Could be extended for link operations.
5. **BDD-first**: 83 BDD scenarios exist before implementation. Step definitions in `crates/libaipm/tests/bdd.rs` should be extended for install/update/link.
6. **No `println!`/`unwrap`/`panic`**: Strict lint rules in `Cargo.toml` enforce `writeln!(stdout, ...)`, proper error handling with `?` and `Result`, and no unsafe code.

### Confirmed Design Decisions (from research docs)

| Decision | Source | Rationale |
|----------|--------|-----------|
| Content-addressable store (pnpm model) | `research/docs/2026-03-09-pnpm-core-principles.md` | 70-80% disk savings, 4x faster clean install |
| Backtracking resolver (Cargo model) | `research/docs/2026-03-09-cargo-core-principles.md` | Proven at scale, handles real-world conflicts |
| Lockfile never upgrades on `install` | `research/docs/2026-03-09-cargo-core-principles.md` | Enterprise-grade determinism |
| SHA-512 integrity hashes | `research/docs/2026-03-09-npm-core-principles.md` | Supply chain security |
| Junctions on Windows | `research/docs/2026-03-09-pnpm-core-principles.md` | No elevation required |
| No peer dependencies | `specs/2026-03-09-aipm-technical-design.md` | Eliminates dependency hell; AI plugins are config/markdown |
| TOML lockfile format | `specs/2026-03-09-aipm-technical-design.md` | Consistency with manifest, human-readable diffs |

---

## Historical Context (from research/)

- `research/docs/2026-03-09-pnpm-core-principles.md` — Content-addressable store architecture, hard linking, strict isolation, junction strategy on Windows
- `research/docs/2026-03-09-cargo-core-principles.md` — Backtracking resolver algorithm, lockfile behavior (install never upgrades), version unification within major
- `research/docs/2026-03-09-npm-core-principles.md` — Registry API model, SRI integrity hashes, lockfile design, install flow
- `research/docs/2026-03-09-manifest-format-comparison.md` — TOML chosen for comments, AI-safe generation, no Norway problem
- `research/docs/2026-03-10-microsoft-apm-analysis.md` — 16-point gap analysis of microsoft/apm that AIPM addresses
- `research/docs/2026-03-09-aipm-cucumber-feature-spec.md` — Feature inventory, architecture decisions, design principles

---

## Related Research

- `specs/2026-03-09-aipm-technical-design.md` — Primary technical design document
- `research/docs/2026-03-09-aipm-cucumber-feature-spec.md` — Feature file inventory and synthesis
- `research/docs/2026-03-16-aipm-init-workspace-marketplace.md` — Workspace init implementation details

---

## Open Questions

1. **Cross-volume hard links**: What happens when `~/.aipm/store/` is on a different volume than the project? Need a fallback strategy (copy? warn? configurable?).
2. **Concurrent store access**: If multiple `aipm` processes install simultaneously, file locking is needed. No `fs2` or `fd-lock` crate is declared. Consider adding one.
3. **Link state persistence**: Where exactly is `aipm link` state stored? Likely `.aipm/link-state.toml` but not explicitly specified in the design doc.
4. **`reqwest` placement**: Should the registry client live in `libaipm` (shared) or `aipm` (consumer-only)? The consumer needs it for `install`; the author needs it for `publish`. `libaipm` is the natural home.
5. **Async vs blocking HTTP**: `reqwest` 0.12 defaults to async. Either add `tokio` or enable the `blocking` feature. The project currently has no async runtime.
6. **TOML-preserving manifest edits**: `aipm install <pkg>` must add dependencies without destroying comments. Need `toml_edit` or similar.
7. **Backtracking solver**: Custom implementation vs. `pubgrub` crate. The spec says "inspired by pubgrub" but neither is declared as a dependency.
