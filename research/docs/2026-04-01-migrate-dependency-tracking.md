---
title: "Migrate Dependency Tracking and Script Reference Parsing"
date: 2026-04-01
author: analysis
tags: [migrate, dependency-tracking, scripts, issue-123]
---

# Migrate Dependency Tracking and Script Reference Parsing

## Overview

The `aipm migrate` pipeline detects AI tool configuration artifacts (skills,
commands, agents, hooks) and converts them into plugin directories under `.ai/`.
Each detector populates an `Artifact` struct that includes a
`referenced_scripts` field. During emission, referenced scripts are copied into
the output plugin directory. This document traces exactly how script references
are extracted, stored, and used during migration, and identifies the current
boundaries of dependency tracking.

---

## 1. The Artifact and ArtifactMetadata Structs

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/mod.rs`

### Artifact (lines 97-110)

```rust
pub struct Artifact {
    pub kind: ArtifactKind,
    pub name: String,
    pub source_path: PathBuf,
    pub files: Vec<PathBuf>,
    pub referenced_scripts: Vec<PathBuf>,
    pub metadata: ArtifactMetadata,
}
```

- `files` — all files relative to `source_path`, collected recursively for
  skills (via `collect_files_recursive`), or a single-element vec for commands
  and agents.
- `referenced_scripts` — script paths extracted from the artifact's content.
  These are relative paths (e.g., `scripts/deploy.sh` or `./scripts/validate.sh`).

### ArtifactMetadata (lines 67-79)

```rust
pub struct ArtifactMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
    pub hooks: Option<String>,
    pub model_invocation_disabled: bool,
    pub raw_content: Option<String>,
}
```

There is **no** `dependencies` field, no `references` field, and no structured
representation of script dependencies on `ArtifactMetadata`. The only place
script dependencies are tracked is the `referenced_scripts: Vec<PathBuf>` field
on `Artifact` itself.

---

## 2. How `extract_script_references` Works

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/skill_common.rs`, lines 84-104

### Algorithm

The function `extract_script_references(content: &str, variable_prefix: &str)`
scans Markdown content line by line for occurrences of a variable prefix
(e.g., `${CLAUDE_SKILL_DIR}/` or `${SKILL_DIR}/`).

1. For each line, it searches for the `variable_prefix` string.
2. After the prefix, it extracts characters until it hits a terminator:
   whitespace, `"`, `'`, `` ` ``, `)`, or end of line (line 93).
3. If the extracted path starts with `scripts/`, it is added to the result
   vector as a `PathBuf`.
4. Paths that do **not** start with `scripts/` are silently discarded (line 96).
5. The search continues within the same line after each match, so multiple
   references per line are captured.

### Return Value

Returns `Vec<PathBuf>` containing relative paths like `scripts/deploy.sh`. The
`variable_prefix` is stripped; only the path after it is retained.

### Limitations

- Only matches literal variable-prefix patterns. Does not detect hardcoded
  relative paths, shell variable expansions, or other indirect references.
- Only retains paths starting with `scripts/`. A reference like
  `${CLAUDE_SKILL_DIR}/lib/helper.py` would be ignored.
- Does not deduplicate; the same script referenced twice produces two entries.

---

## 3. Detector-by-Detector Script Reference Behavior

### 3.1 SkillDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/skill_detector.rs`, lines 19-59

- Scans `.claude/skills/<name>/SKILL.md`.
- Calls `skill_common::extract_script_references(&content, "${CLAUDE_SKILL_DIR}/")` (line 44).
- Populates `Artifact.referenced_scripts` with the result (line 53).
- Also collects all files in the skill directory via `collect_files_recursive`
  (line 42), which means script files that physically exist under
  `skills/<name>/scripts/` are included in `Artifact.files`.

### 3.2 CopilotSkillDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/copilot_skill_detector.rs`, lines 47-50

- Calls `extract_script_references` with **two** prefixes:
  `${SKILL_DIR}/` first, then `${CLAUDE_SKILL_DIR}/`, merging both result sets.
- Populates `Artifact.referenced_scripts` with the combined list.

### 3.3 CommandDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/command_detector.rs`, lines 50-58

- Scans `.claude/commands/<name>.md`.
- Calls `skill_detector::extract_script_references(&content)` (line 50), which
  is a convenience wrapper that uses the `${CLAUDE_SKILL_DIR}/` prefix.
- Populates `Artifact.referenced_scripts` with the result.

### 3.4 AgentDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/agent_detector.rs`, lines 46-54

- Scans `.claude/agents/<name>.md`.
- Sets `referenced_scripts: Vec::new()` unconditionally (line 51).
- **Does not extract any script references from agent content.**

### 3.5 CopilotAgentDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/copilot_agent_detector.rs`, line 78

- Sets `referenced_scripts: Vec::new()` unconditionally.
- **Does not extract any script references from agent content.**

### 3.6 HookDetector

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/hook_detector.rs`, lines 18-57

- Reads `.claude/settings.json` and extracts the `hooks` JSON object.
- Calls `extract_hook_script_references(hooks_value, source_dir)` (line 42).
- Populates `Artifact.referenced_scripts` with the result.

**Hook script extraction** (lines 63-105) uses a different algorithm from the
skill/command extractor:

1. Recursively walks the hooks JSON value (objects and arrays).
2. For each object with `"type": "command"`, reads the `"command"` string.
3. Splits on whitespace and takes the first token as the script path (line 86).
4. Includes the path if it starts with `./` or passes `is_relative_script()`.

`is_relative_script` (lines 108-128) returns true when:
- The path contains `/` or `\` (a directory separator).
- The path has a known script extension: `.sh`, `.py`, `.js` (case-insensitive).

It returns false for:
- Empty strings.
- Absolute paths (Unix or Windows drive-letter style).
- Bare command names without separators or script extensions (e.g., `echo`, `npx`).

The resulting paths may include `./` prefixes (e.g., `./scripts/validate.sh`)
and are stored as-is in `referenced_scripts`.

---

## 4. Emission: How Referenced Scripts Are Copied

**File:** `/workspaces/aipm/crates/libaipm/src/migrate/emitter.rs`

### 4.1 Triggering Script Copy

In `emit_plugin` (line 93-95), `emit_plugin_with_name` (lines 305-307), and
`emit_package_artifacts` (lines 488-490), after emitting the main artifact
files, the emitter checks:

```rust
if !artifact.referenced_scripts.is_empty() {
    copy_referenced_scripts(artifact, &plugin_dir, fs)?;
}
```

### 4.2 `copy_referenced_scripts` (lines 632-674)

For each path in `artifact.referenced_scripts`:

1. **Normalize**: strips a leading `./` prefix if present (line 643-645).
2. **Resolve source path**: For hook artifacts, resolves against the project
   root (grandparent of `settings.json`). For all other artifact types, resolves
   against `artifact.source_path` (line 649-660).
3. **Check existence**: only copies if `fs.exists(&source)` returns true (line 662).
   If the file does not exist, it is silently skipped.
4. **Determine destination**: strips a leading `scripts/` prefix from the
   normalized path to avoid `scripts/scripts/` nesting, then writes into
   `<plugin_dir>/scripts/<relative>` (lines 663-665).
5. **Copy**: reads the file as a string and writes it to the destination
   (lines 669-670).

### 4.3 Skill File Deduplication

In `emit_skill_files` (lines 130-154), files from `Artifact.files` that are
under `scripts/` **and** also appear in `referenced_scripts` are skipped
(lines 136-139). This prevents duplicating script files that will be separately
copied by `copy_referenced_scripts` into the plugin root's `scripts/` directory.

### 4.4 Path Rewriting in Emitted SKILL.md

`rewrite_skill_dir_paths` (line 771-773) performs a string replacement:

```rust
content.replace("${CLAUDE_SKILL_DIR}/scripts/", "${CLAUDE_SKILL_DIR}/../../scripts/")
```

This rewrites skill content so that `${CLAUDE_SKILL_DIR}/scripts/deploy.sh`
becomes `${CLAUDE_SKILL_DIR}/../../scripts/deploy.sh`, pointing up from
`skills/<name>/` to the plugin root's `scripts/` directory where the scripts
are actually placed by `copy_referenced_scripts`.

---

## 5. What Is NOT Tracked as Dependencies

### 5.1 Agent Script References

Neither `AgentDetector` nor `CopilotAgentDetector` extracts script references.
Agent `.md` files that reference scripts via `${CLAUDE_SKILL_DIR}/scripts/...`
or any other pattern will have those references ignored. The
`referenced_scripts` field is always an empty `Vec` for agents.

### 5.2 Non-Script File References

`extract_script_references` only captures paths beginning with `scripts/`.
References to other files (e.g., `${CLAUDE_SKILL_DIR}/data/config.yaml` or
`${CLAUDE_SKILL_DIR}/templates/deploy.hbs`) are not tracked.

### 5.3 Cross-Artifact Dependencies

There is no mechanism for one artifact to declare a dependency on another
artifact. For example, if a skill references a hook, or a command references
a skill, there is no tracking of that relationship.

### 5.4 Hook Script File Copying Resolution

Hook-referenced scripts resolve against the **project root** (grandparent of
`.claude/settings.json`). If a hook command references `./scripts/validate.sh`,
the emitter looks for `<project_root>/scripts/validate.sh`. This means hook
scripts are expected to live at the project root level, not inside `.claude/`.

### 5.5 No Dependency Field in Metadata or Manifest

`ArtifactMetadata` has no `dependencies` or `references` field. The generated
`plugin.json` and `aipm.toml` manifest files do not contain dependency
information. The `referenced_scripts` data on `Artifact` is used only during
emission (file copying) and is not persisted into any output metadata.

---

## 6. Summary Table: Script Reference Handling by Detector

| Detector               | Extracts Script Refs?   | Variable Prefix(es)                                | Extraction Method                |
|------------------------|-------------------------|-----------------------------------------------------|----------------------------------|
| `SkillDetector`        | Yes                     | `${CLAUDE_SKILL_DIR}/`                              | `skill_common::extract_script_references` |
| `CopilotSkillDetector` | Yes                     | `${SKILL_DIR}/` and `${CLAUDE_SKILL_DIR}/`          | `skill_common::extract_script_references` (2 calls) |
| `CommandDetector`      | Yes                     | `${CLAUDE_SKILL_DIR}/`                              | `skill_detector::extract_script_references` wrapper |
| `AgentDetector`        | **No** (empty Vec)      | N/A                                                 | N/A |
| `CopilotAgentDetector` | **No** (empty Vec)      | N/A                                                 | N/A |
| `HookDetector`         | Yes                     | N/A (parses JSON `command` fields)                  | `extract_hook_script_references` (recursive JSON walk) |

---

## 7. Relevance to Issue #123

Issue #123 requests that skills and agents track referenced scripts as explicit
dependencies and migrate them together. The current state is:

1. **Skills and commands already extract and copy scripts** via
   `extract_script_references` and `copy_referenced_scripts`. This is functional
   but the dependency relationship is transient (only lives on the `Artifact`
   struct during migration, not persisted in output metadata).

2. **Agents do not extract script references at all.** Both `AgentDetector`
   (line 51) and `CopilotAgentDetector` (line 78) hardcode
   `referenced_scripts: Vec::new()`.

3. **No dependency metadata is emitted.** Neither `plugin.json` nor `aipm.toml`
   includes a list of script dependencies. The `ArtifactMetadata` struct has no
   field for this.

4. **Hook scripts are extracted and copied**, but through a separate code path
   (`extract_hook_script_references`) that walks JSON rather than scanning
   Markdown for variable prefixes.

### What would need to change for Issue #123

The following gaps exist relative to the issue's goals (documented here as
factual gaps, not recommendations):

- `AgentDetector` and `CopilotAgentDetector` would need to call
  `extract_script_references` (or equivalent) and populate `referenced_scripts`.
- `ArtifactMetadata` or the emitted `plugin.json`/`aipm.toml` would need a
  field to persist dependency information.
- The `emit_agent_files` function in the emitter currently copies the agent
  `.md` file only and does not invoke `copy_referenced_scripts` (though the
  outer `emit_plugin`/`emit_plugin_with_name` functions do call it if
  `referenced_scripts` is non-empty, so adding refs to agent artifacts would
  automatically trigger copying).

---

## Key File References

| File | Purpose |
|------|---------|
| `/workspaces/aipm/crates/libaipm/src/migrate/mod.rs` | `Artifact`, `ArtifactMetadata`, `ArtifactKind` definitions, migration orchestration |
| `/workspaces/aipm/crates/libaipm/src/migrate/skill_common.rs` | `extract_script_references`, `parse_skill_frontmatter`, `collect_files_recursive` |
| `/workspaces/aipm/crates/libaipm/src/migrate/skill_detector.rs` | `SkillDetector` — uses `${CLAUDE_SKILL_DIR}/` prefix |
| `/workspaces/aipm/crates/libaipm/src/migrate/copilot_skill_detector.rs` | `CopilotSkillDetector` — uses both `${SKILL_DIR}/` and `${CLAUDE_SKILL_DIR}/` |
| `/workspaces/aipm/crates/libaipm/src/migrate/command_detector.rs` | `CommandDetector` — reuses skill script extraction |
| `/workspaces/aipm/crates/libaipm/src/migrate/agent_detector.rs` | `AgentDetector` — no script extraction |
| `/workspaces/aipm/crates/libaipm/src/migrate/copilot_agent_detector.rs` | `CopilotAgentDetector` — no script extraction |
| `/workspaces/aipm/crates/libaipm/src/migrate/hook_detector.rs` | `HookDetector` — JSON-based command path extraction |
| `/workspaces/aipm/crates/libaipm/src/migrate/emitter.rs` | `emit_plugin`, `copy_referenced_scripts`, `rewrite_skill_dir_paths`, `emit_skill_files` |
