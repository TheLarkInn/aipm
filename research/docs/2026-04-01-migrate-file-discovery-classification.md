---
title: "Migrate File Discovery and Classification System"
date: 2026-04-01
category: implementation-analysis
status: complete
relates_to:
  - 2026-03-23-aipm-migrate-command.md
  - 2026-03-23-recursive-claude-discovery-parallel-migrate.md
  - 2026-03-24-migrate-all-artifact-types.md
  - 2026-03-28-copilot-cli-migrate-adapter.md
---

# Migrate File Discovery and Classification System

## Overview

The `aipm migrate` command scans AI tool configuration directories (`.claude/` and `.github/`) for artifacts, classifies them by type using a set of detectors, and emits them as plugins into the `.ai/` marketplace directory. The pipeline has three stages: discovery (finding source directories), detection (classifying files within those directories), and emission (writing plugin output). This document covers the first two stages in detail.

## Architecture: Three-Stage Pipeline

```
Discovery (find source dirs) --> Detection (classify artifacts) --> Emission (write plugins)
```

The orchestrator lives in `mod.rs` and delegates to `discovery.rs` for the first stage and `detector.rs` + individual detector modules for the second stage.

---

## Stage 1: Discovery

### File: `/workspaces/aipm/crates/libaipm/src/migrate/discovery.rs`

### Structs

**`DiscoveredSource`** (lines 10-22):
- `source_dir: PathBuf` -- absolute path to the discovered directory (e.g., `/project/.claude/`)
- `source_type: String` -- which pattern matched (e.g., `".claude"`, `".github"`)
- `package_name: Option<String>` -- derived from the parent directory name; `None` if at the project root
- `relative_path: PathBuf` -- relative path from the project root to the parent of the source dir; empty for root-level sources

### Functions

**`discover_claude_dirs(project_root, max_depth)`** (lines 35-39):
Convenience wrapper that calls `discover_source_dirs` with patterns `[".claude"]`.

**`discover_source_dirs(project_root, patterns, max_depth)`** (lines 54-120):
The main discovery function. Uses the `ignore` crate (`ignore::WalkBuilder`) for gitignore-aware directory traversal.

Configuration at lines 59-68:
- `hidden(false)` -- must find hidden directories like `.claude/` and `.github/`
- `git_ignore(true)`, `git_global(true)`, `git_exclude(true)` -- respects `.gitignore` rules
- Optional `max_depth` limit

Filter at lines 70-76: Excludes the `.ai/` directory entirely to avoid scanning marketplace plugins.

Walking logic at lines 80-114:
- Iterates all entries from the walker
- Skips non-directory entries (line 84)
- Checks if the directory name matches any pattern in `patterns` (line 90)
- Derives `package_name` from the parent directory's final component (lines 102-106); returns `None` for root-level source dirs
- Results are sorted by path for deterministic output (line 117)

### How Discovery is Called

In `mod.rs` at line 409, recursive mode calls:
```
discovery::discover_source_dirs(dir, &[".claude", ".github"], max_depth)
```

In single-source mode (`migrate_single_source`, line 324), discovery is bypassed entirely -- the source directory is constructed directly from `dir.join(source)`.

---

## Stage 2: Detection

### The Detector Trait

### File: `/workspaces/aipm/crates/libaipm/src/migrate/detector.rs`

**`Detector` trait** (lines 13-19):
- `fn name(&self) -> &'static str` -- human-readable identifier
- `fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error>` -- scans `source_dir` and returns discovered artifacts

### Factory Functions

**`claude_detectors()`** (lines 23-31): Returns 6 detectors for `.claude/` sources:
1. `SkillDetector`
2. `CommandDetector`
3. `AgentDetector`
4. `McpDetector`
5. `HookDetector`
6. `OutputStyleDetector`

**`copilot_detectors()`** (lines 35-43): Returns 6 detectors for `.github/` sources:
1. `CopilotSkillDetector`
2. `CopilotAgentDetector`
3. `CopilotMcpDetector`
4. `CopilotHookDetector`
5. `CopilotExtensionDetector`
6. `CopilotLspDetector`

**`detectors_for_source(source_type)`** (lines 47-53): Dispatches based on source type string. Returns an empty `Vec` for unknown source types.

### ArtifactKind Enum

### File: `/workspaces/aipm/crates/libaipm/src/migrate/mod.rs`, lines 29-47

Complete list of variants:
| Variant | Description | `to_type_string()` |
|---------|-------------|---------------------|
| `Skill` | A skill from `.claude/skills/<name>/` or `.github/skills/<name>/` | `"skill"` |
| `Command` | A legacy command from `.claude/commands/<name>.md` | `"skill"` |
| `Agent` | A subagent from `agents/<name>.md` or `agents/<name>.agent.md` | `"agent"` |
| `McpServer` | MCP server configs from `.mcp.json` or `.copilot/mcp-config.json` | `"mcp"` |
| `Hook` | Hooks from `settings.json` or standalone `hooks.json` | `"hook"` |
| `OutputStyle` | An output style from `.claude/output-styles/<name>.md` | `"composite"` |
| `LspServer` | LSP server config from `lsp.json` | `"lsp"` |
| `Extension` | An extension from `.github/extensions/<name>/` | `"composite"` |

Note: `Skill` and `Command` both map to `"skill"` type string. `OutputStyle` and `Extension` both map to `"composite"`.

### Artifact Struct

### File: `/workspaces/aipm/crates/libaipm/src/migrate/mod.rs`, lines 96-110

- `kind: ArtifactKind` -- classification
- `name: String` -- artifact name
- `source_path: PathBuf` -- absolute path to the source
- `files: Vec<PathBuf>` -- files relative to `source_path`
- `referenced_scripts: Vec<PathBuf>` -- script paths referenced in the body
- `metadata: ArtifactMetadata` -- parsed frontmatter data

### ArtifactMetadata Struct

### File: `/workspaces/aipm/crates/libaipm/src/migrate/mod.rs`, lines 66-79

- `name: Option<String>` -- explicit name from frontmatter
- `description: Option<String>` -- description from frontmatter
- `hooks: Option<String>` -- raw YAML/JSON hooks block
- `model_invocation_disabled: bool` -- always `true` for commands
- `raw_content: Option<String>` -- raw file content for config-based artifacts (MCP, hooks, etc.)

---

## Individual Detectors: Claude Source (`.claude/`)

### SkillDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/skill_detector.rs`

**What it scans:** `<source_dir>/skills/` for subdirectories containing `SKILL.md`

**Detection logic (lines 19-59):**
1. Checks if `<source_dir>/skills/` exists; returns empty if not
2. Lists entries in `skills/` directory
3. Skips non-directory entries (line 29-31)
4. For each subdirectory, checks if `SKILL.md` exists (line 36)
5. Reads `SKILL.md`, parses frontmatter, collects all files recursively, extracts script references with prefix `${CLAUDE_SKILL_DIR}/`
6. Name: uses frontmatter `name` field if present, otherwise falls back to the directory name (line 46)
7. Emits `ArtifactKind::Skill`

**Files that match:** Only directories under `skills/` that contain a `SKILL.md` file. Directories without `SKILL.md` are silently skipped. Non-directory entries under `skills/` are silently skipped.

### CommandDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/command_detector.rs`

**What it scans:** `<source_dir>/commands/` for `.md` files

**Detection logic (lines 19-63):**
1. Checks if `<source_dir>/commands/` exists
2. Lists entries; skips directories (line 29) and non-`.md` files (lines 32-37, case-insensitive extension check)
3. Reads file content, parses optional frontmatter for `name` and `description`
4. Name derived from file stem (e.g., `review.md` -> `review`)
5. Always sets `model_invocation_disabled = true` (line 48)
6. Extracts script references using `${CLAUDE_SKILL_DIR}/` prefix
7. Emits `ArtifactKind::Command`

**Files that match:** Only `.md` files (case-insensitive) directly in the `commands/` directory. Non-`.md` files and subdirectories are silently skipped.

### AgentDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/agent_detector.rs`

**What it scans:** `<source_dir>/agents/` for `.md` files

**Detection logic (lines 18-57):**
1. Checks if `<source_dir>/agents/` exists
2. Lists entries; skips directories (line 28-30) and non-`.md` files (lines 31-34, case-insensitive)
3. Reads content, parses frontmatter for `name` and `description`
4. Name: frontmatter `name` if present, else file stem (`reviewer.md` -> `reviewer`)
5. Emits `ArtifactKind::Agent`

**Files that match:** Only `.md` files directly in `agents/`. Non-`.md` files and subdirectories are silently skipped.

### McpDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/mcp_detector.rs`

**What it scans:** `.mcp.json` at the **project root** (derived by going up one level from `source_dir`)

**Detection logic (lines 19-60):**
1. Derives project root from `source_dir.parent()` (line 21)
2. Checks for `<project_root>/.mcp.json`
3. Parses JSON; validates `mcpServers` key exists and is a non-empty object
4. Emits a single `ArtifactKind::McpServer` artifact named `"project-mcp-servers"` with the raw JSON content in `metadata.raw_content`

**Files that match:** Exactly one file: `.mcp.json` at the project root. Only produces an artifact if `mcpServers` is a non-empty object.

### HookDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/hook_detector.rs`

**What it scans:** `<source_dir>/settings.json`

**Detection logic (lines 18-57):**
1. Checks if `<source_dir>/settings.json` exists
2. Parses JSON; looks for a `hooks` key that is a non-empty object (line 31-33)
3. Wraps the hooks value in a `{"hooks": ...}` envelope
4. Extracts script references from command-type hooks by recursively walking the JSON structure (lines 63-105); identifies scripts by `./` prefix, directory separators, or known extensions (`.sh`, `.py`, `.js`)
5. Emits a single `ArtifactKind::Hook` artifact named `"project-hooks"`

**Important detail:** Only the `hooks` key is extracted from `settings.json`. Other keys in `settings.json` (e.g., `permissions`) are not migrated and are not tracked as unclassified.

### OutputStyleDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/output_style_detector.rs`

**What it scans:** `<source_dir>/output-styles/` for `.md` files

**Detection logic (lines 18-57):**
1. Checks if `<source_dir>/output-styles/` exists
2. Lists entries; skips directories and non-`.md` files (case-insensitive)
3. Reads content, parses optional frontmatter for `name` and `description`
4. Name: frontmatter name, or file stem fallback
5. Emits `ArtifactKind::OutputStyle`

**Files that match:** Only `.md` files directly in `output-styles/`. Non-`.md` files and subdirectories are silently skipped.

---

## Individual Detectors: Copilot Source (`.github/`)

### CopilotSkillDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/copilot_skill_detector.rs`

**What it scans:** `<source_dir>/skills/` for subdirectories containing `SKILL.md`

**Detection logic (lines 21-65):** Same structure as `SkillDetector`, but extracts script references with both `${SKILL_DIR}/` and `${CLAUDE_SKILL_DIR}/` prefixes (lines 47-50). Emits `ArtifactKind::Skill`.

### CopilotAgentDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/copilot_agent_detector.rs`

**What it scans:** `<source_dir>/agents/` for `.md` and `.agent.md` files

**Detection logic (lines 21-89):**
1. Lists entries in `agents/`; skips directories and non-`.md` files
2. Implements dedup: when both `foo.md` and `foo.agent.md` exist, `.agent.md` takes precedence (lines 41-57)
3. Stem extraction: for `.agent.md` files, strips the `.agent.md` suffix; for plain `.md`, uses `file_stem()` (lines 42-48)
4. Stores raw content in `metadata.raw_content`
5. Emits `ArtifactKind::Agent`

### CopilotMcpDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/copilot_mcp_detector.rs`

**What it scans:** `.copilot/mcp-config.json` at the project root

**Detection logic (lines 21-61):**
1. Derives project root from `source_dir.parent()`
2. Checks `<project_root>/.copilot/mcp-config.json`
3. Parses JSON; validates `mcpServers` key is a non-empty object
4. Emits `ArtifactKind::McpServer` named `"copilot-mcp-servers"` with raw content passthrough

### CopilotHookDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/copilot_hook_detector.rs`

**What it scans:** `hooks.json` at `<source_dir>/hooks.json` or `<source_dir>/hooks/hooks.json`

**Detection logic (lines 18-68):**
1. Checks `<source_dir>/hooks.json` first, then `<source_dir>/hooks/hooks.json` as fallback (lines 20-29; root takes priority)
2. Parses JSON; validates it is a non-empty object
3. Normalizes legacy Copilot event names to canonical camelCase (lines 72-85, e.g., `"SessionStart"` -> `"sessionStart"`, `"Stop"` -> `"agentStop"`)
4. Merges arrays when both legacy and canonical keys map to the same canonical name (lines 99-104)
5. Extracts script references using the same logic as `HookDetector`
6. Emits `ArtifactKind::Hook` named `"copilot-hooks"`

### CopilotExtensionDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/copilot_extension_detector.rs`

**What it scans:** `<source_dir>/extensions/` for subdirectories

**Detection logic (lines 19-55):**
1. Checks if `<source_dir>/extensions/` exists
2. Lists entries; skips non-directory entries (line 29)
3. For each subdirectory, collects all files recursively
4. Attempts to read a config file from candidates: `config.json`, `extension.json`, `manifest.json`, plus `.yaml`/`.yml` variants (lines 60-79)
5. Emits `ArtifactKind::Extension` with the directory name as the artifact name

### CopilotLspDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/copilot_lsp_detector.rs`

**What it scans:** `lsp.json` at `<source_dir>/lsp.json`, with fallback to `<project_root>/.github/lsp.json`

**Detection logic (lines 20-72):**
1. Checks `<source_dir>/lsp.json` first
2. Falls back to `<project_root>/.github/lsp.json` (line 25)
3. Parses JSON; validates it is a non-empty object
4. Emits `ArtifactKind::LspServer` named `"copilot-lsp-servers"` with raw content

---

## Shared Utilities

### File: `/workspaces/aipm/crates/libaipm/src/migrate/skill_common.rs`

**`parse_skill_frontmatter(content, path)`** (lines 13-77):
Line-by-line YAML frontmatter parser (no YAML library). Looks for `---` delimiters. Extracts:
- `name:` -- value assigned to `metadata.name`
- `description:` -- value assigned to `metadata.description`
- `hooks:` -- captures the hooks key and all indented continuation lines (2-space or tab indent); stores as `metadata.hooks`
- `disable-model-invocation:` -- sets `metadata.model_invocation_disabled = true` when value is `"true"`
- Unknown keys are silently ignored (line 40 falls through to next iteration)

Returns an error if `---` opener is found but no closing `---` delimiter exists (lines 28-33).

**`extract_script_references(content, variable_prefix)`** (lines 84-104):
Scans content for `<variable_prefix><path>` patterns. Extracts the path after the prefix until a terminator character (whitespace, `"`, `'`, backtick, `)`) or end of line. Only keeps paths starting with `scripts/` (line 96).

**`collect_files_recursive(dir, base, fs)`** (lines 107-126):
Recursively collects all files in a directory tree. Returns paths relative to `base`. Silently skips entries where `strip_prefix(base)` fails (line 120).

### `strip_yaml_quotes` in mod.rs (lines 85-93):
Removes matching surrounding quote delimiters (`"..."` or `'...'`) from YAML scalar values.

---

## Orchestration Flow

### File: `/workspaces/aipm/crates/libaipm/src/migrate/mod.rs`

### Entry Point: `migrate()` (lines 289-321)

1. Validates `.ai/` directory exists (lines 293-295)
2. Branches on `opts.source`:
   - `Some(source)` -> `migrate_single_source()` (legacy mode)
   - `None` -> `migrate_recursive()` (discovery mode)

### Single Source Mode: `migrate_single_source()` (lines 324-395)

1. Validates source directory exists (line 335-337)
2. Gets detectors via `detectors_for_source(source)` (line 339)
3. If `detectors_for_source` returns empty (unknown source type), falls back to all Claude + Copilot detectors combined (lines 340-344)
4. Runs each detector against `source_dir`, collecting all artifacts into `all_artifacts` (lines 347-350)
5. In dry-run mode: generates report and writes to `aipm-migrate-dryrun-report.md`
6. In normal mode: emits each artifact as a plugin, resolves name conflicts, registers in marketplace.json

### Recursive Mode: `migrate_recursive()` (lines 398-529)

1. Calls `discover_source_dirs(dir, &[".claude", ".github"], max_depth)` (line 409)
2. Returns early with empty outcome if no sources found (lines 410-412)
3. **Parallel detection** using `rayon::par_iter()` (lines 415-447): for each `DiscoveredSource`, gets detectors via `detectors_for_source(&src.source_type)` and runs all detectors
4. Package-scoped vs root-level handling (lines 425-445):
   - If `src.package_name` is `Some`, all artifacts are merged into one `PluginPlan` under the package name
   - If `src.package_name` is `None`, each artifact becomes its own `PluginPlan`
5. Empty plans are filtered out (line 456)
6. Sequential name resolution to avoid conflicts (lines 473-486)
7. **Parallel emission** using `rayon::par_iter()` (lines 489-513)
8. Registers all plugins in marketplace.json (line 524)

---

## Complete Map of What Each Detector Scans

### For `.claude/` source:

| Detector | Scans | Extension/Pattern | Emits |
|----------|-------|-------------------|-------|
| SkillDetector | `skills/<dir>/SKILL.md` | directory with SKILL.md | Skill |
| CommandDetector | `commands/*.md` | `.md` files | Command |
| AgentDetector | `agents/*.md` | `.md` files | Agent |
| McpDetector | `../.mcp.json` (project root) | `.mcp.json` with `mcpServers` | McpServer |
| HookDetector | `settings.json` | `hooks` key in JSON | Hook |
| OutputStyleDetector | `output-styles/*.md` | `.md` files | OutputStyle |

### For `.github/` source:

| Detector | Scans | Extension/Pattern | Emits |
|----------|-------|-------------------|-------|
| CopilotSkillDetector | `skills/<dir>/SKILL.md` | directory with SKILL.md | Skill |
| CopilotAgentDetector | `agents/*.md`, `agents/*.agent.md` | `.md` files, dedup | Agent |
| CopilotMcpDetector | `../.copilot/mcp-config.json` | `mcpServers` key | McpServer |
| CopilotHookDetector | `hooks.json` or `hooks/hooks.json` | non-empty JSON object | Hook |
| CopilotExtensionDetector | `extensions/<dir>/` | any subdirectory | Extension |
| CopilotLspDetector | `lsp.json` or `../.github/lsp.json` | non-empty JSON object | LspServer |

---

## Critical Finding: No Tracking of Unclassified Files

After thorough analysis of the entire discovery-detection-emission pipeline, there is **no mechanism** to track, report, or surface files that exist within discovered source directories but are not matched by any detector. Specifically:

### What falls through for `.claude/`:

1. **Files directly in `.claude/`** (not in a subdirectory): files like `.claude/CLAUDE.md`, `.claude/README.md`, or any top-level files. No detector scans for files at the root of the source directory.

2. **Non-`hooks` keys in `settings.json`**: The `HookDetector` only extracts the `hooks` key from `settings.json` (line 31 of `hook_detector.rs`). Other configuration keys (e.g., `permissions`, `allowedTools`, `model`) are not migrated and not flagged.

3. **Non-`.md` files in `commands/`**: The `CommandDetector` silently skips files without a `.md` extension (line 32-37 of `command_detector.rs`).

4. **Non-`.md` files in `agents/`**: The `AgentDetector` silently skips non-`.md` files (line 31-34 of `agent_detector.rs`).

5. **Non-`.md` files in `output-styles/`**: The `OutputStyleDetector` silently skips non-`.md` files (line 31-34 of `output_style_detector.rs`).

6. **Skill directories without `SKILL.md`**: The `SkillDetector` silently skips subdirectories under `skills/` that lack a `SKILL.md` file (line 36-38 of `skill_detector.rs`).

7. **Unknown subdirectories of `.claude/`**: Any directory that is not `skills/`, `commands/`, `agents/`, or `output-styles/` is never examined. For example, `.claude/custom-configs/` would be entirely ignored.

8. **Standalone files in `skills/`**: Non-directory entries directly in `skills/` are skipped (line 29-31 of `skill_detector.rs`).

### What falls through for `.github/`:

1. **Most of `.github/` content**: `.github/workflows/`, `.github/CODEOWNERS`, `.github/ISSUE_TEMPLATE/`, etc. are all silently ignored. The copilot detectors only look at `skills/`, `agents/`, `extensions/`, `hooks.json`, and `lsp.json`.

2. **Non-`.md` files in `agents/`**: Silently skipped by `CopilotAgentDetector`.

3. **Non-directory entries in `extensions/`**: Files directly in `extensions/` (not in subdirectories) are silently skipped (line 29 of `copilot_extension_detector.rs`).

### Where "unclassified" could be reported but is not:

The `dry_run.rs` module generates reports (line 44 shows the sections enumerated), but it only reports artifacts that were **successfully detected**. The discovery table in the recursive report (lines 226-264 of `dry_run.rs`) shows counts per artifact kind plus an "Other" column (for OutputStyle, LspServer, Extension), but this is "other classified types," not "unclassified files."

The `Action` enum in `mod.rs` (lines 134-187) includes `Action::Skipped` with a name and reason, but this variant is never constructed anywhere in the detection pipeline. It exists in the enum definition but is only used in the name-conflict resolution or emission stages, not during classification.

### How detection results aggregate:

In both `migrate_single_source` (line 347-350) and `migrate_recursive` (lines 419-422), the pattern is:

```rust
for det in &detectors {
    let artifacts = det.detect(&source_dir, fs)?;
    all_artifacts.extend(artifacts);
}
```

Each detector returns only what it finds. There is no step that compares "all files in source directory" against "all files claimed by detectors" to find orphans. The total set of files in the source directory is never enumerated in a single pass.

---

## Relevant Files

- `/workspaces/aipm/crates/libaipm/src/migrate/mod.rs` -- orchestrator, `ArtifactKind`, `Artifact`, `migrate()`
- `/workspaces/aipm/crates/libaipm/src/migrate/discovery.rs` -- `DiscoveredSource`, `discover_source_dirs()`
- `/workspaces/aipm/crates/libaipm/src/migrate/detector.rs` -- `Detector` trait, factory functions
- `/workspaces/aipm/crates/libaipm/src/migrate/skill_detector.rs` -- `SkillDetector`
- `/workspaces/aipm/crates/libaipm/src/migrate/command_detector.rs` -- `CommandDetector`
- `/workspaces/aipm/crates/libaipm/src/migrate/agent_detector.rs` -- `AgentDetector`
- `/workspaces/aipm/crates/libaipm/src/migrate/mcp_detector.rs` -- `McpDetector`
- `/workspaces/aipm/crates/libaipm/src/migrate/hook_detector.rs` -- `HookDetector`
- `/workspaces/aipm/crates/libaipm/src/migrate/output_style_detector.rs` -- `OutputStyleDetector`
- `/workspaces/aipm/crates/libaipm/src/migrate/copilot_skill_detector.rs` -- `CopilotSkillDetector`
- `/workspaces/aipm/crates/libaipm/src/migrate/copilot_agent_detector.rs` -- `CopilotAgentDetector`
- `/workspaces/aipm/crates/libaipm/src/migrate/copilot_mcp_detector.rs` -- `CopilotMcpDetector`
- `/workspaces/aipm/crates/libaipm/src/migrate/copilot_hook_detector.rs` -- `CopilotHookDetector`
- `/workspaces/aipm/crates/libaipm/src/migrate/copilot_extension_detector.rs` -- `CopilotExtensionDetector`
- `/workspaces/aipm/crates/libaipm/src/migrate/copilot_lsp_detector.rs` -- `CopilotLspDetector`
- `/workspaces/aipm/crates/libaipm/src/migrate/skill_common.rs` -- shared parsing utilities
- `/workspaces/aipm/crates/libaipm/src/migrate/dry_run.rs` -- dry-run report generation
