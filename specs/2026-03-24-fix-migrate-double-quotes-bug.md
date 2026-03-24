# Fix Double-Quotes Bug in `aipm migrate` Emission

| Document Metadata      | Details                    |
| ---------------------- | -------------------------- |
| Author(s)              | selarkin                   |
| Status                 | Draft (WIP)                |
| Team / Owner           | aipm                       |
| Created / Last Updated | 2026-03-24 / 2026-03-24   |

## 1. Executive Summary

`aipm migrate` produces invalid `plugin.json` and `aipm.toml` files when YAML
frontmatter descriptions contain quotes (e.g., `description: "Deploy app"`).
The hand-built `format!()` string interpolation passes raw strings into JSON/TOML
templates without escaping, producing `"description": ""Deploy app""`. This spec
replaces the `format!()`-based emission with `serde_json` and `toml` crate
serialization, and also strips YAML quotes in the frontmatter parsers to keep
stored values clean.

**Research reference:** [research/docs/2026-03-24-migrate-double-quotes-bug.md](../research/docs/2026-03-24-migrate-double-quotes-bug.md)

## 2. Context and Motivation

### 2.1 Current State

The migrate emitter in `crates/libaipm/src/migrate/emitter.rs` generates
`plugin.json` and `aipm.toml` via `format!()` string interpolation. User-supplied
values (name, description) are spliced into templates without any escaping:

```rust
// plugin.json (emitter.rs:927-929)
format!(
    "{{\n  \"name\": \"{name}\",\n  \"version\": \"0.1.0\",\n  \
     \"description\": \"{description}\"{fields}\n}}\n"
)

// aipm.toml (emitter.rs:879-885)
format!(
    "[package]\nname = \"{plugin_name}\"\n...\ndescription = \"{description}\"\n..."
)
```

The frontmatter parsers in all four detectors (`skill_detector.rs`,
`agent_detector.rs`, `command_detector.rs`, `output_style_detector.rs`) parse
YAML values with `strip_prefix("description:") + .trim()` but do not strip
surrounding quotes. YAML `description: "Deploy app"` is stored as
`"Deploy app"` (with literal quote characters).

### 2.2 The Problem

- **User Impact:** Migrated `plugin.json` files contain invalid JSON that will
  fail to parse. Migrated `aipm.toml` files contain invalid TOML.
- **Root Cause (parser):** Frontmatter parsers preserve YAML quote delimiters as
  literal characters in the stored `description` and `name` values.
- **Root Cause (emitter):** The emitter uses `format!()` string interpolation
  instead of proper serialization, so any special characters (quotes, backslashes,
  newlines) in user-provided values produce invalid output.
- **Scope:** Both `plugin.json` and `aipm.toml` are affected. The `name` field
  uses the same parsing pattern and has the same vulnerability.

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [x] `plugin.json` output is always valid JSON, regardless of what characters
  appear in the frontmatter description or name.
- [x] `aipm.toml` output is always valid TOML, regardless of what characters
  appear in the frontmatter description or name.
- [x] YAML-quoted values in frontmatter are stored without surrounding quote
  delimiters in `ArtifactMetadata`.
- [x] Output format (field order, indentation) is visually comparable to the
  current output for common cases (no regressions in readability).
- [x] All existing tests continue to pass after the change.
- [x] New tests cover quoted descriptions, descriptions with special characters
  (backslashes, newlines, colons), and the `name` field with quotes.
- [x] Branch coverage remains >= 89%.

### 3.2 Non-Goals (Out of Scope)

- [x] We will NOT switch the frontmatter parsers to a full YAML library — the
  simple line-by-line parser is adequate for the frontmatter format used.
- [x] We will NOT change the `workspace_init` plugin.json generation — it uses
  hardcoded string literals that cannot contain user input.
- [x] We will NOT refactor the `registrar.rs` emission — it already uses
  `serde_json::json!()` correctly.
- [x] We will NOT add `Serialize` derives to `ArtifactMetadata` or `Artifact` —
  the emitter functions build small purpose-built serializable structs internally.

## 4. Proposed Solution (High-Level Design)

Two complementary changes:

1. **Frontmatter parsers** — Add a `strip_yaml_quotes()` helper that strips
   matching surrounding `"..."` or `'...'` delimiters from parsed values. Apply
   it to both `name:` and `description:` in all four detectors.

2. **Emitter functions** — Replace `format!()` string interpolation with
   `serde_json` (for `plugin.json`) and `toml` (for `aipm.toml`) serialization.

### 4.1 Key Components

| Component | Change | Files |
|-----------|--------|-------|
| YAML quote stripping | New `strip_yaml_quotes()` helper | `emitter.rs` or new shared util |
| Frontmatter parsers | Apply `strip_yaml_quotes()` to `name:` and `description:` | `skill_detector.rs`, `agent_detector.rs`, `command_detector.rs`, `output_style_detector.rs` |
| JSON emission | Replace `format!()` with `serde_json::json!()` + `to_string_pretty()` | `emitter.rs` (`generate_plugin_json_multi`) |
| TOML emission | Replace `format!()` with `toml::to_string_pretty()` using `Serialize` structs | `emitter.rs` (`generate_plugin_manifest`, `generate_package_manifest`) |

## 5. Detailed Design

### 5.1 YAML Quote Stripping Helper

Add a shared helper function. Since the frontmatter parsing is in the `migrate`
module, place it in `emitter.rs` (already used by detectors indirectly) or in
`mod.rs` where `ArtifactMetadata` is defined. Prefer `mod.rs` since it is
imported by all detectors.

```rust
// In crates/libaipm/src/migrate/mod.rs

/// Strip matching surrounding YAML quote delimiters from a scalar value.
///
/// Handles both double-quoted (`"..."`) and single-quoted (`'...'`) YAML scalars.
/// Returns the inner content if delimiters match, otherwise returns the input unchanged.
pub(crate) fn strip_yaml_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &s[1..s.len() - 1];
        }
    }
    s
}
```

### 5.2 Frontmatter Parser Changes

In all four detectors, update `name:` and `description:` parsing to call
`strip_yaml_quotes()`:

**Before** (all four detectors):
```rust
if let Some(value) = trimmed_line.strip_prefix("name:") {
    metadata.name = Some(value.trim().to_string());
} else if let Some(value) = trimmed_line.strip_prefix("description:") {
    metadata.description = Some(value.trim().to_string());
}
```

**After:**
```rust
use super::strip_yaml_quotes;

if let Some(value) = trimmed_line.strip_prefix("name:") {
    metadata.name = Some(strip_yaml_quotes(value.trim()).to_string());
} else if let Some(value) = trimmed_line.strip_prefix("description:") {
    metadata.description = Some(strip_yaml_quotes(value.trim()).to_string());
}
```

**Files to change:**
- `crates/libaipm/src/migrate/skill_detector.rs:106-109`
- `crates/libaipm/src/migrate/agent_detector.rs:87-90`
- `crates/libaipm/src/migrate/command_detector.rs:85-88`
- `crates/libaipm/src/migrate/output_style_detector.rs:81-84`

### 5.3 JSON Emission (`generate_plugin_json_multi`)

Replace the `format!()` template with `serde_json::json!()` and
`serde_json::to_string_pretty()`.

**Current** (`emitter.rs:901-931`):
```rust
fn generate_plugin_json_multi(
    name: &str,
    metadata: &ArtifactMetadata,
    kinds: &[ArtifactKind],
) -> String {
    let description =
        metadata.description.as_deref().unwrap_or("Migrated from .claude/ configuration");
    let mut fields = String::new();
    // ... conditionally append field strings ...
    format!(
        "{{\n  \"name\": \"{name}\",\n  \"version\": \"0.1.0\",\n  \
         \"description\": \"{description}\"{fields}\n}}\n"
    )
}
```

**Proposed:**
```rust
fn generate_plugin_json_multi(
    name: &str,
    metadata: &ArtifactMetadata,
    kinds: &[ArtifactKind],
) -> String {
    let description =
        metadata.description.as_deref().unwrap_or("Migrated from .claude/ configuration");

    // Use an ordered map to preserve deterministic field order
    let mut map = serde_json::Map::new();
    map.insert("name".to_string(), serde_json::Value::String(name.to_string()));
    map.insert("version".to_string(), serde_json::Value::String("0.1.0".to_string()));
    map.insert(
        "description".to_string(),
        serde_json::Value::String(description.to_string()),
    );

    let distinct: HashSet<&ArtifactKind> = kinds.iter().collect();
    if distinct.contains(&ArtifactKind::Skill) || distinct.contains(&ArtifactKind::Command) {
        map.insert(
            "skills".to_string(),
            serde_json::Value::String("./skills/".to_string()),
        );
    }
    if distinct.contains(&ArtifactKind::Agent) {
        map.insert(
            "agents".to_string(),
            serde_json::Value::String("./agents/".to_string()),
        );
    }
    if distinct.contains(&ArtifactKind::McpServer) {
        map.insert(
            "mcpServers".to_string(),
            serde_json::Value::String("./.mcp.json".to_string()),
        );
    }
    if distinct.contains(&ArtifactKind::Hook) {
        map.insert(
            "hooks".to_string(),
            serde_json::Value::String("./hooks/hooks.json".to_string()),
        );
    }
    if distinct.contains(&ArtifactKind::OutputStyle) {
        map.insert(
            "outputStyles".to_string(),
            serde_json::Value::String("./".to_string()),
        );
    }

    let obj = serde_json::Value::Object(map);
    // to_string_pretty uses 2-space indent by default, matching current output
    let mut output = serde_json::to_string_pretty(&obj).unwrap_or_default();
    output.push('\n');
    output
}
```

**Note on `unwrap_or_default()`:** `serde_json::to_string_pretty` can only fail
if the value contains non-string map keys (impossible with `serde_json::Map`) or
if a custom serializer fails (not applicable). So this call cannot actually fail.
Using `unwrap_or_default()` avoids `unwrap()` which is denied by lint policy.

**Note on field order:** `serde_json::Map` preserves insertion order (it uses
`IndexMap` under the `preserve_order` feature, which is the default in
serde_json). This means the output field order matches the insertion order above,
which matches the current output. If `preserve_order` is not enabled, the output
will be sorted alphabetically by key. Check the serde_json dependency — the
workspace already depends on `serde_json = "1"`. We should verify that
`preserve_order` is not needed for our case, since the current tests check for
content presence (not exact output), and JSON consumers shouldn't depend on field
order. If exact field order matters, add `features = ["preserve_order"]` to the
serde_json workspace dependency.

### 5.4 TOML Emission (`generate_plugin_manifest`)

Replace the `format!()` template with `Serialize` structs and
`toml::to_string_pretty()`. Define local structs within the function (or at
module level) to model the TOML structure.

**Current** (`emitter.rs:831-889`):
```rust
fn generate_plugin_manifest(artifact: &Artifact, plugin_name: &str) -> String {
    // ... manual format!() string building ...
}
```

**Proposed structs** (module-level in `emitter.rs`):

```rust
use serde::Serialize;

#[derive(Serialize)]
struct PluginToml {
    package: PluginPackage,
    components: PluginComponents,
}

#[derive(Serialize)]
struct PluginPackage {
    name: String,
    version: String,
    #[serde(rename = "type")]
    kind: String,
    edition: String,
    description: String,
}

#[derive(Default, Serialize)]
struct PluginComponents {
    #[serde(skip_serializing_if = "Option::is_none")]
    skills: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agents: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mcp_servers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hooks: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_styles: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scripts: Option<Vec<String>>,
}
```

**Updated `generate_plugin_manifest`:**
```rust
fn generate_plugin_manifest(artifact: &Artifact, plugin_name: &str) -> String {
    let type_str = artifact.kind.to_type_string();
    let description =
        artifact.metadata.description.as_deref().unwrap_or("Migrated from .claude/ configuration");

    let mut components = PluginComponents::default();

    match artifact.kind {
        ArtifactKind::Skill | ArtifactKind::Command => {
            components.skills =
                Some(vec![format!("skills/{}/SKILL.md", artifact.name)]);
        },
        ArtifactKind::Agent => {
            components.agents = Some(vec![format!("agents/{}.md", artifact.name)]);
        },
        ArtifactKind::McpServer => {
            components.mcp_servers = Some(vec![".mcp.json".to_string()]);
        },
        ArtifactKind::Hook => {
            components.hooks = Some(vec!["hooks/hooks.json".to_string()]);
        },
        ArtifactKind::OutputStyle => {
            components.output_styles = Some(vec![format!("{}.md", artifact.name)]);
        },
    }

    // Scripts (if any)
    if !artifact.referenced_scripts.is_empty() {
        let scripts_root = Path::new("scripts");
        let scripts: Vec<String> = artifact
            .referenced_scripts
            .iter()
            .map(|p| {
                let relative = p.strip_prefix(scripts_root).unwrap_or(p);
                format!("scripts/{}", relative.to_string_lossy())
            })
            .collect();
        components.scripts = Some(scripts);
    }

    // Hooks from skill/command frontmatter
    if artifact.metadata.hooks.is_some() && artifact.kind != ArtifactKind::Hook {
        components.hooks = Some(vec!["hooks/hooks.json".to_string()]);
    }

    let manifest = PluginToml {
        package: PluginPackage {
            name: plugin_name.to_string(),
            version: "0.1.0".to_string(),
            kind: type_str.to_string(),
            edition: "2024".to_string(),
            description: description.to_string(),
        },
        components,
    };

    toml::to_string_pretty(&manifest).unwrap_or_default()
}
```

**Updated `generate_package_manifest`:** Same pattern — build `PluginToml` from
the multi-artifact parameters and serialize. The logic for grouping component
paths by type remains the same, just filling in `PluginComponents` fields instead
of writing strings.

### 5.5 TOML Output Format Compatibility

`toml::to_string_pretty()` produces slightly different formatting than the
hand-built strings:
- May add a blank line between `[package]` and `[components]` differently
- Uses `\n` for arrays rather than inline `[...]` for long arrays

The current inline array format (e.g., `skills = ["skills/deploy/SKILL.md"]`)
may change to a multi-line format for longer arrays. This is acceptable — both
are valid TOML.

However, some existing tests check for exact string matches like
`contains("skills = [\"skills/deploy/SKILL.md\"]")`. These tests may need minor
updates to accommodate `toml` crate formatting. Alternatively, the tests can
parse the TOML and check the values programmatically.

### 5.6 `serde` Dependency

The `serde` crate with `derive` is already a workspace dependency
(`Cargo.toml:32`). The `Serialize` derive will work with the existing deny lint
policy — serde's derive macros are explicitly permitted to emit internal
`#[allow]` attributes per the `CLAUDE.md` lint policy.

## 6. Alternatives Considered

| Option | Pros | Cons | Decision |
|--------|------|------|----------|
| A: Strip quotes only (no serialization change) | Minimal diff, fixes the immediate bug | Still vulnerable to backslashes, newlines, or other special characters in descriptions | Rejected: doesn't eliminate the class of bugs |
| B: Manual JSON/TOML escaping functions | Small diff, no new structs | Reimplements what serde_json/toml already do; error-prone | Rejected: wheel reinvention |
| C: serde_json + toml serialization (selected) | Eliminates the entire class of escaping bugs; uses battle-tested libraries already in deps | Slightly larger diff; may change output formatting | **Selected**: robust and idiomatic |

## 7. Cross-Cutting Concerns

### 7.1 Output Format Stability

Downstream tools or tests that parse `plugin.json` or `aipm.toml` by exact
string matching may break if the formatting changes. Mitigation:
- `serde_json::to_string_pretty` uses 2-space indentation, same as the current
  hand-built output.
- `toml::to_string_pretty` may differ slightly in whitespace. Tests that do exact
  string comparisons should be updated to parse and compare values instead.

### 7.2 `serde_json::Map` Field Ordering

`serde_json::Map` uses `BTreeMap` by default (alphabetical order). If the
`preserve_order` feature is enabled, it uses `IndexMap` (insertion order).
The current output order is: `name`, `version`, `description`, then optional
component fields. Under alphabetical ordering, `description` would appear before
`name`. This is valid JSON but may surprise users diffing output.

**Recommendation:** Check if `preserve_order` is already enabled. If not, and if
field order matters for user experience, add it to the workspace dependency:
```toml
serde_json = { version = "1", features = ["preserve_order"] }
```

### 7.3 `toml` Crate TOML Struct Ordering

The `toml` crate serializes struct fields in declaration order. The `PluginToml`
struct should declare `package` before `components` to match the current output.
Within `PluginPackage`, fields should be declared in the desired output order:
`name`, `version`, `type`, `edition`, `description`.

## 8. Implementation Plan

### Phase 1: Core Fix (Single PR)

- [ ] **Step 1:** Add `strip_yaml_quotes()` to `crates/libaipm/src/migrate/mod.rs`
  with unit tests.
- [ ] **Step 2:** Update frontmatter parsers in all four detectors to call
  `strip_yaml_quotes()` on `name:` and `description:` values.
- [ ] **Step 3:** Add `PluginToml`, `PluginPackage`, `PluginComponents` structs
  with `Serialize` to `emitter.rs`.
- [ ] **Step 4:** Rewrite `generate_plugin_json_multi()` to use `serde_json::Map`
  and `to_string_pretty()`.
- [ ] **Step 5:** Rewrite `generate_plugin_manifest()` to use `PluginToml` +
  `toml::to_string_pretty()`.
- [ ] **Step 6:** Rewrite `generate_package_manifest()` similarly.
- [ ] **Step 7:** Update existing unit tests that assert exact string format
  to parse the output and check values instead.
- [ ] **Step 8:** Add new tests:
  - Quoted YAML description: `description: "Deploy app"` → JSON `"Deploy app"`
  - Single-quoted YAML description: `description: 'Deploy app'` → `"Deploy app"`
  - Description with special chars: backslash, newline, colon
  - Name with quotes: `name: "my-plugin"` → JSON `"my-plugin"`
  - Description with internal quotes: `description: She said "hello"`
  - E2E test: create a skill with quoted description, migrate, parse the resulting
    `plugin.json` as valid JSON, verify the description value.
- [ ] **Step 9:** Run full build/test/clippy/fmt/coverage pipeline.

### Test Plan

- **Unit Tests:** All existing `emitter.rs` unit tests updated + new tests for
  quoted values and special characters.
- **Unit Tests:** `strip_yaml_quotes()` tests in `mod.rs`.
- **Unit Tests:** Each detector gets a test with `description: "quoted text"` to
  verify quotes are stripped.
- **Integration Tests:** Existing BDD features in `tests/features/manifest/`.
- **E2E Tests:** New test in `migrate_e2e.rs` that creates a skill with
  `description: "Quoted description"`, runs migrate, reads the resulting
  `plugin.json`, and parses it with `serde_json::from_str` to verify it is valid
  JSON with the correct unquoted description.

## 9. Open Questions / Unresolved Issues

- [ ] Should we enable `serde_json`'s `preserve_order` feature to maintain
  the current field ordering in `plugin.json`? (Low stakes — JSON consumers
  should not depend on field order, but it affects human readability of diffs.)
- [ ] Should `workspace_init::generate_plugin_json()` also be migrated to
  `serde_json` for consistency, even though its hardcoded strings cannot trigger
  this bug? (Consistency vs. unnecessary churn.)
