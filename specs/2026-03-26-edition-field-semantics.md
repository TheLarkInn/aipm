# Remove `edition` Field from `aipm.toml`

| Document Metadata      | Details                                                                        |
| ---------------------- | ------------------------------------------------------------------------------ |
| Author(s)              | selarkin                                                                       |
| Status                 | Draft (WIP)                                                                    |
| Team / Owner           | AI Dev Tooling                                                                 |
| Created / Last Updated | 2026-03-26                                                                     |
| Research               | [research/docs/2026-03-26-edition-field-purpose-and-rationale.md](../research/docs/2026-03-26-edition-field-purpose-and-rationale.md) |

## 1. Executive Summary

The `edition` field in `aipm.toml` plugin manifests was borrowed from Rust's Cargo package manager where it controls compiler behavior across language versions. AI plugins have no runtime, no compiler, and no language versioning — the field serves no purpose and never will. This spec removes `edition` from the manifest schema, all five generation paths, the scaffold script, and all tests. This is a breaking removal — existing manifests containing `edition` will fail to parse. Since aipm is pre-1.0 (semver pre-release), breaking changes are expected and no backward compatibility is provided.

## 2. Context and Motivation

### 2.1 Current State

The `edition` field exists across the codebase with zero functional purpose ([edition research](../research/docs/2026-03-26-edition-field-purpose-and-rationale.md)):

- **Schema**: `Option<String>` in `Package` struct ([types.rs:60-61](../crates/libaipm/src/manifest/types.rs))
- **Validation**: None — `validate.rs` has zero references to `edition`
- **Generation**: Hardcoded to `"2024"` in all five manifest generation paths
- **Runtime behavior**: Zero. No code reads, branches on, or acts on the edition value
- **Origin**: Not in the [original technical design](../specs/2026-03-09-aipm-technical-design.md) (2026-03-09). Added one week later by copying from Cargo's `[package]` schema without evaluating whether it applies to AI plugins

### 2.2 The Problem

| Problem | Impact |
|---------|--------|
| **No purpose** | Cargo editions gate compiler behavior across Rust language versions (2015, 2018, 2021, 2024). AI plugins are markdown, JSON, and config files — there is no compiler, no language versioning, and no runtime that editions could control |
| **User confusion** | The [suppress-manifest spec](../specs/2026-03-24-suppress-plugin-manifest-generation.md) identifies `edition` as one of the fields that "suggest a package management system" that doesn't exist, causing users to think they need to set or understand it |
| **Cargo cargo-culting** | The field was adopted from Cargo's schema pattern without evaluating whether the concept maps to AI plugins. Cargo editions exist because Rust is a compiled language with backward-compatibility guarantees across language versions — none of this applies to aipm |
| **Schema pollution** | Every generated manifest includes a meaningless `edition = "2024"` line that adds noise without information |

### 2.3 Why Not Keep It for the Future?

The [edition research](../research/docs/2026-03-26-edition-field-purpose-and-rationale.md) explored whether edition could serve a future purpose (resolver behavior, validation strictness, feature unification). The conclusion: **none of these require an edition field**.

- **Resolver behavior changes** can be controlled by a `resolver` field (as Cargo itself does separately from `edition` — see `resolver = "2"` in workspace manifests)
- **Validation strictness** is a toolchain concern, not a per-manifest concern — new `aipm` versions can tighten validation globally
- **Feature unification** is a resolver algorithm choice, not a per-plugin epoch

If a versioning epoch is ever genuinely needed for AI plugins, it can be added at that time with a concrete purpose. Reserving a field "just in case" adds confusion now for speculative future value.

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [ ] Remove the `edition` field from the `Package` struct in `types.rs`
- [ ] Remove `edition = "2024"` from all five manifest generation paths (init, workspace_init, migrate emitter x2, scaffold script)
- [ ] Remove `edition` from the migrate emitter's `PluginPackage` serialization struct
- [ ] Remove all tests that assert edition presence
- [ ] Update BDD feature files to remove edition steps
- [ ] Existing manifests containing `edition` will fail to parse — this is intentional (pre-1.0 breaking change)
- [ ] All four quality gates pass: `cargo build`, `cargo test`, `cargo clippy`, `cargo fmt`
- [ ] Coverage remains at or above 89% branch threshold

### 3.2 Non-Goals (Out of Scope)

- [ ] We will NOT scan existing `.ai/` directories to remove `edition` from previously-generated manifests — users must remove the field manually if they encounter parse errors
- [ ] We will NOT add a deprecation period or silent ignore — the field is gone, manifests that include it will fail
- [ ] We will NOT modify workspace root manifests (they never had `edition`)
- [ ] We will NOT introduce a replacement field (no `schema_version`, no `resolver`, nothing)

## 4. Proposed Solution (High-Level Design)

### 4.1 Removal Strategy

Hard removal — no backward compatibility shims:

1. **Schema**: Remove `edition` from the `Package` struct. Manifests containing `edition` will fail to parse — this is the desired behavior. The `Manifest` struct's `#[serde(deny_unknown_fields)]` is on `Manifest` only, not on `Package`. Since `Package` does not have `deny_unknown_fields`, serde's default behavior for unknown fields in `Package` applies (silently ignored). If we want a **hard break**, we add `#[serde(deny_unknown_fields)]` to `Package` as well, so any unknown field (including `edition`) causes an explicit parse error.

2. **Generation**: Remove `edition = "2024"` from every template string and serialization struct.

### 4.2 Change Summary

```
BEFORE:
  types.rs            → Package { ..., edition: Option<String>, ... }
  init.rs             → edition = "2024"\n  in format!() template
  workspace_init.rs   → edition = "2024"\n  in starter manifest + scaffold script
  migrate/emitter.rs  → edition: "2024".to_string() in PluginPackage struct (x2)
  scaffold-plugin.ts  → edition = "2024" in TOML template string
  tests               → 3+ tests assert edition presence

AFTER:
  types.rs            → Package { ... } (no edition field, deny_unknown_fields added)
  init.rs             → no edition line in template
  workspace_init.rs   → no edition line in starter manifest or scaffold script
  migrate/emitter.rs  → no edition field in PluginPackage struct
  scaffold-plugin.ts  → no edition in TOML template string
  tests               → edition assertions removed
  Package             → rejects unknown fields (including edition) — breaking change
```

## 5. Detailed Design

### 5.1 Schema Change — `types.rs`

Two changes to `Package`:

1. Remove the `edition` field and its doc comment:

```rust
// REMOVE these two lines:
/// Edition identifier.
pub edition: Option<String>,
```

2. Add `#[serde(deny_unknown_fields)]` to the `Package` struct:

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]   // ← ADD: reject unknown fields like edition
pub struct Package {
```

This ensures that existing manifests containing `edition = "2024"` produce a clear parse error rather than silently ignoring the field. The `Manifest` struct already has `deny_unknown_fields` (line 11) — adding it to `Package` extends the same strictness to the `[package]` section. This is a deliberate breaking change; aipm is pre-1.0.

### 5.3 Generation Path Changes

#### Path 1: Package init (`init.rs`)

Remove the `edition = "2024"\n` line from the `generate_manifest()` format string at ~line 202:

```
BEFORE:  name, version, type, edition
AFTER:   name, version, type
```

#### Path 2: Starter plugin manifest (`workspace_init/mod.rs`)

Remove `edition = "2024"\n` from `generate_starter_manifest()` at ~line 276.

#### Path 3: Scaffold script template (`workspace_init/mod.rs`)

Remove `edition = "2024"` from the embedded JavaScript template string at ~line 360.

#### Path 4 & 5: Migrate emitter (`migrate/emitter.rs`)

Remove the `edition` field from the `PluginPackage` serialization struct at ~line 825. This automatically removes it from both generation paths (~lines 659 and 897) since both construct a `PluginPackage`.

#### Path 6: Scaffold TypeScript file (`.ai/starter-aipm-plugin/scripts/scaffold-plugin.ts`)

Remove `edition = "2024"` from the TOML template string at ~line 25.

### 5.4 Test Changes

| Test | File | Change |
|------|------|--------|
| BDD step: `the manifest contains an edition field` | `tests/features/manifest/init.feature:13` | Remove this step |
| BDD step impl: `then_manifest_has_edition` | `crates/libaipm/tests/bdd.rs:405-408` | Remove step function |
| BDD helper: manifest with edition | `crates/libaipm/tests/bdd.rs:153` | Remove `edition = "2024"` from template |
| Unit test: `manifest_contains_edition` | `crates/libaipm/src/init.rs:376-388` | Remove entire test |
| E2E assertion | `crates/aipm-pack/tests/init_e2e.rs:42` | Remove `assert!(content.contains("edition"))` |
| E2E assertion | `crates/aipm-pack/tests/init_e2e.rs:248` | Remove `assert!(content.contains("edition"))` |
| Parse test: `parse_full_member_manifest` | `crates/libaipm/src/manifest/mod.rs:78` | Remove `edition = "2024"` from test TOML |
| Snapshot | `workspace_init/snapshots/...scaffold_script_snapshot.snap` | Will auto-update when snapshot test runs |

**New test to add**: Verify that a manifest containing `edition` is rejected (breaking change):

```rust
#[test]
fn edition_field_rejected() {
    let toml = r#"
[package]
name = "my-plugin"
version = "0.1.0"
edition = "2024"
"#;
    let result = parse_and_validate(toml, None);
    assert!(result.is_err());
}
```

### 5.5 Research Document Update

Update `research/docs/2026-03-26-edition-field-purpose-and-rationale.md` to note the decision to remove the field, linking to this spec.

## 6. Alternatives Considered

| Option | Pros | Cons | Decision |
|--------|------|------|----------|
| **A: Remove `edition` entirely (Selected)** | Eliminates confusion; simplifies schema; removes dead code; honest about what aipm is | Breaking change for existing manifests | **Selected** — the field has no purpose for AI plugins. Pre-1.0 breaking changes are expected |
| **B: Keep and formalize with validation** | Follows Cargo pattern; "future-proofing" | Cargo editions exist for compiled language versioning — AI plugins have no compiler, no runtime, no language versions. Formalizing a meaningless field adds complexity for zero value | Rejected — solving a problem that doesn't exist |
| **C: Keep as-is (unvalidated placeholder)** | Zero effort | Continues to confuse users; pollutes every generated manifest; premature schema commitment | Rejected — doing nothing perpetuates the problem |

## 7. Cross-Cutting Concerns

### 7.1 Breaking Change — Pre-1.0 Semver

aipm is pre-1.0 (`0.x.y`). Per semver, breaking changes are expected in pre-release versions. Removing `edition` is an intentional breaking change:

- Existing manifests containing `edition` will fail to parse after this change
- Users must remove the `edition` line from their `aipm.toml` files
- The error message from serde's `deny_unknown_fields` will clearly identify `edition` as the unknown field

### 7.2 Generated Manifests in the Wild

Manifests previously generated by `aipm init`, `aipm migrate`, or the scaffold script contain `edition = "2024"`. After this change, these manifests will fail to parse. Users must delete the `edition` line. This is acceptable for a pre-1.0 tool with a small user base.

## 8. Migration, Rollout, and Testing

### 8.1 Deployment Strategy

Single-step removal — all changes ship together:

- [ ] Phase 1: Remove `edition` from `Package` struct, add `deny_unknown_fields` to `Package`, remove from all generation templates
- [ ] Phase 2: Remove edition-related tests; add rejection test for manifests containing `edition`
- [ ] Phase 3: Update snapshot tests
- [ ] Phase 4: Run full quality gates (`build`, `test`, `clippy`, `fmt`, coverage)

### 8.2 Files Modified

| File | Change |
|------|--------|
| `crates/libaipm/src/manifest/types.rs` | Remove `edition` field from `Package`; add `deny_unknown_fields` to `Package` |
| `crates/libaipm/src/manifest/mod.rs` | Remove `edition` from test TOML; add rejection test |
| `crates/libaipm/src/init.rs` | Remove `edition` line from template; remove `manifest_contains_edition` test |
| `crates/libaipm/src/workspace_init/mod.rs` | Remove `edition` from starter manifest and scaffold script templates |
| `crates/libaipm/src/migrate/emitter.rs` | Remove `edition` field from `PluginPackage` struct and both construction sites |
| `.ai/starter-aipm-plugin/scripts/scaffold-plugin.ts` | Remove `edition` from TOML template |
| `tests/features/manifest/init.feature` | Remove edition step |
| `crates/libaipm/tests/bdd.rs` | Remove edition step impl and template reference |
| `crates/aipm-pack/tests/init_e2e.rs` | Remove edition assertions |
| Snapshot file | Auto-updated by test runner |

### 8.3 Files NOT Modified

- `validate.rs` — already has no edition references
- `error.rs` — no edition error to remove
- Workspace root manifests — never had edition
- `Cargo.toml` — Rust's own `edition = "2021"` is unrelated

## 9. Open Questions / Unresolved Issues

None. This is a straightforward breaking removal of a dead field in a pre-1.0 project.
