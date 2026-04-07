# Lint Rules for marketplace.json and plugin.json Validation

| Document Metadata      | Details                                                   |
| ---------------------- | --------------------------------------------------------- |
| Author(s)              | Sean Larkin                                               |
| Status                 | Implemented                                               |
| Team / Owner           | aipm                                                      |
| Created / Last Updated | 2026-04-07                                                |
| Issues                 | #287, #288, #289, #290                                    |
| Research               | `research/docs/2026-04-07-lint-rules-287-288-289-290.md`  |

## 1. Executive Summary

This spec adds five new lint rules that validate `marketplace.json` and `plugin.json` files in the `.ai/` marketplace directory. These rules catch common authoring mistakes: missing or unresolvable source paths, inconsistent name/description fields between manifests, missing plugin registration, and missing required plugin.json fields. The work extends the discovery pipeline with two new `FeatureKind` variants (`Marketplace`, `PluginJson`) so these JSON files participate in the existing rule dispatch system. Generator functions are also updated to emit an `author` field in plugin.json so newly-scaffolded projects pass all rules immediately.

## 2. Context and Motivation

### 2.1 Current State

The `aipm lint` pipeline discovers feature files (SKILL.md, agent .md, hooks.json, aipm.toml) via a gitignore-aware recursive walk in `crates/libaipm/src/discovery.rs`. Each discovered file is classified by `FeatureKind` and dispatched to rules via `quality_rules_for_kind()` in `crates/libaipm/src/lint/rules/mod.rs:33-51`.

Currently, 12 lint rules exist — all targeting skill frontmatter, agent frontmatter, hook JSON events, broken file paths, or misplaced features. **None target marketplace.json or plugin.json**, even though these are critical manifest files that tie the plugin ecosystem together.

The JSON manifest files (`marketplace.json` at `.ai/.claude-plugin/marketplace.json` and `plugin.json` at `.ai/<plugin>/.claude-plugin/plugin.json`) have no typed Rust structs — they are generated and consumed as raw `serde_json::Value` objects (see `crates/libaipm/src/workspace_init/mod.rs:294-309` and `crates/libaipm/src/migrate/registrar.rs:10-51`).

### 2.2 The Problem

- **User Impact:** Developers can forget to register a plugin in marketplace.json, misconfigure source paths, or leave plugin.json incomplete — with no lint feedback until runtime failure.
- **Consistency:** Name and description can drift between marketplace.json and plugin.json with no automated check.
- **Discovery Gap:** The `FeatureKind` enum does not include `Marketplace` or `PluginJson`, so these files are invisible to the lint pipeline.

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [x] **#290** — `marketplace/source-resolve`: Error when a plugin's `source` field is missing or its resolved path does not exist on disk.
- [x] **#289** — `marketplace/plugin-field-mismatch`: Error when `name` or `description` in marketplace.json differs from the corresponding plugin.json.
- [x] **#287** — `plugin/missing-registration`: Error when a plugin directory under `.ai/` is not listed in marketplace.json.
- [x] **#287** — `plugin/missing-manifest`: Error when a plugin directory under `.ai/` lacks a `.claude-plugin/plugin.json` file.
- [x] **#288** — `plugin/required-fields`: Error when plugin.json is missing required fields (`name`, `description`, `version`, `author.name`, `author.email`).
- [x] Extend `FeatureKind` with `Marketplace` and `PluginJson` variants in `discovery.rs`.
- [x] Update `generate_plugin_json()` in `workspace_init/mod.rs` and `generate_plugin_json_multi()` in `migrate/emitter.rs` to include `author: { name, email }`.
- [x] All rules configurable via `aipm.toml` `[workspace.lints]` (allow, warn, error).
- [x] All rules have `help_text` guiding the user to fix the issue.
- [x] All new rules have unit tests using `MockFs`, covering happy-path and violation cases.
- [x] All four `cargo build/test/clippy/fmt` gates pass with zero warnings.
- [x] Branch coverage remains >= 89%.

### 3.2 Non-Goals (Out of Scope)

- [ ] Auto-fix capability for #289 (future work per issue description).
- [ ] Validating marketplace.json under `.claude/` or `.github/` source dirs — only `.ai/` is scoped.
- [ ] Creating typed Rust structs for marketplace.json/plugin.json (raw `serde_json::Value` is sufficient).
- [ ] Validating fields beyond what the issues specify (e.g., semver format of `version`).

## 4. Proposed Solution (High-Level Design)

### 4.1 Discovery Extension

Extend `FeatureKind` in `crates/libaipm/src/discovery.rs` with two new variants:

```
FeatureKind::Marketplace  — triggered by marketplace.json at .ai/.claude-plugin/marketplace.json
FeatureKind::PluginJson   — triggered by plugin.json at .ai/<plugin>/.claude-plugin/plugin.json
```

Detection logic in the walk loop (alongside existing `aipm.toml` detection):

- `marketplace.json`: file named `marketplace.json`, parent is `.claude-plugin`, grandparent is `.ai`.
- `plugin.json`: file named `plugin.json`, parent is `.claude-plugin`, grandparent is a plugin dir under `.ai/` (i.e., great-grandparent is `.ai`).

### 4.2 Rule Dispatch

Add two new arms to `quality_rules_for_kind()` in `crates/libaipm/src/lint/rules/mod.rs`:

```rust
FeatureKind::Marketplace => vec![
    Box::new(marketplace_source_resolve::SourceResolve),
    Box::new(marketplace_field_mismatch::FieldMismatch),
    Box::new(plugin_missing_registration::MissingRegistration),
],
FeatureKind::PluginJson => vec![
    Box::new(plugin_required_fields::RequiredFields),
    Box::new(plugin_missing_manifest::MissingManifest),
],
```

### 4.3 New Rule Files

```
crates/libaipm/src/lint/rules/
├── marketplace_source_resolve.rs      (#290)
├── marketplace_field_mismatch.rs      (#289)
├── plugin_missing_registration.rs     (#287a — unregistered plugin dirs)
├── plugin_missing_manifest.rs         (#287b — missing plugin.json)
├── plugin_required_fields.rs          (#288)
```

### 4.4 Key Components

| Component | Responsibility | File |
| --------- | -------------- | ---- |
| Discovery extension | Detect marketplace.json and plugin.json as features | `crates/libaipm/src/discovery.rs` |
| Rule dispatch | Route new feature kinds to new rules | `crates/libaipm/src/lint/rules/mod.rs` |
| `marketplace/source-resolve` | Validate source paths resolve | `crates/libaipm/src/lint/rules/marketplace_source_resolve.rs` |
| `marketplace/plugin-field-mismatch` | Cross-reference marketplace.json ↔ plugin.json | `crates/libaipm/src/lint/rules/marketplace_field_mismatch.rs` |
| `plugin/missing-registration` | Verify plugin dirs are listed in marketplace.json | `crates/libaipm/src/lint/rules/plugin_missing_registration.rs` |
| `plugin/missing-manifest` | Verify plugin dirs have plugin.json | `crates/libaipm/src/lint/rules/plugin_missing_manifest.rs` |
| `plugin/required-fields` | Validate required fields in plugin.json | `crates/libaipm/src/lint/rules/plugin_required_fields.rs` |
| Scan helpers | Read/parse marketplace.json and plugin.json | `crates/libaipm/src/lint/rules/scan.rs` |
| MockFs helpers | Test helpers for new file types | `crates/libaipm/src/lint/rules/test_helpers.rs` |
| Generator updates | Add `author` field to generated plugin.json | `workspace_init/mod.rs`, `migrate/emitter.rs` |

## 5. Detailed Design

### 5.1 Discovery Changes (`discovery.rs`)

Add to the `FeatureKind` enum:

```rust
pub enum FeatureKind {
    Skill,
    Agent,
    Hook,
    Plugin,
    Marketplace, // NEW
    PluginJson,  // NEW
}
```

Add detection branches in the walk loop (after the `aipm.toml` branch at line ~296):

```rust
} else if file_name == "marketplace.json" && parent_name == ".claude-plugin" {
    // .ai/.claude-plugin/marketplace.json — marketplace manifest
    if grandparent_name == ".ai" {
        Some(FeatureKind::Marketplace)
    } else {
        None
    }
} else if file_name == "plugin.json" && parent_name == ".claude-plugin" {
    // .ai/<plugin>/.claude-plugin/plugin.json — plugin manifest
    // Great-grandparent must be ".ai" to avoid matching random plugin.json files.
    let great_grandparent_name = file_path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if great_grandparent_name == ".ai" {
        Some(FeatureKind::PluginJson)
    } else {
        None
    }
```

### 5.2 Scan Helpers (`scan.rs`)

Add these functions:

```rust
/// Read and parse `.ai/.claude-plugin/marketplace.json`.
/// Returns `None` if the file does not exist or cannot be read.
pub fn read_marketplace_json(ai_dir: &Path, fs: &dyn Fs) -> Option<(PathBuf, serde_json::Value)>

/// Read and parse `.ai/<plugin>/.claude-plugin/plugin.json`.
/// Returns `None` if the file does not exist or cannot be read.
pub fn read_plugin_json(plugin_dir: &Path, fs: &dyn Fs) -> Option<(PathBuf, serde_json::Value)>

/// List plugin directories under `.ai/`, excluding `.claude-plugin`.
/// Returns directory names (not full paths).
pub fn list_plugin_dirs(ai_dir: &Path, fs: &dyn Fs) -> Vec<String>
```

### 5.3 MockFs Helpers (`test_helpers.rs`)

```rust
/// Add marketplace.json at `.ai/.claude-plugin/marketplace.json`.
pub fn add_marketplace_json(&mut self, content: &str)

/// Add plugin.json at `.ai/<plugin>/.claude-plugin/plugin.json`.
pub fn add_plugin_json(&mut self, plugin: &str, content: &str)
```

### 5.4 Rule: `marketplace/source-resolve` (#290)

**File:** `marketplace_source_resolve.rs`
**ID:** `"marketplace/source-resolve"`
**Default severity:** `Error`
**Triggered by:** `FeatureKind::Marketplace`

**Logic in `check_file()`:**
1. Derive `ai_dir` from `file_path` (the `.ai/` directory, two levels up from marketplace.json).
2. Read marketplace.json via `fs.read_to_string()`, parse as `serde_json::Value`.
3. If parse fails, emit a diagnostic and return.
4. Get `plugins` array. If missing or not an array, emit diagnostic.
5. For each plugin entry:
   a. If `source` field is missing or not a string, emit diagnostic pointing at the marketplace.json file.
   b. Strip leading `./` from source value, join with `ai_dir` to get resolved path.
   c. If `!fs.exists(&resolved)`, emit diagnostic: `"plugin source path does not resolve: {source}"`.

**Help text:** `"ensure the source field points to an existing plugin directory under .ai/"`

**Diagnostic file_path:** `.ai/.claude-plugin/marketplace.json` (all diagnostics point here).
**Diagnostic source_type:** `".ai"`.

### 5.5 Rule: `marketplace/plugin-field-mismatch` (#289)

**File:** `marketplace_field_mismatch.rs`
**ID:** `"marketplace/plugin-field-mismatch"`
**Default severity:** `Error`
**Triggered by:** `FeatureKind::Marketplace`

**Logic in `check_file()`:**
1. Derive `ai_dir` from `file_path`.
2. Read and parse marketplace.json.
3. For each plugin entry with a `source` field:
   a. Resolve the plugin directory (strip `./`, join with `ai_dir`).
   b. Attempt to read `<resolved>/.claude-plugin/plugin.json`. If it doesn't exist, skip (other rules handle that).
   c. Parse plugin.json as `serde_json::Value`.
   d. Compare `name`: if marketplace entry `name` differs from plugin.json `name`, emit diagnostic.
   e. Compare `description`: if marketplace entry `description` differs from plugin.json `description`, emit diagnostic. (Only if both are present — skip if either is absent.)

**Help text:** `"update marketplace.json or plugin.json so the name and description fields match"`

**Diagnostic file_path:** `.ai/.claude-plugin/marketplace.json`.

### 5.6 Rule: `plugin/missing-registration` (#287a)

**File:** `plugin_missing_registration.rs`
**ID:** `"plugin/missing-registration"`
**Default severity:** `Error`
**Triggered by:** `FeatureKind::Marketplace`

**Logic in `check_file()`:**
1. Derive `ai_dir` from `file_path`.
2. Read and parse marketplace.json. Extract the set of registered plugin names from `plugins[].source` (strip `./` prefix to get directory names).
3. List all directories under `ai_dir` via `fs.read_dir()`.
4. Exclude the `.claude-plugin` directory itself (it's the marketplace config dir, not a plugin).
5. For each plugin directory NOT in the registered set, emit diagnostic: `"plugin directory '{name}' is not registered in marketplace.json"`.

**Help text:** `"add this plugin to the plugins array in .ai/.claude-plugin/marketplace.json"`

**Diagnostic file_path:** `.ai/.claude-plugin/marketplace.json`.

### 5.7 Rule: `plugin/missing-manifest` (#287b)

**File:** `plugin_missing_manifest.rs`
**ID:** `"plugin/missing-manifest"`
**Default severity:** `Error`
**Triggered by:** `FeatureKind::Marketplace`

This rule also triggers on the marketplace.json file (since it needs the list of registered plugins to know what to check).

**Logic in `check_file()`:**
1. Derive `ai_dir` from `file_path`.
2. List all directories under `ai_dir`, excluding `.claude-plugin`.
3. For each plugin directory, check if `.ai/<plugin>/.claude-plugin/plugin.json` exists via `fs.exists()`.
4. If not, emit diagnostic: `"plugin '{name}' is missing .claude-plugin/plugin.json"`.

**Help text:** `"create a .claude-plugin/plugin.json file in the plugin directory"`

**Diagnostic file_path:** The expected plugin.json path (e.g., `.ai/my-plugin/.claude-plugin/plugin.json`).

### 5.8 Rule: `plugin/required-fields` (#288)

**File:** `plugin_required_fields.rs`
**ID:** `"plugin/required-fields"`
**Default severity:** `Error`
**Triggered by:** `FeatureKind::PluginJson`

**Logic in `check_file()`:**
1. Read plugin.json via `fs.read_to_string()`, parse as `serde_json::Value`.
2. If parse fails, emit a single diagnostic and return.
3. Check each required top-level field: `name`, `description`, `version`.
   - Missing or empty string → diagnostic per field.
4. Check `author` object:
   - If `author` is missing or not an object → diagnostic.
   - If `author.name` is missing or empty → diagnostic.
   - If `author.email` is missing or empty → diagnostic.

**Help text:** `"add the missing required fields to plugin.json (name, description, version, author.name, author.email)"`

**Diagnostic file_path:** The plugin.json file path.

### 5.9 Generator Updates

#### `workspace_init/mod.rs:294-309` — `generate_plugin_json()`

Add an `author` object with placeholder values:

```json
{
  "name": "starter-aipm-plugin",
  "version": "0.1.0",
  "description": "Default starter plugin...",
  "author": {
    "name": "TODO",
    "email": "TODO"
  }
}
```

#### `migrate/emitter.rs:1100-1146` — `generate_plugin_json_multi()`

Add an `author` object with placeholder values (`ArtifactMetadata` does not carry an author field, so TODO placeholders are always emitted):

```json
{
  "name": "...",
  "version": "0.1.0",
  "description": "...",
  "author": {
    "name": "TODO",
    "email": "TODO"
  }
}
```

#### `workspace_init/mod.rs` — TypeScript scaffold script

Update the inline scaffold script (`generate_scaffold_script()`) to also emit `author` in the generated plugin.json JSON.stringify call.

### 5.10 Rule Configuration

All five rules follow the existing `aipm.toml` `[workspace.lints]` config pattern:

```toml
[workspace.lints]
"marketplace/source-resolve" = "warn"       # downgrade from error
"marketplace/plugin-field-mismatch" = "allow"  # suppress
"plugin/missing-registration" = "error"     # default
"plugin/missing-manifest" = "error"         # default
"plugin/required-fields" = "warn"           # downgrade
```

No changes to `config.rs` or `load_lint_config()` are needed — the existing override infrastructure handles arbitrary rule IDs.

## 6. Alternatives Considered

| Option | Pros | Cons | Reason for Rejection |
| ------ | ---- | ---- | -------------------- |
| Independent scanning (rules scan without discovery) | No discovery.rs changes | Requires new dispatch hook in lint engine, rules run outside normal pipeline | Breaks the established single-pass architecture |
| Attach to existing FeatureKind::Plugin | Minimal changes | Only triggers when aipm.toml exists; most init'd workspaces don't have aipm.toml | Would miss the majority of real-world cases |
| **Extend FeatureKind (Selected)** | Natural integration with existing pipeline, rules dispatched identically to all other rules | Touches discovery.rs | Selected: cleanest integration, no special-casing |
| Single rule for #287 | Simpler | Users can't independently configure the two checks | Two rule IDs give better granularity |

## 7. Cross-Cutting Concerns

### 7.1 Backwards Compatibility

- New rules default to `Error` severity. Existing projects that lack `author` in plugin.json will see new errors. This is intentional — the generator updates ensure new projects are compliant, and existing projects should be updated.
- Users can downgrade or suppress any rule via `aipm.toml` `[workspace.lints]`.

### 7.2 Performance

- marketplace.json is read once per marketplace (a single file). plugin.json is read once per plugin. Both are small JSON files. No measurable performance impact.
- Rules that need marketplace.json content (3 rules) each read it independently. Since marketplace.json is tiny (<1KB typically), this is acceptable. If it becomes a concern later, a shared scan cache could be introduced.

### 7.3 Error Handling

Following the established convention:
- JSON parse failures → `Diagnostic` (the rule continues to other files)
- I/O failures reading the file → `Err(lint::Error::Io(...))` (infrastructure failure)
- Missing files (e.g., plugin.json doesn't exist) → `Ok(vec![])` or a diagnostic depending on rule purpose

## 8. Migration, Rollout, and Testing

### 8.1 Deployment Strategy

Single PR. All rules ship at once since they share infrastructure changes (discovery, scan helpers, MockFs).

### 8.2 Test Plan

#### Unit Tests (per rule file, using MockFs)

Each rule file should have at minimum:

**`marketplace/source-resolve`:**
- Source present and resolves → no diagnostic
- Source missing from entry → diagnostic
- Source present but path does not exist → diagnostic
- Malformed JSON → diagnostic
- No plugins array → diagnostic
- `check_file()` with nonexistent file → empty

**`marketplace/plugin-field-mismatch`:**
- Name and description match → no diagnostic
- Name mismatch → diagnostic
- Description mismatch → diagnostic
- Both mismatch → two diagnostics
- plugin.json not found → skip (no diagnostic from this rule)
- plugin.json parse error → diagnostic

**`plugin/missing-registration`:**
- All dirs registered → no diagnostic
- Unregistered dir → diagnostic
- `.claude-plugin` dir excluded → no diagnostic for it
- Empty marketplace → diagnostic for every plugin dir

**`plugin/missing-manifest`:**
- All dirs have plugin.json → no diagnostic
- Dir missing plugin.json → diagnostic
- `.claude-plugin` dir excluded

**`plugin/required-fields`:**
- All fields present → no diagnostic
- Each missing field → one diagnostic per field
- `author` missing → diagnostic
- `author.name` missing → diagnostic
- `author.email` missing → diagnostic
- Empty strings treated as missing → diagnostic
- Malformed JSON → diagnostic
- `check_file()` with nonexistent file → empty

#### Integration Tests (`lint/mod.rs` tests)

- End-to-end test with `tempfile::tempdir()` that creates a marketplace with marketplace.json + plugin.json, runs the full `lint()` pipeline, and verifies diagnostics from new rules.
- Config override test: suppress new rules via `RuleOverride::Allow`, verify they're skipped.

#### Generator Tests

- Verify `generate_plugin_json()` output includes `author` object.
- Verify `generate_plugin_json_multi()` output includes `author` object.
- Update existing snapshot tests if applicable.

## 9. Open Questions / Unresolved Issues

All open questions have been resolved:

- [x] **Discovery approach** → Extend `FeatureKind` with `Marketplace` and `PluginJson`.
- [x] **#287 rule split** → Two separate rule IDs: `plugin/missing-registration` and `plugin/missing-manifest`.
- [x] **#288 author field** → Default `Error` severity; update generators to include `author: { name, email }`.
- [x] **Directory naming** → Use `.claude-plugin/` (singular), matching codebase convention.
- [x] **Scope** → Only validate `.ai/.claude-plugin/marketplace.json`.
