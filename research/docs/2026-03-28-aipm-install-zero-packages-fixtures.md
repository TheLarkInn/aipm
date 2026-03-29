---
date: 2026-03-28 17:32:05 PDT
researcher: Claude Opus 4.6
git_commit: 3bbd04df2c33c3aeeabcfebc23382d635a10b118
branch: main
repository: aipm
topic: "Why does aipm install resolve 0 packages in the fixtures directory?"
tags: [research, codebase, install, workspace, dependencies, fixtures]
status: complete
last_updated: 2026-03-28
last_updated_by: Claude Opus 4.6
---

# Research: Why `aipm install` Resolves 0 Packages in Fixtures

## Research Question

When running `aipm install` from `Q:/aipm/fixtures/`, the output is "Installed 0 package(s), 0 up-to-date, 0 removed" despite the workspace having two local plugins (`get-current-time` and `print-clock`) where `print-clock` depends on `get-current-time` via `workspace = "*"`.

## Summary

**Root cause: The fixtures `aipm.toml` has no `[dependencies]` section.** The install pipeline exclusively reads the root manifest's `[dependencies]` table to determine what to install. Since `[dependencies]` is commented out in `fixtures/aipm.toml`, the pipeline sees zero dependencies, resolves zero packages, and correctly reports "Installed 0 package(s)".

The `[workspace].members` glob discovers the plugins, and `print-clock`'s `[dependencies]` declares `get-current-time = { workspace = "*" }`, but these member inter-dependencies are only resolved **transitively** — they are only discovered after at least one workspace dependency appears in the root `[dependencies]`.

## Detailed Findings

### The Install Pipeline Data Flow

The install command flows through these steps (all in [`pipeline.rs`](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/crates/libaipm/src/installer/pipeline.rs)):

1. **Load manifest** (line 79): Reads `aipm.toml` from the current directory
2. **Extract dependency names** (line 104): `extract_dep_names()` reads `manifest.dependencies` — the root `[dependencies]` table
3. **Discover workspace members** (line 127): `discover_workspace_members()` expands `[workspace].members` globs
4. **Split dependencies** (line 139): `split_dependencies()` partitions root `[dependencies]` into workspace vs. registry
5. **Resolve workspace deps** (lines 142-146): Only runs if `workspace_dep_names` is non-empty
6. **Collect transitive registry deps** (lines 149-150): Scans resolved workspace members for non-workspace deps
7. **Resolve registry deps** (lines 153-160): Runs the constraint solver for registry packages
8. **Merge and link** (lines 163-175): Creates directory symlinks for workspace packages, downloads for registry

### Where It Short-Circuits

At step 2, `extract_dep_names()` ([line 286-288](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/crates/libaipm/src/installer/pipeline.rs#L286-L288)) returns an empty `BTreeSet` because `manifest.dependencies` is `None`:

```rust
fn extract_dep_names(manifest: &manifest::types::Manifest) -> BTreeSet<String> {
    manifest.dependencies.as_ref().map(|deps| deps.keys().cloned().collect()).unwrap_or_default()
}
```

At step 4, `split_dependencies()` ([line 322-359](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/crates/libaipm/src/installer/pipeline.rs#L322-L359)) hits the early return on line 325-327:

```rust
let Some(ref deps) = manifest.dependencies else {
    return (Vec::new(), Vec::new());
};
```

Both workspace and registry dep lists are empty. At step 5, `workspace_dep_names.is_empty()` is true (line 142), so workspace resolution is skipped entirely. Steps 6 and 7 receive empty inputs and produce empty results.

Result: `InstallResult { installed: 0, up_to_date: 0, removed: 0 }`.

### The Fixtures Configuration

**Root manifest** (`fixtures/aipm.toml`):
```toml
[workspace]
members = [".ai/*"]
plugins_dir = ".ai"

# [workspace.dependencies]    <-- commented out
# [dependencies]              <-- commented out
```

**Plugin manifests:**
- `get-current-time/aipm.toml`: `[package]` only, no dependencies
- `print-clock/aipm.toml`: `[dependencies]` with `get-current-time = { workspace = "*" }`

### How Workspace Dependencies Are Designed to Work

The intended flow requires the root `[dependencies]` to declare workspace packages:

1. Root `aipm.toml` declares `print-clock = { workspace = "*" }` in `[dependencies]`
2. `split_dependencies()` classifies `print-clock` as a workspace dep name
3. `resolve_workspace_deps()` looks up `print-clock` in discovered members
4. It reads `print-clock`'s `[dependencies]` and finds `get-current-time = { workspace = "*" }`
5. `get-current-time` is enqueued and resolved transitively
6. Both packages end up as `Resolved` with `Source::Workspace`
7. Linking creates directory symlinks from member paths into `plugins_dir`

### What `[workspace.dependencies]` Does (and Doesn't Do)

The `[workspace.dependencies]` section is parsed into `Workspace.dependencies` ([`types.rs:75`](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/crates/libaipm/src/manifest/types.rs#L75)) and validated ([`validate.rs:89-94`](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/crates/libaipm/src/manifest/validate.rs#L89-L94)), but **it is never consumed by the install pipeline**. It serves as a version catalog — a mechanism for workspace members to inherit version constraints via `workspace = "*"` — but the install pipeline does not read it to determine what to install.

### The Fix

To make `aipm install` resolve packages, uncomment and populate the `[dependencies]` section in `fixtures/aipm.toml`:

```toml
[dependencies]
print-clock = { workspace = "*" }
```

This is sufficient because `print-clock`'s own `[dependencies]` declares `get-current-time = { workspace = "*" }`, which will be resolved transitively. Alternatively, to be explicit:

```toml
[dependencies]
get-current-time = { workspace = "*" }
print-clock = { workspace = "*" }
```

## Code References

- [`crates/aipm/src/main.rs:282-321`](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/crates/aipm/src/main.rs#L282-L321) — `cmd_install()` CLI handler
- [`crates/aipm/src/main.rs:315-319`](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/crates/aipm/src/main.rs#L315-L319) — "Installed X package(s)" output
- [`crates/libaipm/src/installer/pipeline.rs:76-203`](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/crates/libaipm/src/installer/pipeline.rs#L76-L203) — `install()` orchestration
- [`crates/libaipm/src/installer/pipeline.rs:286-288`](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/crates/libaipm/src/installer/pipeline.rs#L286-L288) — `extract_dep_names()` reads only root `[dependencies]`
- [`crates/libaipm/src/installer/pipeline.rs:322-359`](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/crates/libaipm/src/installer/pipeline.rs#L322-L359) — `split_dependencies()` early return on empty deps
- [`crates/libaipm/src/installer/pipeline.rs:368-480`](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/crates/libaipm/src/installer/pipeline.rs#L368-L480) — `resolve_workspace_deps()` BFS transitive resolution
- [`crates/libaipm/src/workspace/mod.rs:63-126`](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/crates/libaipm/src/workspace/mod.rs#L63-L126) — `discover_members()` glob expansion
- [`crates/libaipm/src/manifest/types.rs:67-107`](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/crates/libaipm/src/manifest/types.rs#L67-L107) — `Workspace`, `DependencySpec`, `DetailedDependency` structs

## Architecture Documentation

The install pipeline follows a strict "root-driven" model similar to npm/pnpm workspaces:
- Only root `[dependencies]` drives installation
- Workspace members are discovered via `[workspace].members` globs but only installed when referenced
- Transitive workspace dependencies are resolved via BFS from the root dependency set
- The `[workspace.dependencies]` catalog exists for version inheritance, not for install enumeration
- Member inter-dependencies are invisible to the installer unless at least one member appears in root `[dependencies]`

## Historical Context (from research/)

- [`research/docs/2026-03-26-install-update-link-lockfile-implementation.md`](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/research/docs/2026-03-26-install-update-link-lockfile-implementation.md) — Implementation research for the install/update/link/lockfile feature
- [`research/tickets/2026-03-28-129-workspace-dependencies-linking.md`](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/research/tickets/2026-03-28-129-workspace-dependencies-linking.md) — Ticket tracking workspace dependency resolution and linking

## Related Research

- [`specs/2026-03-26-install-update-link-lockfile.md`](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/specs/2026-03-26-install-update-link-lockfile.md) — Primary design spec for install/update/link/lockfile
- [`specs/2026-03-28-workspace-dependency-linking.md`](https://github.com/TheLarkInn/aipm/blob/3bbd04df2c33c3aeeabcfebc23382d635a10b118/specs/2026-03-28-workspace-dependency-linking.md) — Workspace dependency resolution and linking spec

## Open Questions

1. **Should `aipm install` auto-install all workspace members?** The current design requires explicit root `[dependencies]` entries. An alternative would be to treat all `[workspace].members` as implicit dependencies (similar to `npm install` in a workspace root installing all workspace packages). This is a design decision, not a bug.

2. **Is `[workspace.dependencies]` intended for future use?** It is parsed and validated but never consumed. If it's meant to serve as a shared version catalog for registry deps inherited by members, that path is not yet wired into the install pipeline.
