# Cargo Core Architectural Principles

> Research date: 2026-03-09
> Scope: Design decisions and principles behind Rust's package manager

---

## 1. Registry Model

### How crates.io Works

crates.io is Cargo's default, centralized package registry. It serves as a **permanent, immutable archive** of Rust crates. The registry consists of two core components:

- **Index**: A structured metadata store that Cargo queries to discover crates and resolve dependencies. The index contains one file per crate, with each line being a JSON object describing a published version.
- **Download endpoint**: Serves `.crate` files (compressed source archives) identified by name, version, and SHA256 checksum.

### Registry Protocols

Cargo supports two remote protocols:
- **Git protocol**: Clones the entire index repository
- **Sparse protocol** (prefix `sparse+`): Fetches individual crate files over HTTP with standard caching

### Crate Naming Conventions

- ASCII alphanumeric characters, `-`, and `_` only
- First character must be alphabetic
- Maximum 64 characters
- Case-insensitive collision detection

### Alternative Registries

- Dependencies must declare which registry they come from
- **crates.io packages cannot depend on alternative registry crates**
- The `publish` field can restrict which registries a crate may be published to

**Sources**:
- [Registries - The Cargo Book](https://doc.rust-lang.org/cargo/reference/registries.html)
- [Registry Index - The Cargo Book](https://doc.rust-lang.org/cargo/reference/registry-index.html)

---

## 2. Versioning

### Semver Enforcement

Cargo adopts Semantic Versioning 2.0.0. The core assumption is that all crates follow semver.

**Pre-1.0 convention**: For `0.x.y` versions, the minor version acts as the "major" boundary.

### Version Requirements Syntax

| Syntax | Example | Resolves to | Design Intent |
|--------|---------|-------------|---------------|
| **Default/Caret** `^` | `"1.2.3"` | `>=1.2.3, <2.0.0` | Maximum flexibility within semver-compatible range |
| **Tilde** `~` | `"~1.2.3"` | `>=1.2.3, <1.3.0` | Conservative: patch-level updates only |
| **Wildcard** `*` | `"1.2.*"` | `>=1.2.0, <1.3.0` | Positional flexibility |
| **Exact** `=` | `"=1.2.3"` | Exactly `1.2.3` | Tightly coupled packages only |

**Sources**:
- [Specifying Dependencies - The Cargo Book](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)

---

## 3. Dependency Resolution

### Algorithm Design

Cargo uses a **backtracking search** with heuristics:
1. Pick the next unresolved dependency
2. Try the **highest compatible version** first
3. Attempt to **unify** with an already-activated version if semver-compatible
4. If a conflict is found, **backtrack** and try the next candidate

### Version Unification

If two packages depend on the same crate with semver-compatible requirements, Cargo builds it **once** at the highest satisfying version. Cargo **does allow** multiple semver-incompatible versions in one graph.

### The `links` Constraint

The `links` field enforces that only **one crate** per native library can exist in a dependency graph.

### Resolver Versions

| Resolver | Default For | Key Behavior |
|----------|-------------|--------------|
| `"1"` | Pre-2021 editions | Union of all features across entire graph |
| `"2"` | Edition 2021+ | Selective feature unification |
| `"3"` | Edition 2024+ | Adds MSRV-aware resolution |

**Sources**:
- [Dependency Resolution - The Cargo Book](https://doc.rust-lang.org/cargo/reference/resolver.html)

---

## 4. Lockfiles

### Purpose of Cargo.lock

`Cargo.lock` captures the **exact resolved state** of every dependency. Its sole purpose is **deterministic, reproducible builds**.

**Design separation**:
- `Cargo.toml` = **intent** (flexible ranges)
- `Cargo.lock` = **reality** (exact versions)

### Workspace Behavior

All packages in a workspace share a **single `Cargo.lock`** at the workspace root.

**Sources**:
- [Cargo.toml vs Cargo.lock - The Cargo Book](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html)

---

## 5. Manifest File (Cargo.toml)

### Structure Overview

- **`[package]`**: `name`, `version`, `edition` (required trio), plus `rust-version`, `description`, `license`, `publish`, `links`
- **`[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`**: Three dependency scopes
- **`[features]`**: Conditional compilation flags
- **`[workspace]`**: Monorepo configuration
- **`[profile.*]`**: Compiler settings per build profile
- **`[patch]`**: Override any dependency in the graph

**Sources**:
- [The Manifest Format - The Cargo Book](https://doc.rust-lang.org/cargo/reference/manifest.html)

---

## 6. Publish Flow

### Immutability Guarantees

- **A published version is permanent**. It can never be overwritten, re-published, or deleted.
- **Version numbers are consumed forever**.

### Yanking vs. Unpublishing

**There is no unpublishing.** Only yanking exists. Yanked versions remain in the archive but are excluded from new resolutions.

**Sources**:
- [Publishing on crates.io - The Cargo Book](https://doc.rust-lang.org/cargo/reference/publishing.html)

---

## 7. Workspaces

### Two Workspace Types

1. **Root package workspace**: Has both `[package]` and `[workspace]` sections
2. **Virtual workspace**: Has only `[workspace]` -- no package of its own

### Dependency Inheritance (`workspace.dependencies`)

Shared dependency versions can be defined centrally and inherited with `workspace = true`.

**Sources**:
- [Workspaces - The Cargo Book](https://doc.rust-lang.org/cargo/reference/workspaces.html)

---

## 8. Build Scripts

Build scripts (`build.rs`) integrate with the non-Rust world. They communicate back to Cargo via `cargo::KEY=VALUE` lines on stdout.

### The `-sys` Convention

- `foo-sys`: Raw FFI bindings, declares `links = "foo"`
- `foo`: Safe Rust API built on top of `foo-sys`

**Sources**:
- [Build Scripts - The Cargo Book](https://doc.rust-lang.org/cargo/reference/build-scripts.html)

---

## 9. Init and Scaffolding

Cargo enforces a standard project layout by convention. No configuration needed for standard layouts. Cargo deliberately does **not** include a built-in template system.

**Sources**:
- [cargo new - The Cargo Book](https://doc.rust-lang.org/cargo/commands/cargo-new.html)

---

## 10. Features System

### Feature Unification

When multiple packages enable different features on the same dependency, Cargo builds it with the **union of all features**. Features **must be additive**.

### Default Features

Removing a feature from `default` is a **semver-breaking change**.

### Optional Dependencies and `dep:` Syntax

The `dep:` prefix gives explicit control over the feature namespace vs dependency namespace.

**Sources**:
- [Features - The Cargo Book](https://doc.rust-lang.org/cargo/reference/features.html)

---

## Cross-Cutting Design Themes

1. **Convention over configuration**: Standard project layout, default caret versioning
2. **Immutability and reproducibility**: Permanent crate archive, lockfiles, deterministic resolution
3. **Additive composition**: Features are always additive, workspaces share a single resolution
4. **Explicit over implicit**: `dep:` syntax, `links` field declarations, registry annotations
5. **Ecosystem-scale thinking**: No unpublishing, semver by default
6. **Declarative configuration**: TOML manifests are intentionally not Turing-complete
