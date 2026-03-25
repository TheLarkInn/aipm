# Fix marketplace.json Description Mismatch Bug

| Document Metadata      | Details                          |
| ---------------------- | -------------------------------- |
| Author(s)              | selarkin                         |
| Status                 | Draft (WIP)                      |
| Team / Owner           | aipm                             |
| Created / Last Updated | 2026-03-24                       |

## 1. Executive Summary

When `aipm migrate` converts `.claude/` artifacts into marketplace plugins, the `description` field written to `marketplace.json` is always the hardcoded string `"Migrated from .claude/ configuration"`, even when the source artifact (e.g., SKILL.md) has a real description. Meanwhile, the per-plugin `plugin.json` correctly uses the actual description. This spec proposes threading description data through the registrar so that `marketplace.json` entries match their corresponding `plugin.json` descriptions.

## 2. Context and Motivation

### 2.1 Current State

The migrate pipeline flows through four stages:

1. **Discovery** (`discovery.rs`) — finds `.claude/` directories
2. **Detection** (`*_detector.rs`) — extracts `Artifact` structs containing `ArtifactMetadata` with a `description: Option<String>` field
3. **Emission** (`emitter.rs`) — writes `plugin.json` and `aipm.toml` per plugin, using `metadata.description` with fallback
4. **Registration** (`registrar.rs`) — appends entries to `marketplace.json` using only plugin names

The registration step receives `&[String]` (names only) and hardcodes the description:

```rust
// registrar.rs:36-40
plugins.push(serde_json::json!({
    "name": name,
    "source": format!("./{name}"),
    "description": "Migrated from .claude/ configuration"
}));
```

### 2.2 The Problem

- **User Impact:** After running `aipm migrate`, a user inspecting `marketplace.json` sees generic `"Migrated from .claude/ configuration"` descriptions for all plugins, even when the source artifacts had meaningful descriptions like `"Deploy app"` or `"Analyze bugs by reading bug reports."`. The `plugin.json` for each plugin has the correct description, creating an inconsistency.
- **Technical Debt:** The `register_plugins()` function signature (`&[String]`) structurally prevents description propagation. Both call sites in `mod.rs` (lines 294 and 423) only pass names.

**Related research:** [research/docs/2026-03-24-marketplace-description-mismatch-bug.md](../research/docs/2026-03-24-marketplace-description-mismatch-bug.md)

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [x] `marketplace.json` entries must use the same description as the corresponding `plugin.json` when available
- [x] When no description exists in the source artifact, fall back to `"Migrated from .claude/ configuration"` (preserving current behavior for description-less artifacts)
- [x] Existing registrar tests must be updated to verify description propagation
- [x] New tests must verify marketplace/plugin.json description parity

### 3.2 Non-Goals (Out of Scope)

- [ ] Propagating additional metadata fields (version, author) to `marketplace.json` — separate future work
- [ ] Changing the workspace-init (`aipm init`) marketplace.json generation — starter plugin descriptions are intentionally different
- [ ] Changing the scaffold-plugin.ts runtime behavior — it uses `TODO: describe ${name}` by design

## 4. Proposed Solution (High-Level Design)

### 4.1 Data Flow (Before vs After)

**Before:**
```
ArtifactMetadata.description
    ├── emitter → plugin.json   ✅ uses real description
    ├── emitter → aipm.toml     ✅ uses real description
    └── registrar → marketplace.json  ❌ hardcoded "Migrated from .claude/ configuration"
```

**After:**
```
ArtifactMetadata.description
    ├── emitter → plugin.json   ✅ uses real description
    ├── emitter → aipm.toml     ✅ uses real description
    └── registrar → marketplace.json  ✅ uses real description (with fallback)
```

### 4.2 Approach

Introduce a lightweight struct to pair plugin names with their descriptions, change the registrar's function signature to accept it, and update both call sites in `mod.rs` to collect and pass descriptions alongside names.

## 5. Detailed Design

### 5.1 New Type: `PluginEntry`

Add a small struct in `mod.rs` to carry registration data:

```rust
/// Data needed to register a plugin in `marketplace.json`.
#[derive(Debug, Clone)]
pub struct PluginEntry {
    /// Plugin name.
    pub name: String,
    /// Plugin description (from artifact metadata or fallback).
    pub description: Option<String>,
}
```

### 5.2 Registrar Signature Change

**File:** `crates/libaipm/src/migrate/registrar.rs`

Change the function signature from:

```rust
pub fn register_plugins(ai_dir: &Path, plugin_names: &[String], fs: &dyn Fs) -> Result<(), Error>
```

To:

```rust
pub fn register_plugins(ai_dir: &Path, entries: &[PluginEntry], fs: &dyn Fs) -> Result<(), Error>
```

Update the loop body to use the entry's description with fallback:

```rust
for entry in entries {
    let already_registered = plugins
        .iter()
        .any(|p| p.get("name").and_then(serde_json::Value::as_str) == Some(entry.name.as_str()));
    if already_registered {
        continue;
    }

    let description = entry
        .description
        .as_deref()
        .unwrap_or("Migrated from .claude/ configuration");

    plugins.push(serde_json::json!({
        "name": entry.name,
        "source": format!("./{}", entry.name),
        "description": description
    }));
}
```

The early-return guard changes from `plugin_names.is_empty()` to `entries.is_empty()`.

### 5.3 Call Site Updates in `mod.rs`

#### Single-source path (`migrate_single_source`, line ~276-294)

Change `registered_names: Vec<String>` to `registered_entries: Vec<PluginEntry>`.

After `emitter::emit_plugin()` returns the `plugin_name`, capture the description from the artifact:

```rust
registered_entries.push(PluginEntry {
    name: plugin_name,
    description: artifact.metadata.description.clone(),
});
```

Pass `&registered_entries` to `registrar::register_plugins()`.

Update the `MarketplaceRegistered` action loop to use `entry.name`.

#### Recursive path (`migrate_recursive`, line ~414-423)

Change `registered_names: Vec<String>` to `registered_entries: Vec<PluginEntry>`.

After collecting emission results, build entries by extracting the description from the plan:

- **Single-artifact plans:** use `plan.artifacts.first()` metadata description
- **Package-scoped plans:** use `plan.artifacts.first()` metadata description (same heuristic the emitter uses at `emit_package_plugin`)

```rust
registered_entries.push(PluginEntry {
    name,
    description: plan.artifacts.first().and_then(|a| a.metadata.description.clone()),
});
```

Pass `&registered_entries` to `registrar::register_plugins()`.

### 5.4 Description Source by Detector Type

No changes needed to any detectors. The existing description extraction works correctly:

| Detector | Description Source | Fallback in marketplace.json |
|----------|-------------------|------------------------------|
| SkillDetector | SKILL.md `description:` frontmatter | `"Migrated from .claude/ configuration"` |
| CommandDetector | command.md `description:` frontmatter | `"Migrated from .claude/ configuration"` |
| AgentDetector | agent.md `description:` frontmatter | `"Migrated from .claude/ configuration"` |
| OutputStyleDetector | output-style.md `description:` frontmatter | `"Migrated from .claude/ configuration"` |
| McpDetector | Hardcoded `"{N} MCP server(s) from .mcp.json"` | passes through to marketplace.json |
| HookDetector | Hardcoded `"Hooks extracted from .claude/settings.json"` | passes through to marketplace.json |

## 6. Alternatives Considered

| Option | Pros | Cons | Reason for Rejection |
|--------|------|------|----------------------|
| A: Read `plugin.json` back in registrar | No signature change needed | Extra I/O, circular dependency (emitter writes then registrar reads), fragile coupling | Registrar would depend on emitter's file layout |
| B: Pass `&[(String, Option<String>)]` tuples | Minimal change, no new type | Less readable, harder to extend later | Tuple fields are unnamed and harder to reason about |
| **C: `PluginEntry` struct (Selected)** | Self-documenting, extensible | Introduces a new type | **Selected:** Clean, readable, and extensible if more fields are needed later |

## 7. Cross-Cutting Concerns

### 7.1 Backward Compatibility

- Plugins migrated before this fix will retain their hardcoded descriptions in `marketplace.json`. Re-running `aipm migrate` skips already-registered plugins (the `already_registered` guard), so existing entries are not updated. This is acceptable — the fix only applies to new migrations.

### 7.2 Fallback Behavior

- The `"Migrated from .claude/ configuration"` fallback string is preserved for artifacts that genuinely have no description (e.g., a SKILL.md with no frontmatter, or McpDetector/HookDetector artifacts where the description is already descriptive of the source).

## 8. Test Plan

### 8.1 Unit Tests — `registrar.rs`

- **Update existing tests** to use `PluginEntry` instead of `String`
- **New test: `register_uses_entry_description`** — verify that when a `PluginEntry` has `description: Some("Deploy app")`, the written marketplace.json contains `"description": "Deploy app"`
- **New test: `register_uses_fallback_when_no_description`** — verify that when `description: None`, the fallback `"Migrated from .claude/ configuration"` is used
- **New test: `register_mixed_descriptions`** — verify a mix of entries with and without descriptions produces correct output

### 8.2 Unit Tests — `mod.rs`

- **Update `migrate_full_flow`** — after migration, read the written marketplace.json and verify the "deploy" plugin has description `"Deploy app"` (matching the SKILL.md frontmatter) and the "review" command has the fallback description (since the test fixture has no frontmatter)

### 8.3 Integration / E2E Tests — `migrate_e2e.rs`

- **New test: `migrate_marketplace_description_matches_plugin_json`** — run a full migration with a skill that has a description in its SKILL.md, then parse both the emitted `plugin.json` and the `marketplace.json` and assert the descriptions are equal

### 8.4 BDD — `tests/features/manifest/migrate.feature`

- Consider adding a scenario that verifies marketplace.json description parity if BDD coverage of the registrar is desired

## 9. Files to Modify

| File | Change |
|------|--------|
| `crates/libaipm/src/migrate/mod.rs` | Add `PluginEntry` struct; update both `migrate_single_source` and `migrate_recursive` to collect `Vec<PluginEntry>` and pass to registrar |
| `crates/libaipm/src/migrate/registrar.rs` | Change `register_plugins` signature to `&[PluginEntry]`; use entry description with fallback |
| `crates/aipm/tests/migrate_e2e.rs` | Add E2E test verifying marketplace/plugin.json description parity |

## 10. Open Questions / Unresolved Issues

- [ ] Should the `PluginEntry` struct live in `mod.rs` alongside other pipeline types, or in `registrar.rs` since it's only used by the registrar? (Recommendation: `mod.rs` since it's constructed by the pipeline orchestrator)
- [ ] For package-scoped plugins with multiple artifacts that have different descriptions, should we concatenate them, use the first, or pick the "best"? (Recommendation: use the first artifact's description, matching the emitter's existing heuristic)
