---
date: 2026-03-24 17:30:47 PDT
researcher: Claude (Opus 4.6)
git_commit: d23e4b48db8f512ace3aa4513f53f230c9594e45
branch: fix/no-starter-enabledplugins-bug
repository: aipm
topic: "marketplace.json description does not match plugin.json description during aipm migrate"
tags: [research, codebase, migrate, marketplace, plugin-json, registrar, description, bug]
status: complete
last_updated: 2026-03-24
last_updated_by: Claude (Opus 4.6)
---

# Research

## Research Question
When using `aipm migrate`, the descriptions added to `marketplace.json` for each plugin don't match the description found in `plugin.json`. These should be the same values.

## Summary

This is a confirmed bug. The `registrar::register_plugins()` function in `registrar.rs` hardcodes the description `"Migrated from .claude/ configuration"` for every plugin entry written to `marketplace.json`. Meanwhile, the emitter's `generate_plugin_json()` function correctly uses the actual description from `ArtifactMetadata.description` (extracted from SKILL.md/command/agent frontmatter) when writing `plugin.json`. The root cause is that the registrar only receives plugin names (`&[String]`), not descriptions.

## Detailed Findings

### The Bug: Hardcoded Description in Registrar

**File**: `crates/libaipm/src/migrate/registrar.rs:36-40`

The `register_plugins()` function hardcodes the description for every marketplace entry:

```rust
plugins.push(serde_json::json!({
    "name": name,
    "source": format!("./{name}"),
    "description": "Migrated from .claude/ configuration"
}));
```

The function signature at line 10 only accepts names:

```rust
pub fn register_plugins(ai_dir: &Path, plugin_names: &[String], fs: &dyn Fs) -> Result<(), Error>
```

### Correct Behavior: Emitter Uses Real Descriptions

**File**: `crates/libaipm/src/migrate/emitter.rs:920-921`

The `generate_plugin_json_multi()` function correctly reads the actual description:

```rust
let description =
    metadata.description.as_deref().unwrap_or("Migrated from .claude/ configuration");
```

This means `plugin.json` gets the real description (e.g., "Deploy app") while `marketplace.json` always gets `"Migrated from .claude/ configuration"`.

### Call Sites in the Migrate Pipeline

**File**: `crates/libaipm/src/migrate/mod.rs:294`

Single-plugin migrate path:
```rust
registrar::register_plugins(ai_dir, &registered_names, fs)?;
```

**File**: `crates/libaipm/src/migrate/mod.rs:423`

Recursive migrate path:
```rust
registrar::register_plugins(ai_dir, &registered_names, fs)?;
```

Both call sites only pass `&registered_names` (a `Vec<String>` of plugin names). The descriptions from `ArtifactMetadata` are available earlier in the pipeline but are not forwarded to the registrar.

### Description Source Chain

1. **Extraction**: YAML frontmatter in `.claude/skills/*/SKILL.md`, `.claude/commands/*.md`, `.claude/agents/*.md` — parsed via `strip_prefix("description:")` + `strip_yaml_quotes()` in each detector
2. **Storage**: `ArtifactMetadata.description: Option<String>` (`mod.rs:57`)
3. **plugin.json**: `generate_plugin_json_multi()` uses `metadata.description` with fallback (`emitter.rs:920-921`)
4. **aipm.toml**: `generate_plugin_manifest()` also uses `metadata.description` with fallback (`emitter.rs:850-851`)
5. **marketplace.json**: `register_plugins()` ignores all descriptions, hardcodes fallback (`registrar.rs:39`)

### Description Extraction by Detector Type

| Detector | Source | File:Line |
|----------|--------|-----------|
| SkillDetector | `description:` in SKILL.md YAML frontmatter | `skill_detector.rs:108-109` |
| CommandDetector | `description:` in command .md YAML frontmatter | `command_detector.rs:87-88` |
| AgentDetector | `description:` in agent .md YAML frontmatter | `agent_detector.rs:89-90` |
| OutputStyleDetector | `description:` in output-style .md YAML frontmatter | `output_style_detector.rs:83-84` |
| McpDetector | Hardcoded: `"{N} MCP server(s) from .mcp.json"` | `mcp_detector.rs:45,55` |
| HookDetector | Hardcoded: `"Hooks extracted from .claude/settings.json"` | `hook_detector.rs:52` |

### Existing Tests

Tests exist for `plugin.json` description handling but none verify that `marketplace.json` uses the same description:

- `emitter.rs:1357` — `generate_plugin_json_with_description()` — confirms plugin.json uses metadata description
- `emitter.rs:1367` — `generate_plugin_json_no_description()` — confirms fallback
- `emitter.rs:2412` — `generate_plugin_json_description_with_special_chars()` — special character handling
- `registrar.rs:124-177` — all registrar tests only check name/source presence, not description accuracy

## Code References

- `crates/libaipm/src/migrate/registrar.rs:10` — `register_plugins()` function signature (only accepts names)
- `crates/libaipm/src/migrate/registrar.rs:36-40` — hardcoded description in marketplace entry
- `crates/libaipm/src/migrate/emitter.rs:910-952` — `generate_plugin_json` / `generate_plugin_json_multi` (uses real description)
- `crates/libaipm/src/migrate/emitter.rs:847-904` — `generate_plugin_manifest` (uses real description for aipm.toml)
- `crates/libaipm/src/migrate/mod.rs:52-65` — `ArtifactMetadata` struct definition
- `crates/libaipm/src/migrate/mod.rs:294` — single-migrate call to registrar (names only)
- `crates/libaipm/src/migrate/mod.rs:423` — recursive-migrate call to registrar (names only)

## Architecture Documentation

The migrate pipeline flows: Discovery → Detection → Emission → Registration.

- **Discovery** (`discovery.rs`) finds `.claude/` directories
- **Detection** (various `*_detector.rs`) extracts `Artifact` structs containing `ArtifactMetadata` (with description)
- **Emission** (`emitter.rs`) writes `plugin.json` and `aipm.toml` using the full metadata
- **Registration** (`registrar.rs`) writes `marketplace.json` using only names — the metadata is lost at this stage

The architectural gap is that the registration step receives a reduced data type (`&[String]`) instead of the full artifact information.

## Historical Context (from research/)

- `research/docs/2026-03-24-migrate-all-artifact-types.md` — notes that the marketplace registrar could be made "type-aware" for migrated plugins, hinting at the known limitation
- `research/docs/2026-03-24-migrate-double-quotes-bug.md` — documents a prior bug in plugin.json description serialization (double-double-quotes), which was fixed in commit d72aed1
- `research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md` — documents the description flow for aipm.toml generation

## Related Research

- `research/docs/2026-03-24-migrate-double-quotes-bug.md` — prior description-related bug in the same pipeline
- `research/docs/2026-03-24-migrate-all-artifact-types.md` — comprehensive migrate architecture documentation
- `research/docs/2026-03-20-scaffold-plugin-ts-missing-features.md` — scaffold-plugin.ts marketplace registration

## Open Questions

- Should the registrar also propagate other metadata beyond description (e.g., version, author)?
- For package plugins (multiple artifacts bundled), which artifact's description should be used for the marketplace entry?
