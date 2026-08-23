# Port Plugin Validation Rules into `aipm lint` — Technical Design Document

| Document Metadata      | Details                          |
| ---------------------- | -------------------------------- |
| Author(s)              | Nick Pape                        |
| Status                 | Draft (WIP)                      |
| Team / Owner           | AIPM Core                        |
| Created / Last Updated | 2026-04-01 / 2026-04-01         |

## 1. Executive Summary

This spec adds 6 new lint rules to `aipm lint`, inspired by plugin validation patterns observed in a large enterprise monorepo. The rules fall into two tiers: **plugin structure integrity** (3 rules) validates that `.claude-plugin/plugin.json` exists and is free of known-broken patterns, and that no root-level MCP config files bypass the plugin system; **VS Code location sync** (3 rules) validates that `.vscode/settings.json` correctly registers plugin skill/agent directories and warns about ineffective repo-level `chat.pluginLocations` settings. Rules `plugin/missing-manifest` and `plugin/banned-mcp-servers` default to Error severity; all others default to Warning. Implementation follows the existing `Rule` trait pattern with a new `scan_plugin_dirs()` scanner and `MockFs`-based tests.

**Research basis**:
- [research/docs/2026-04-01-plugin-validation-lint-rules.md](../research/docs/2026-04-01-plugin-validation-lint-rules.md) — Research on plugin validation patterns and aipm lint architecture
- [research/docs/2026-03-31-110-aipm-lint-architecture-research.md](../research/docs/2026-03-31-110-aipm-lint-architecture-research.md) — Original lint architecture research

---

## 2. Context and Motivation

### 2.1 Current State

`aipm lint` currently exposes multiple rule groups ([`specs/2026-03-31-aipm-lint-command.md`](./2026-03-31-aipm-lint-command.md)). The `.ai/` marketplace rule group contains 11 rules focused on **skill/agent/hook content validation**: frontmatter fields, file size limits, broken path references, hook event names, and cross-tool compatibility. All of these `.ai` rules operate by scanning individual skill, agent, and hook files. Additional rule groups target `.claude/` and `.github/` source directories.

Meanwhile, large repos that have adopted the `.ai/` marketplace structure enforce a different layer of validation: **plugin-level structural consistency**. These checks verify that `plugin.json` manifests exist, that no MCP anti-patterns are present, and that VS Code settings correctly register plugin locations. These checks typically live as ad-hoc build scripts and test suites rather than as portable `aipm lint` rules.

### 2.2 The Problem

- **Missing manifests**: A plugin directory under `.ai/` can exist without a `.claude-plugin/plugin.json`, making it invisible to Claude Code.
- **MCP footguns**: A `"mcpServers"` key in `plugin.json` breaks Claude Code entirely. Root-level MCP config files (`.vscode/mcp.json`, `mcp.json`) bypass the plugin system.
- **VS Code settings rot**: `chat.agentSkillsLocations` and `chat.agentFilesLocations` can go stale when plugins are added, removed, or renamed. `chat.pluginLocations` only works at user level, but teams often mistakenly add it at repo level where it has no effect.
- **No portability**: These validations are typically implemented as repo-specific tooling. Other repos using aipm have no equivalent checks.

---

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [ ] 6 new lint rules added to `for_marketplace()`, all configurable via `[workspace.lints]` in `aipm.toml`
- [ ] `plugin/missing-manifest` and `plugin/banned-mcp-servers` default to **Error** severity (blocking, exit code 1)
- [ ] Remaining 4 rules default to **Warning** severity (advisory, exit code 0)
- [ ] New `scan_plugin_dirs()` scanner in `scan.rs` with `FoundPluginDir` struct
- [ ] New `MockFs` helpers: `add_plugin_json()`, `add_vscode_settings()`
- [ ] Each rule has `#[cfg(test)]` unit tests following the existing pattern
- [ ] Branch coverage >= 89% for all new code

### 3.2 Non-Goals (Out of Scope)

- [ ] **Typed Rust structs for `plugin.json` or `marketplace.json`** — We parse as `serde_json::Value`, matching the existing JSON-handling pattern in `hook_unknown_event.rs`.
- [ ] **JSONC parsing for `.vscode/settings.json`** — We use standard `serde_json`. Files with comments will cause the settings rules to silently skip (no diagnostics), with graceful fallback on parse failure.
- [ ] **Copilot CLI equivalent location checks** — Only VS Code `chat.*` settings are validated.
- [ ] **Undeclared component checks** — Rules checking `skills`/`agents`/`mcp` field declarations in `plugin.json` are deferred. The `plugin.json` schema differs between Claude Code plugins and aipm plugins (which use `aipm.toml`), and these fields aren't consistently useful yet.
- [ ] **`--fix` mode** — Deferred to a future spec.

---

## 4. Proposed Solution (High-Level Design)

### 4.1 Architecture

All 6 rules follow the existing adapter pattern: zero-sized structs implementing the `Rule` trait ([`rule.rs:16-31`](../crates/libaipm/src/lint/rule.rs#L16-L31)), registered in `for_marketplace()` ([`rules/mod.rs:36-52`](../crates/libaipm/src/lint/rules/mod.rs#L36-L52)), and discoverable via `for_source(".ai")`.

```
crates/libaipm/src/lint/rules/
  scan.rs                          # + scan_plugin_dirs(), FoundPluginDir
  test_helpers.rs                  # + add_plugin_json(), add_vscode_settings()
  mod.rs                           # + 4 pub mod + 6 Box::new() entries
  plugin_missing_manifest.rs       # NEW — plugin/missing-manifest
  plugin_banned_mcp_servers.rs     # NEW — plugin/banned-mcp-servers
  repo_banned_mcp_config.rs        # NEW — repo/banned-mcp-config
  vscode_missing_location.rs       # NEW — vscode/missing-skills-location, vscode/missing-agents-location
  vscode_ineffective_locations.rs  # NEW — vscode/ineffective-plugin-locations
```

`plugin/missing-manifest` and `plugin/banned-mcp-servers` depend only on `scan_plugin_dirs()`. `repo/banned-mcp-config` uses `fs.exists()` on the workspace root. The vscode rules additionally read `.vscode/settings.json`.

### 4.2 Workspace Root Access

The `Rule::check()` method receives `source_dir` (e.g., `.ai/`). Rules that need the workspace root compute it via `source_dir.parent()`. This avoids changing the `Rule` trait signature. See [`broken_paths.rs`](../crates/libaipm/src/lint/rules/broken_paths.rs) for precedent — it already resolves paths relative to skill directories using `skill.path.parent()`.

### 4.3 Key Components

| Component | Responsibility | Justification |
|-----------|---------------|---------------|
| `scan_plugin_dirs()` | Enumerate `.ai/<plugin>/` dirs with metadata | Shared by multiple rules; avoids duplicate filesystem traversal |
| `FoundPluginDir` struct | Plugin dir name, path, parsed plugin.json, has_skills/has_agents booleans | Single scan produces all data needed by multiple rules |
| `add_plugin_json()` helper | MockFs convenience for `.claude-plugin/plugin.json` | Follows pattern of `add_skill()`, `add_agent()`, `add_hooks()` |

---

## 5. Detailed Design

### 5.1 New Scanner: `scan_plugin_dirs()`

**File:** [`crates/libaipm/src/lint/rules/scan.rs`](../crates/libaipm/src/lint/rules/scan.rs)

```rust
/// A plugin directory found during scanning.
pub struct FoundPluginDir {
    /// Directory name (e.g., "my-plugin").
    pub name: String,
    /// Path to plugin dir (e.g., .ai/my-plugin).
    pub path: PathBuf,
    /// Parsed .claude-plugin/plugin.json, if it exists and parses.
    pub plugin_json: Option<serde_json::Value>,
    /// Raw content of plugin.json for text-level checks.
    pub plugin_json_raw: Option<String>,
    /// Whether a skills/ subdirectory exists.
    pub has_skills_dir: bool,
    /// Whether an agents/ subdirectory exists.
    pub has_agents_dir: bool,
}

/// Scan all plugin directories in a marketplace directory.
///
/// Iterates `.ai/<plugin>/` for each plugin directory.
pub fn scan_plugin_dirs(marketplace_dir: &Path, fs: &dyn Fs) -> Vec<FoundPluginDir>
```

**Algorithm** (follows the `scan_skills()` pattern at lines 32-66):
1. `fs.read_dir(marketplace_dir)` — list plugins, skip non-dirs
2. Skip entries named `.claude-plugin` or `.gitignore` (infrastructure dirs, not plugins)
3. For each plugin dir:
   - Try `fs.read_to_string(<plugin>/.claude-plugin/plugin.json)` → store raw + parse as JSON
   - Check `fs.exists(<plugin>/skills/)` → `has_skills_dir`
   - Check `fs.exists(<plugin>/agents/)` → `has_agents_dir`
4. Return `Vec<FoundPluginDir>`

### 5.2 New MockFs Helpers

**File:** [`crates/libaipm/src/lint/rules/test_helpers.rs`](../crates/libaipm/src/lint/rules/test_helpers.rs)

```rust
/// Add a plugin.json at `.ai/<plugin>/.claude-plugin/plugin.json`.
pub fn add_plugin_json(&mut self, plugin: &str, content: &str)

/// Add .vscode/settings.json (relative to workspace root, not .ai/).
pub fn add_vscode_settings(&mut self, content: &str)
```

`add_plugin_json()` follows the same deduplication pattern as `add_skill()` — ensures the plugin appears in the `.ai` dir listing without duplicates, inserts both the `exists` entry and the `files` content.

`add_vscode_settings()` writes to `.vscode/settings.json` (the workspace root is the parent of `.ai/`).

### 5.3 Rule Specifications

#### `plugin/missing-manifest` (Error)

**File:** `plugin_missing_manifest.rs` | **Struct:** `MissingManifest`

**Logic:** Call `scan_plugin_dirs()`. For each plugin where `plugin_json.is_none()`, emit a diagnostic.

**Message:** `plugin "{name}" is missing .claude-plugin/plugin.json`

**Diagnostic fields:**
- `rule_id`: `"plugin/missing-manifest"`
- `severity`: `Error`
- `file_path`: `.ai/<plugin>` (the plugin directory itself)
- `line`: `None`
- `source_type`: `".ai"`

**Test cases:**
- Plugin with valid plugin.json → no diagnostic
- Plugin without plugin.json → 1 diagnostic
- Empty `.ai/` dir → no diagnostics
- Multiple plugins, one missing → 1 diagnostic
- `.claude-plugin` infrastructure dir is skipped

---

#### `plugin/banned-mcp-servers` (Error)

**File:** `plugin_banned_mcp_servers.rs` | **Struct:** `BannedMcpServers`

**Rationale:** A `"mcpServers"` key in `plugin.json` breaks Claude Code. MCP servers should be declared in `.mcp.json` files instead.

**Logic:** Call `scan_plugin_dirs()`. For each plugin where `plugin_json` parsed successfully, check whether any object in the JSON tree contains a key named `"mcpServers"`. If JSON parsing failed but `plugin_json_raw` is available, fall back to checking whether the raw text contains `"mcpServers"`.

**Message:** `plugin "{name}" plugin.json contains "mcpServers" which breaks Claude Code; use .mcp.json instead`

**Diagnostic fields:**
- `file_path`: `.ai/<plugin>/.claude-plugin/plugin.json`
- `line`: `None`
- `source_type`: `".ai"`

**Test cases:**
- plugin.json without mcpServers → no diagnostic
- plugin.json with `"mcpServers": {}` → 1 diagnostic
- plugin.json with mcpServers in a nested context → 1 diagnostic
- No plugin.json → no diagnostic

---

#### `repo/banned-mcp-config` (Warning)

**File:** `repo_banned_mcp_config.rs` | **Struct:** `BannedMcpConfig`

**Rationale:** When using the `.ai/` marketplace, all MCP servers should be declared in per-plugin `.mcp.json` files. Root-level MCP config files bypass the plugin system.

**Logic:** Compute workspace root via `source_dir.parent()`. If `None`, return empty. Check `fs.exists()` for each banned path:

```rust
const BANNED_MCP_PATHS: &[&str] = &[
    ".vscode/mcp.json",
    ".copilot/mcp.json",
    "mcp.json",
    "mcpServers.json",
];
```

Emit one diagnostic per existing banned file.

**Message:** `"<path>" should not exist at repo root; declare MCP servers in plugin .mcp.json files instead`

**Diagnostic fields:**
- `file_path`: the banned file path (e.g., `.vscode/mcp.json`)
- `line`: `None`
- `source_type`: `".ai"` (even though the file is at repo root, the rule is in the marketplace rule set)

**Test cases:**
- No banned files → no diagnostics
- Each banned file individually → 1 diagnostic each
- All 4 present → 4 diagnostics
- `source_dir` has no parent → empty (graceful)

---

#### `vscode/missing-skills-location`, `vscode/missing-agents-location` (Warning)

**File:** `vscode_missing_location.rs` | **Structs:** `MissingSkillsLocation`, `MissingAgentsLocation`

**Logic:**
1. Compute workspace root via `source_dir.parent()`
2. Read `.vscode/settings.json` — if missing or unparseable, return empty (no diagnostics)
3. Call `scan_plugin_dirs(source_dir, fs)`
4. For `MissingSkillsLocation`: extract `settings["chat.agentSkillsLocations"]` as a JSON object. For each plugin with `has_skills_dir == true`, check that the key `.ai/<name>/skills` has value `true`. If not → diagnostic.
5. For `MissingAgentsLocation`: same pattern with `settings["chat.agentFilesLocations"]` and `.ai/<name>/agents`.

**Messages:**
- `plugin "{name}" has skills/ but is missing from chat.agentSkillsLocations in .vscode/settings.json`
- `plugin "{name}" has agents/ but is missing from chat.agentFilesLocations in .vscode/settings.json`

**Diagnostic fields:**
- `file_path`: `.vscode/settings.json`
- `line`: `None`
- `source_type`: `".ai"`

**Test cases (per struct):**
- Settings has correct entry → no diagnostic
- Settings missing entry → 1 diagnostic
- No `.vscode/settings.json` → no diagnostics (silent skip)
- Settings unparseable → no diagnostics (silent skip)
- Plugin without skills/agents dir → no diagnostic
- Multiple plugins, one missing → 1 diagnostic

---

#### `vscode/ineffective-plugin-locations` (Warning)

**File:** `vscode_ineffective_locations.rs` | **Struct:** `IneffectivePluginLocations`

**Rationale:** `chat.pluginLocations` only works at the VS Code **user** settings level. When placed in a repo's `.vscode/settings.json`, it has no effect but can mislead contributors into thinking plugins are being discovered via this setting.

**Logic:**
1. Compute workspace root via `source_dir.parent()`
2. Read `.vscode/settings.json` — if missing or unparseable, return empty
3. Check if `settings["chat.pluginLocations"]` exists
4. If it exists AND is a non-empty object (has at least one key), emit a diagnostic

**Message:** `chat.pluginLocations in .vscode/settings.json has no effect (this setting only works at user level); remove it or leave it empty`

**Diagnostic fields:**
- `rule_id`: `"vscode/ineffective-plugin-locations"`
- `severity`: `Warning`
- `file_path`: `.vscode/settings.json`
- `line`: `None`
- `source_type`: `".ai"`

**Test cases:**
- No `.vscode/settings.json` → no diagnostics
- Settings without `chat.pluginLocations` → no diagnostic
- `"chat.pluginLocations": {}` (empty) → no diagnostic
- `"chat.pluginLocations": { ".ai/foo": true }` (non-empty) → 1 diagnostic
- Settings unparseable → no diagnostics (silent skip)

---

### 5.4 Rule Registration

**File:** [`crates/libaipm/src/lint/rules/mod.rs`](../crates/libaipm/src/lint/rules/mod.rs)

Add 5 new `pub mod` declarations and 6 new `Box::new()` entries in `for_marketplace()`:

```rust
// Plugin structure rules
Box::new(plugin_missing_manifest::MissingManifest),
Box::new(plugin_banned_mcp_servers::BannedMcpServers),
Box::new(repo_banned_mcp_config::BannedMcpConfig),
// VS Code location sync rules
Box::new(vscode_missing_location::MissingSkillsLocation),
Box::new(vscode_missing_location::MissingAgentsLocation),
Box::new(vscode_ineffective_locations::IneffectivePluginLocations),
```

This brings the total marketplace rules from 11 to 17.

---

## 6. Alternatives Considered

| Option | Pros | Cons | Reason for Rejection |
|--------|------|------|---------------------|
| Keep as external repo-specific scripts | No aipm changes needed | Not portable to other repos | Defeats the purpose of aipm as a universal tool |
| Add a new `.vscode` source type | Clean separation of concerns | Requires new factory function, new dispatch arm, more pipeline changes | Over-engineering — these rules validate marketplace-plugin consistency, not VS Code config quality |
| Parse `plugin.json` into typed Rust struct | Type safety, serde validation | Additional maintenance burden, only 3 fields currently used | `serde_json::Value` is sufficient and matches existing `hook_unknown_event.rs` pattern |
| Use JSONC parser for `.vscode/settings.json` | Handles comments in settings files | Additional dependency, most settings.json files don't use comments | Silent skip on parse failure is acceptable; can revisit if needed |

---

## 7. Cross-Cutting Concerns

### 7.1 Backward Compatibility

All new rules are additive. Existing `aipm lint` behavior is unchanged. Users can suppress any new rule via `[workspace.lints]` in `aipm.toml`:

```toml
[workspace.lints]
"plugin/missing-manifest" = "allow"   # suppress entirely
"vscode/missing-skills-location" = "error"  # upgrade to error
```

### 7.2 Performance

`scan_plugin_dirs()` traverses the same `.ai/<plugin>/` directory listing that `scan_skills()`, `scan_agents()`, and `scan_hook_files()` already traverse. Each rule independently calls `scan_plugin_dirs()`, resulting in redundant filesystem reads. This matches the existing pattern (every skill rule independently calls `scan_skills()`). Caching can be added later if profiling shows a bottleneck.

### 7.3 Error Handling

Following the project's lint conventions:
- Filesystem errors (can't read directory) → silently skip, return empty diagnostics
- JSON parse errors → silently skip (no diagnostics), except for `plugin/banned-mcp-servers` which uses text-level detection
- No `.unwrap()`, `.expect()`, `panic!()`, or `#[allow(..)]` per `CLAUDE.md` lint policy

---

## 8. Migration, Rollout, and Testing

### 8.1 Test Plan

Each rule file includes a `#[cfg(test)] mod tests` block using `test_helpers::MockFs`. Test patterns follow existing rules like [`broken_paths.rs`](../crates/libaipm/src/lint/rules/broken_paths.rs):

**Standard test cases per rule:**
1. **Happy path**: valid input → empty diagnostics
2. **Finding produced**: invalid input → correct diagnostic count, rule_id, message substring
3. **Empty `.ai/` dir**: no plugins → no diagnostics
4. **Graceful degradation**: missing files/dirs → no crash, empty diagnostics
5. **Edge cases**: multiple plugins with mixed validity, infrastructure dirs skipped

**Scanner tests (`scan.rs`):**
- Empty marketplace, nonexistent marketplace, plugin is file not dir
- Plugin with/without plugin.json, skills/, agents/
- `.claude-plugin` infrastructure dir filtered out

**MockFs helper tests (`test_helpers.rs`):**
- `add_plugin_json()` creates correct paths and doesn't duplicate dir entries
- `add_vscode_settings()` creates at correct relative path

### 8.2 Verification Commands

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check

# Coverage gate
cargo +nightly llvm-cov clean --workspace
cargo +nightly llvm-cov --no-report --workspace --branch
cargo +nightly llvm-cov --no-report --doc
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)'
```

---

## 9. Open Questions / Unresolved Issues

- [ ] `repo/banned-mcp-config` flags `.vscode/mcp.json`, which is a legitimate VS Code MCP config file in non-marketplace repos. This is already gated (only fires when `.ai/` is detected since it's a marketplace rule). Warning severity seems appropriate — teams can suppress it via `[workspace.lints]` if they intentionally have both.
- [ ] Should `vscode/missing-skills-location` and `vscode/missing-agents-location` also check `.github/` and `.claude/` source paths in addition to `.ai/<plugin>/` entries? For v1, we only check `.ai/<plugin>/` entries.

### Resolved

- **Undeclared component rules cut.** The `plugin.json` schema differs between Claude Code plugins and aipm plugins (which use `aipm.toml`). These fields aren't consistently useful yet, and aipm needs to define its own component declaration model before enforcing it.
- **Stale/orphan pluginLocations rules replaced** with a single `vscode/ineffective-plugin-locations` warning. `chat.pluginLocations` only works at VS Code user level — having it in repo `.vscode/settings.json` has no effect.

---

## 10. Planned Implementation Files

*These changes are expected in a follow-up implementation PR, not in this spec PR.*

### Modified Files

| File | Change |
|------|--------|
| `crates/libaipm/src/lint/rules/scan.rs` | Add `FoundPluginDir` struct, `scan_plugin_dirs()` function, and tests |
| `crates/libaipm/src/lint/rules/test_helpers.rs` | Add `add_plugin_json()`, `add_vscode_settings()` helpers and tests |
| `crates/libaipm/src/lint/rules/mod.rs` | Add 5 `pub mod` declarations and 6 `Box::new()` entries in `for_marketplace()` |

### New Files

| File | Rule ID(s) | Structs |
|------|------------|---------|
| `crates/libaipm/src/lint/rules/plugin_missing_manifest.rs` | `plugin/missing-manifest` | `MissingManifest` |
| `crates/libaipm/src/lint/rules/plugin_banned_mcp_servers.rs` | `plugin/banned-mcp-servers` | `BannedMcpServers` |
| `crates/libaipm/src/lint/rules/repo_banned_mcp_config.rs` | `repo/banned-mcp-config` | `BannedMcpConfig` |
| `crates/libaipm/src/lint/rules/vscode_missing_location.rs` | `vscode/missing-skills-location`, `vscode/missing-agents-location` | `MissingSkillsLocation`, `MissingAgentsLocation` |
| `crates/libaipm/src/lint/rules/vscode_ineffective_locations.rs` | `vscode/ineffective-plugin-locations` | `IneffectivePluginLocations` |
