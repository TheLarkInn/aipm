---
date: 2026-03-28 11:09:00 PDT
researcher: Claude Code
git_commit: b434459f5db45d4fb2d9908c3a838ce82839c1d0
branch: main
repository: aipm
topic: "Issue #129: Workspace dependencies do not link yet"
tags: [research, codebase, workspace, dependencies, install, linking, lockfile, resolver]
status: complete
last_updated: 2026-03-28
last_updated_by: Claude Code
---

# Research: Workspace Dependencies Do Not Link Yet (Issue #129)

## Research Question

[GitHub Issue #129](https://github.com/TheLarkInn/aipm/issues/129): When one plugin in a workspace references another via `workspace = "^"` in its `[dependencies]`, the reference should trigger install/update resolution and be recorded in the lock file. Currently this does nothing.

**Example from issue:**

```toml
# plugin-a/aipm.toml
[package]
name = "plugin-a"
version = "0.1.0"
type = "composite"

[dependencies]
plugin-b = { workspace = "^" }
```

```toml
# plugin-b/aipm.toml
[package]
name = "plugin-b"
version = "0.1.0"
type = "composite"
```

## Summary

The workspace dependency protocol (`workspace = "^"`, `"="`, `"*"`) is **parsed and validated** in the manifest layer but is **not resolved** during `aipm install`. The `manifest_to_resolver_deps()` function in the install pipeline ignores the `workspace` field entirely — a workspace dep falls through as `version: "*"` and is sent to the registry-backed solver, where it either fails (no registry entry) or is silently mishandled. No workspace member discovery, local linking, or lockfile recording occurs for these dependencies.

The codebase already has the structural scaffolding needed to support this feature:
1. `DetailedDependency.workspace` field exists and is parsed
2. `resolver::Source::Workspace` enum variant exists
3. Lockfile source encoding `"workspace"` exists and round-trips correctly
4. The `aipm link` command demonstrates the local-directory linking pattern
5. BDD scenarios in `orchestration.feature` specify the expected behavior

What's missing is the **connecting logic**: detecting workspace deps in the install pipeline, discovering workspace member paths, resolving their versions from the workspace root or member manifests, creating `Resolved` entries with `Source::Workspace`, and linking member directories instead of downloading tarballs.

## Detailed Findings

### 1. Manifest Parsing — Workspace Field is Parsed

**File:** [`crates/libaipm/src/manifest/types.rs:89-107`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/manifest/types.rs#L89-L107)

The `DetailedDependency` struct has a `workspace: Option<String>` field that accepts protocol strings `"^"`, `"="`, or `"*"`:

```rust
pub struct DetailedDependency {
    pub version: Option<String>,
    pub workspace: Option<String>,  // <-- parsed but unused during install
    pub optional: Option<bool>,
    pub default_features: Option<bool>,
    pub features: Option<Vec<String>>,
}
```

A manifest entry like `plugin-b = { workspace = "^" }` deserializes into `DependencySpec::Detailed` with `workspace: Some("^")` and `version: None`.

**Test confirming parsing works:** [`crates/libaipm/src/manifest/mod.rs:237-248`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/manifest/mod.rs#L237-L248)

```rust
#[test]
fn workspace_ref_dependency_valid() {
    let toml = r#"
[package]
name = "my-plugin"
version = "0.1.0"

[dependencies]
sibling = { workspace = "^" }
"#;
    let result = parse_and_validate(toml, None);
    assert!(result.is_ok());
}
```

### 2. Manifest Validation — Workspace Deps Skip Version Check

**File:** [`crates/libaipm/src/manifest/validate.rs:140-165`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/manifest/validate.rs#L140-L165)

During validation, workspace deps are detected by `d.workspace.is_some()` and skipped with `continue` (line 150). No version requirement validation is applied to them. The workspace protocol strings themselves (`"^"`, `"="`, `"*"`) are also accepted as valid version requirements by `is_valid_version_req()` at line 55-58 (for `[workspace.dependencies]` entries).

### 3. Install Pipeline — Workspace Field is Ignored (THE GAP)

**File:** [`crates/libaipm/src/installer/pipeline.rs:216-243`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/installer/pipeline.rs#L216-L243)

The `manifest_to_resolver_deps()` function converts manifest dependencies into resolver `Dependency` structs. For `DependencySpec::Detailed`, it extracts `d.version` (or falls back to `"*"` if `None`) but **never consults `d.workspace`**:

```rust
manifest::types::DependencySpec::Detailed(d) => {
    let version = d.version.clone().unwrap_or_else(|| "*".to_string());
    // d.workspace is NEVER checked here
    let feats = d.features.clone().unwrap_or_default();
    let df = d.default_features.unwrap_or(true);
    (version, feats, df)
},
```

A workspace dep `{ workspace = "^" }` produces `req = "*"` (since `version` is `None`) and goes into the resolver as an ordinary dependency with `source: "root"`. The resolver then tries to find it in the registry, where it either fails or produces incorrect results.

### 4. Resolver — Source::Workspace Exists but is Never Produced

**File:** [`crates/libaipm/src/resolver/mod.rs:56-70`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/resolver/mod.rs#L56-L70)

The `Source` enum has a `Workspace` variant:

```rust
pub enum Source {
    Registry { index_url: String },
    Workspace,                    // <-- exists, ready to use
    Path { path: std::path::PathBuf },
}
```

However, `build_resolution()` at line 441-456 always produces `Source::Registry { index_url: String::new() }` for all resolved packages. The `Source::Workspace` variant is only produced when reconstructing from a lockfile that already contains `"workspace"` source strings.

### 5. Lockfile — Workspace Source Encoding Works

**File:** [`crates/libaipm/src/installer/pipeline.rs:428-460`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/installer/pipeline.rs#L428-L460)

The `build_lockfile()` function correctly converts `Source::Workspace` to the string `"workspace"`:

```rust
resolver::Source::Workspace => "workspace".to_string(),
```

And `build_resolution_from_lockfile()` at line 302-334 correctly parses `"workspace"` back to `Source::Workspace`:

```rust
let source = if pkg.source == "workspace" {
    resolver::Source::Workspace
} else if let Some(path) = pkg.source.strip_prefix("path+") {
    // ...
```

**Tests confirming round-trip:** [`crates/libaipm/src/installer/pipeline.rs:823-855`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/installer/pipeline.rs#L823-L855) and [`crates/libaipm/src/lockfile/types.rs:187-214`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/lockfile/types.rs#L187-L214)

### 6. Install Loop — Workspace Packages Would Be Treated as Registry Downloads

**File:** [`crates/libaipm/src/installer/pipeline.rs:139-180`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/installer/pipeline.rs#L139-L180)

The fetch-store-link loop (lines 139-180) unconditionally calls `registry.download()` for every resolved package. There is no check for `resolved.source` — workspace or path deps would attempt a registry download and fail. The loop needs a branch that handles `Source::Workspace` by linking the local member directory instead of downloading a tarball.

### 7. Workspace Manifest Structure — Dependencies Catalog Exists

**File:** [`crates/libaipm/src/manifest/types.rs:65-76`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/manifest/types.rs#L65-L76)

The `Workspace` struct supports `[workspace.dependencies]` for shared dependency catalogs:

```rust
pub struct Workspace {
    pub members: Vec<String>,
    pub plugins_dir: Option<String>,
    pub dependencies: Option<BTreeMap<String, DependencySpec>>,
}
```

The workspace init scaffolding at [`crates/libaipm/src/workspace_init/mod.rs:166-185`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/workspace_init/mod.rs#L166-L185) generates comments referencing the workspace protocol:

```
# Members reference these via: dep = { workspace = "^" }
# [workspace.dependencies]
```

### 8. `aipm link` Command — Existing Local Linking Pattern

**File:** [`crates/aipm/src/main.rs:341-384`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/aipm/src/main.rs#L341-L384)

The existing `aipm link` command provides the exact pattern workspace deps need to follow:
1. Validate the target path has an `aipm.toml`
2. Read the package name from the manifest
3. Create a directory link (symlink/junction) from the local path into the plugins dir
4. Record the link in `.aipm/links.toml`

This is the closest analog to what workspace dep resolution should do — except it would be triggered automatically during `aipm install` rather than requiring a manual `aipm link` command.

### 9. BDD Scenarios — Expected Behavior is Specified

**File:** [`tests/features/monorepo/orchestration.feature:104-134`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/tests/features/monorepo/orchestration.feature#L104-L134)

The feature file specifies the workspace protocol behavior under `Rule: Workspace protocol for inter-package references`:

| Scenario | Protocol | Expected Behavior |
|----------|----------|-------------------|
| Reference a workspace sibling | `{ workspace = "^" }` | Links to local package, no registry lookup |
| Replaced on publish | `^` | Published as `"^2.3.0"` (caret + member version) |
| Exact version on publish | `=` | Published as `"=2.3.0"` |
| Wildcard on publish | `*` | Published as `"2.3.0"` (bare version) |

Key scenario (lines 106-115):
```gherkin
Scenario: Reference a workspace sibling with workspace protocol
  Given a workspace with members "core" and "cli"
  And "cli" manifest declares:
    """toml
    [dependencies]
    core = { workspace = "^" }
    """
  When the user runs "aipm install" from the workspace root
  Then "cli" links to the local "core" package
  And no registry lookup is performed for "core"
```

### 10. Workspace Member Discovery — Not Yet Implemented

The `Workspace.members` field contains glob patterns (e.g., `[".ai/*"]`) that define which directories are workspace members. The install pipeline does not currently:
- Read the workspace root manifest to find member globs
- Discover member directories by expanding those globs
- Build a map of member name -> member path
- Use that map when resolving workspace dependencies

The `workspace_init` module ([`crates/libaipm/src/workspace_init/`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/workspace_init/mod.rs)) handles workspace scaffolding but does not provide member discovery utilities.

## Code References

| File | Lines | Description |
|------|-------|-------------|
| [`crates/libaipm/src/manifest/types.rs`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/manifest/types.rs#L89-L107) | 89-107 | `DetailedDependency` with `workspace` field |
| [`crates/libaipm/src/manifest/types.rs`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/manifest/types.rs#L65-L76) | 65-76 | `Workspace` struct with `members` and `dependencies` |
| [`crates/libaipm/src/manifest/validate.rs`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/manifest/validate.rs#L148-L151) | 148-151 | Workspace deps skip validation |
| [`crates/libaipm/src/installer/pipeline.rs`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/installer/pipeline.rs#L216-L243) | 216-243 | `manifest_to_resolver_deps()` — ignores workspace field |
| [`crates/libaipm/src/installer/pipeline.rs`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/installer/pipeline.rs#L139-L180) | 139-180 | Fetch-store-link loop — no workspace branch |
| [`crates/libaipm/src/installer/pipeline.rs`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/installer/pipeline.rs#L428-L460) | 428-460 | `build_lockfile()` — handles `Source::Workspace` |
| [`crates/libaipm/src/installer/pipeline.rs`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/installer/pipeline.rs#L301-L334) | 301-334 | `build_resolution_from_lockfile()` — parses `"workspace"` |
| [`crates/libaipm/src/resolver/mod.rs`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/resolver/mod.rs#L56-L70) | 56-70 | `Source::Workspace` enum variant |
| [`crates/libaipm/src/resolver/mod.rs`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/resolver/mod.rs#L441-L456) | 441-456 | `build_resolution()` — always produces `Source::Registry` |
| [`crates/libaipm/src/lockfile/types.rs`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/lockfile/types.rs#L36-L53) | 36-53 | Lockfile `Package` with source string encoding |
| [`crates/aipm/src/main.rs`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/aipm/src/main.rs#L341-L384) | 341-384 | `cmd_link()` — local directory linking pattern |
| [`crates/libaipm/src/workspace_init/mod.rs`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/crates/libaipm/src/workspace_init/mod.rs#L166-L185) | 166-185 | Workspace manifest template with protocol comment |
| [`tests/features/monorepo/orchestration.feature`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/tests/features/monorepo/orchestration.feature#L104-L134) | 104-134 | BDD scenarios for workspace protocol |

## Architecture Documentation

### Current Install Pipeline Flow

```
aipm install
  └─> load aipm.toml (manifest::parse_and_validate)
  └─> load aipm.lock (lockfile::read)
  └─> manifest_to_resolver_deps()     ← workspace deps become req="*", source="root"
  └─> resolve_dependencies()
        └─> lockfile::reconcile::reconcile()
        └─> resolver::resolve_with_overrides()  ← queries registry for all deps
  └─> for each resolved package:
        └─> registry.download()        ← fails for workspace deps (no registry entry)
        └─> store_tarball_contents()
        └─> linker::pipeline::link_package()
  └─> build_lockfile()
  └─> lockfile::write()
```

### What Needs to Change (Structural Observations)

The pipeline processes all dependencies uniformly through the registry. For workspace deps to work, the pipeline needs to:

1. **Detect** workspace deps in `manifest_to_resolver_deps()` by checking `d.workspace.is_some()`
2. **Discover** workspace members by reading the workspace root's `[workspace].members` globs and loading each member's `aipm.toml` to build a name-to-path map
3. **Resolve** workspace deps outside the registry solver — look up the member's version from its `[package].version`, apply the workspace protocol (`^` = caret range, `=` = exact, `*` = wildcard)
4. **Produce** `Resolved` entries with `Source::Workspace` and the member's actual version
5. **Link** workspace deps via directory linking (like `aipm link`) instead of tarball download
6. **Record** workspace deps in the lockfile with `source = "workspace"`

### Three Workspace Protocol Modes

| Protocol | Meaning | Install Behavior | Publish Replacement |
|----------|---------|------------------|---------------------|
| `^` | Caret-compatible | Link to local, use member version | `"^{version}"` |
| `=` | Exact match | Link to local, use member version | `"={version}"` |
| `*` | Any version | Link to local, use member version | `"{version}"` |

All three link to the local member at install time. The protocol only affects what version requirement is used when publishing.

## Historical Context (from research/)

- [`specs/2026-03-26-install-update-link-lockfile.md`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/specs/2026-03-26-install-update-link-lockfile.md) — The primary design spec for the install pipeline. Lists workspace filtering and monorepo orchestration as **non-goals** for the initial implementation, but the workspace protocol itself is part of the manifest schema.
- [`specs/2026-03-09-aipm-technical-design.md`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/specs/2026-03-09-aipm-technical-design.md) — The foundational technical design doc. Lists workspace protocol (`workspace:^`) as a dependency resolution feature.
- [`specs/2026-03-16-aipm-init-workspace-marketplace.md`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/specs/2026-03-16-aipm-init-workspace-marketplace.md) — Workspace scaffolding spec, covers workspace init and member structure.
- [`research/docs/2026-03-26-install-update-link-lockfile-implementation.md`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/research/docs/2026-03-26-install-update-link-lockfile-implementation.md) — Implementation readiness analysis for the install pipeline.
- [`research/docs/2026-03-09-pnpm-core-principles.md`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/research/docs/2026-03-09-pnpm-core-principles.md) — pnpm workspace protocol is the direct inspiration for AIPM's `workspace = "^"` syntax.
- [`research/docs/2026-03-09-cargo-core-principles.md`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/research/docs/2026-03-09-cargo-core-principles.md) — Cargo workspace dependency inheritance model for reference.

## Related Research

- [`research/docs/2026-03-09-npm-core-principles.md`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/research/docs/2026-03-09-npm-core-principles.md) — npm workspace linking patterns
- [`research/docs/2026-03-16-aipm-init-workspace-marketplace.md`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/research/docs/2026-03-16-aipm-init-workspace-marketplace.md) — Workspace and marketplace scaffolding research
- [`research/docs/2026-03-23-recursive-claude-discovery-parallel-migrate.md`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/research/docs/2026-03-23-recursive-claude-discovery-parallel-migrate.md) — Recursive folder discovery patterns (relevant to workspace member discovery)
- [`research/docs/2026-03-09-aipm-cucumber-feature-spec.md`](https://github.com/TheLarkInn/aipm/blob/b434459f5db45d4fb2d9908c3a838ce82839c1d0/research/docs/2026-03-09-aipm-cucumber-feature-spec.md) — BDD feature specifications covering install and dependency scenarios

## Open Questions

1. **Workspace root discovery**: When `aipm install` runs from a member directory, should it walk up to find the workspace root `aipm.toml`? Or must it always be invoked from the workspace root?
2. **Workspace member discovery**: The `[workspace].members` field uses glob patterns. Should member discovery use the `glob` crate, or a simpler directory-walk approach?
3. **Transitive workspace deps**: If workspace member A depends on workspace member B, and B depends on workspace member C, should all three be resolved as workspace deps? (The BDD scenarios imply yes.)
4. **Workspace deps in lockfile reconciliation**: How should `lockfile::reconcile::reconcile()` handle workspace deps? Should workspace packages be carried forward like registry packages, or always re-resolved (since local source could have changed)?
5. **Version consistency**: Should `aipm install` validate that the workspace protocol matches? E.g., if `plugin-a` uses `{ workspace = "^" }` to reference `plugin-b` at `1.0.0`, should it warn if `plugin-b` is at `2.0.0` and `plugin-a` was authored against `1.x`?
6. **Interaction with `aipm link`**: If a package is both a workspace dep and has a manual `aipm link` override, which takes priority?
