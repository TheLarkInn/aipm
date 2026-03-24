---
date: 2026-03-24
researcher: Claude
git_commit: 7d9847c348c127f697d29f536cb58fddfd5762e7
branch: main
repository: aipm
topic: "Bug: aipm migrate produces double-double-quotes in plugin.json description"
tags: [research, bug, migrate, plugin-json, frontmatter-parsing]
status: complete
last_updated: 2026-03-24
last_updated_by: Claude
---

# Research: Double-Double-Quotes Bug in `aipm migrate`

## Research Question

When `aipm migrate` produces a `plugin.json`, the description field contains
double-double-quotes:

```json
{
  "description": ""Analyze bugs by reading bug reports and investigating the codebase to determine root cause.""
}
```

Expected output should be:

```json
{
  "description": "Analyze bugs by reading bug reports and investigating the codebase to determine root cause."
}
```

## Summary

The bug is caused by the YAML frontmatter parsers in all four detectors not
stripping surrounding quotes from YAML values. When a `.md` file has
`description: "Some text"`, the parser stores `"Some text"` (with literal quotes)
in `ArtifactMetadata.description`. The emitter then interpolates this into
`"description": "{description}"` via `format!()`, producing double-double-quotes.

The same bug also affects `aipm.toml` generation (`description = ""...""`)
and potentially the `name` field, which uses the identical parsing pattern.

## Detailed Findings

### Component 1: Frontmatter Parsing (Root Cause)

All four detectors parse YAML frontmatter with a simple `strip_prefix` +
`.trim()` approach. None strip YAML-style surrounding quotes:

**skill_detector.rs:108-109**
```rust
} else if let Some(value) = trimmed_line.strip_prefix("description:") {
    metadata.description = Some(value.trim().to_string());
}
```

Identical pattern in:
- `agent_detector.rs:89-90`
- `command_detector.rs:87-88`
- `output_style_detector.rs:83-84`

Given YAML frontmatter `description: "Analyze bugs..."`, after
`strip_prefix("description:")` the value is ` "Analyze bugs..."`, and after
`.trim()` it is `"Analyze bugs..."` — the double quotes are preserved as
literal characters.

The `name:` field uses the same pattern and has the same vulnerability.

### Component 2: JSON Emission (Manifestation in plugin.json)

`emitter.rs:906-931` — `generate_plugin_json_multi()`:

```rust
let description =
    metadata.description.as_deref().unwrap_or("Migrated from .claude/ configuration");
// ...
format!(
    "{{\n  \"name\": \"{name}\",\n  \"version\": \"0.1.0\",\n  \
     \"description\": \"{description}\"{fields}\n}}\n"
)
```

When `description` = `"Analyze bugs..."` (with literal quotes), the output is:
`"description": ""Analyze bugs...""` — invalid JSON.

### Component 3: TOML Emission (Same bug in aipm.toml)

`emitter.rs:835-889` — `generate_plugin_manifest()`:

```rust
let description =
    artifact.metadata.description.as_deref().unwrap_or("Migrated from .claude/ configuration");
// ...
format!("... description = \"{description}\"\n ...")
```

Produces `description = ""Analyze bugs...""` — invalid TOML.

`emitter.rs:593-596, 675-685` — `generate_package_manifest()`:

Same pattern, same bug for multi-artifact packages.

### Unaffected Code Paths

- `mcp_detector.rs:45` — constructs description programmatically (no quotes)
- `hook_detector.rs:52` — hardcoded string (no quotes)
- `registrar.rs:36-41` — uses `serde_json::json!()` with a hardcoded string
- `workspace_init/mod.rs:272-279` — hardcoded string literal

### Existing Tests (Gap)

Current tests use unquoted descriptions:
- `emitter.rs:1336-1343` — `generate_plugin_json_with_description` uses `"Test desc"`
- `skill_detector.rs:295-317` — test frontmatter uses `description: Deploy app` (no quotes)

No tests exercise quoted YAML values like `description: "Deploy app"`, which is
why the bug was not caught.

## Code References

- `crates/libaipm/src/migrate/skill_detector.rs:108-109` — description parsing (skill)
- `crates/libaipm/src/migrate/agent_detector.rs:89-90` — description parsing (agent)
- `crates/libaipm/src/migrate/command_detector.rs:87-88` — description parsing (command)
- `crates/libaipm/src/migrate/output_style_detector.rs:83-84` — description parsing (output style)
- `crates/libaipm/src/migrate/emitter.rs:906-931` — `generate_plugin_json_multi()` (JSON)
- `crates/libaipm/src/migrate/emitter.rs:835-889` — `generate_plugin_manifest()` (TOML)
- `crates/libaipm/src/migrate/emitter.rs:579-686` — `generate_package_manifest()` (TOML)
- `crates/libaipm/src/migrate/mod.rs:52-65` — `ArtifactMetadata` struct definition

## Architecture Documentation

The migrate pipeline has three stages:
1. **Detection** — detectors parse `.md` frontmatter or config files
2. **Emission** — emitter generates `plugin.json` and `aipm.toml` via `format!()`
3. **Registration** — registrar generates workspace plugin list

All output files (JSON and TOML) are generated with raw string interpolation,
not with serialization libraries (`serde_json`, `toml`). This means any special
characters in the description (or name) can produce invalid output.

## Scope of the Bug

The bug is triggered when any YAML frontmatter description is wrapped in quotes.
This is standard YAML practice, especially for descriptions containing special
characters (colons, commas, etc.). Both `plugin.json` and `aipm.toml` are
affected. The `name` field has the same vulnerability.

## Open Questions

1. Should the fix also escape characters that are special in JSON/TOML (e.g.,
   backslashes, newlines) in addition to stripping quotes? The current `format!()`
   approach has no escaping at all.
2. Would it be better long-term to switch to `serde_json` / `toml` serialization
   instead of hand-built string templates?
