---
title: "Migrate File Movement and Path Rewriting System"
date: 2026-04-01
scope: "aipm migrate — emitter, registrar, cleanup"
status: documentation
---

# Migrate File Movement and Path Rewriting System

## Overview

The `aipm migrate` command converts AI tool configurations (skills, commands,
agents, MCP servers, hooks, output styles, LSP servers, extensions) from source
directories (`.claude/`, `.github/`) into plugin directories under `.ai/`. The
pipeline has three phases: (1) detection of artifacts, (2) emission of plugin
directories, and (3) optional post-migration cleanup. Emission includes creating
directory structures, copying and transforming file content, and registering
plugins in `marketplace.json`.

## Entry Points

### CLI Layer (`crates/aipm/src/main.rs:617-697`)

The `cmd_migrate` function at line 617 is the CLI entry point. It:

1. Resolves the project directory via `resolve_dir` (line 625).
2. Constructs an `Options` struct (line 626-627) and calls `libaipm::migrate::migrate()` (line 629).
3. Iterates over actions in the `Outcome` and prints status messages to stdout (lines 632-658).
4. Handles the post-migration cleanup phase (lines 662-694):
   - Skips cleanup if `dry_run` is true or no artifacts were migrated (line 662).
   - If `--destructive` was passed, cleanup runs unconditionally (line 666-667).
   - Otherwise, in interactive terminals, prompts via `wizard_tty::resolve_migrate_cleanup` (line 671).
   - Calls `cleanup::remove_migrated_sources` and prints removal actions (lines 678-693).

### Library Orchestration (`crates/libaipm/src/migrate/mod.rs:289-321`)

The `migrate()` function at line 289 validates that `.ai/` exists (line 293),
then dispatches to one of two modes based on whether `opts.source` is `Some`:

- `migrate_single_source` (line 324-395): Legacy mode when `--source` is
  explicitly provided. Runs detectors for the given source, then emits each
  artifact as its own plugin via `emitter::emit_plugin`.
- `migrate_recursive` (line 398-530): Default mode. Uses `discovery::discover_source_dirs`
  to walk the project tree for `.claude/` and `.github/` directories. Detection
  runs in parallel via `rayon`. Plans are grouped as either package-scoped (all
  artifacts merged into one plugin) or root-level (each artifact becomes its own
  plugin). Name resolution happens sequentially, then emission runs in parallel.

## Core Data Structures

### `Options` (`mod.rs:113-130`)

| Field       | Type              | Purpose                                                       |
|-------------|-------------------|---------------------------------------------------------------|
| `dir`       | `&Path`           | Project root directory                                        |
| `source`    | `Option<&str>`    | Explicit source folder name (e.g., ".claude"); `None` = recursive |
| `dry_run`   | `bool`            | Report only, no writes                                        |
| `destructive` | `bool`          | Auto-remove source files after migration                      |
| `max_depth` | `Option<usize>`   | Max directory traversal depth for recursive discovery         |
| `manifest`  | `bool`            | Generate `aipm.toml` plugin manifests                         |

### `Action` Enum (`mod.rs:133-187`)

| Variant                | Fields                                       | Meaning                                        |
|------------------------|----------------------------------------------|-------------------------------------------------|
| `PluginCreated`        | `name`, `source`, `plugin_type`, `source_is_dir` | A plugin directory was created                |
| `MarketplaceRegistered`| `name`                                       | Plugin was added to `marketplace.json`          |
| `Renamed`              | `original_name`, `new_name`, `reason`        | Name conflict caused a rename                   |
| `Skipped`              | `name`, `reason`                             | Artifact was skipped (e.g., unsafe name)        |
| `DryRunReport`         | `path`                                       | Dry-run report file was written                 |
| `SourceFileRemoved`    | `path`                                       | A migrated source file was deleted              |
| `SourceDirRemoved`     | `path`                                       | A migrated source directory was deleted         |
| `EmptyDirPruned`       | `path`                                       | An empty parent directory was removed           |

### `PluginEntry` (`mod.rs:267-273`)

Carries a plugin `name` and optional `description` for marketplace registration.

### `PluginPlan` (`mod.rs:277-286`)

Groups one or more `Artifact` objects under a plugin name. Fields:

- `name`: The target plugin name (package name or individual artifact name).
- `artifacts`: All artifacts to include in this plugin.
- `is_package_scoped`: `true` when multiple artifacts were merged from a
  monorepo sub-package; `false` when each artifact is its own plugin.
- `source_dir`: The `.claude/` directory this plan originated from.

Used only in the recursive migration path. In single-source mode, artifacts are
emitted individually without a `PluginPlan` wrapper.

### `Artifact` (`mod.rs:96-110`)

The unit of detected content. Fields:

- `kind`: One of `Skill`, `Command`, `Agent`, `McpServer`, `Hook`, `OutputStyle`, `LspServer`, `Extension`.
- `name`: Artifact name (e.g., "deploy").
- `source_path`: Absolute path to the source file or directory.
- `files`: Paths relative to `source_path` (populated for skills and extensions).
- `referenced_scripts`: Script paths found in the artifact content (populated for skills and hooks).
- `metadata`: Parsed frontmatter/config metadata.

## Emitter: Plugin Directory Creation (`crates/libaipm/src/migrate/emitter.rs`)

### Three Emission Entry Points

1. **`emit_plugin`** (line 30-125): Used by single-source mode. Resolves name
   conflicts via `resolve_plugin_name`, then emits the artifact.

2. **`emit_plugin_with_name`** (line 242-334): Used by recursive mode for
   single-artifact plans. Accepts a pre-resolved name (no rename logic).

3. **`emit_package_plugin`** (line 341-429): Used by recursive mode for
   package-scoped plans. Emits all artifacts under one plugin directory.

All three follow the same structural pattern:

1. Validate the plugin/artifact name via `is_safe_path_segment` (line 17-24).
2. Create `<ai_dir>/<plugin_name>/` and `<ai_dir>/<plugin_name>/.claude-plugin/`.
3. Dispatch to a kind-specific emitter function.
4. Copy referenced scripts (if any).
5. Extract hooks from frontmatter into `hooks/hooks.json` (if present and not a Hook artifact).
6. Generate `aipm.toml` (if `--manifest` is requested).
7. Generate `.claude-plugin/plugin.json`.

### Directory Structure Per Artifact Kind

#### Skill (`emit_skill_files`, line 130-154)

```
.ai/<plugin>/
  .claude-plugin/plugin.json
  skills/<artifact-name>/
    SKILL.md           (copied from source, with path rewriting)
    <other files>      (copied verbatim from source skill directory)
  scripts/             (if referenced_scripts is non-empty)
    <script files>
  hooks/hooks.json     (if frontmatter contains hooks)
  aipm.toml            (if --manifest)
```

All files listed in `artifact.files` are copied from `artifact.source_path` to
`skills/<name>/`. Files under `scripts/` that also appear in
`artifact.referenced_scripts` are excluded from the skill subdirectory copy to
avoid duplication — they are copied separately to the plugin root `scripts/`
directory.

For SKILL.md files specifically, path rewriting is applied (see Path
Transformations below).

#### Command (`emit_command_as_skill`, line 157-174)

Commands are converted to skills. The emitter reads the original command `.md`
file and wraps it in a SKILL.md with `disable-model-invocation: true` in the
frontmatter. If frontmatter already exists, `inject_disable_model_invocation`
(line 178-215) inserts or updates the key.

```
.ai/<plugin>/
  .claude-plugin/plugin.json
  skills/<artifact-name>/
    SKILL.md           (generated from command content)
```

#### Agent (`emit_agent_files`, line 510-521)

```
.ai/<plugin>/
  .claude-plugin/plugin.json
  agents/
    <artifact-name>.md (copied from source)
```

The destination filename is derived from `artifact.name`, not the original
filename.

#### MCP Server (`emit_mcp_config`, line 526-534)

```
.ai/<plugin>/
  .claude-plugin/plugin.json
  .mcp.json            (from raw_content or source_path)
```

Uses `artifact.metadata.raw_content` if available; otherwise reads from
`artifact.source_path`.

#### Hook (`emit_hooks_config`, line 540-553)

```
.ai/<plugin>/
  .claude-plugin/plugin.json
  hooks/
    hooks.json         (from raw_content, with command path rewriting)
```

Applies `rewrite_hook_command_paths` (see Path Transformations below).

#### Output Style (`emit_output_style`, line 556-561)

```
.ai/<plugin>/
  .claude-plugin/plugin.json
  <artifact-name>.md   (copied from source)
```

#### LSP Server (`emit_lsp_config`, line 566-574)

```
.ai/<plugin>/
  .claude-plugin/plugin.json
  lsp.json             (from raw_content or source_path)
```

#### Extension (`emit_extension_files`, line 579-629)

```
.ai/<plugin>/
  .claude-plugin/plugin.json
  extensions/<artifact-name>/
    config.json        (from raw_content if available)
    <other files>      (all files from artifact.files, excluding the source config file)
```

If `artifact.metadata.raw_content` is present, it is written as
`config.json`. The original config file (identified by matching filename against
a list of candidates at line 585-595) is then skipped in the file-copy loop to
avoid overwriting. All other files in `artifact.files` are copied preserving
their relative paths.

### Name Resolution (`resolve_plugin_name`, line 218-236)

When a plugin name conflicts with an existing name in `.ai/`, the function
appends `-renamed-<N>` where `N` is a monotonically incrementing counter. A
`Renamed` action is recorded.

### Package Emission (`emit_package_plugin`, line 341-429)

For package-scoped plugins, `emit_package_artifacts` (line 439-504) iterates
over all artifacts and dispatches to the same kind-specific emitters. Multiple
skills and commands coexist as separate subdirectories under `skills/`. Hooks
from skill/command frontmatter are merged into a single `hooks/hooks.json`. A
`PluginCreated` action is recorded for each distinct `artifact.source_path` (line
416-426) to enable cleanup of all migrated sources.

## Path Transformations

### Skill Directory Path Rewriting (`rewrite_skill_dir_paths`, line 771-773)

When copying SKILL.md files, all occurrences of `${CLAUDE_SKILL_DIR}/scripts/`
are replaced with `${CLAUDE_SKILL_DIR}/../../scripts/`. This accounts for the
fact that the skill file moves from its original location at the skill directory
root to a nested path under `skills/<name>/SKILL.md`, requiring a `../../`
traversal to reach the plugin root `scripts/` directory.

The rewriting is a simple string replacement applied to the entire file content.
It is applied only to files identified as SKILL.md by `file_is_skill_md` (line
766-768), which checks whether the filename equals `"SKILL.md"`.

### Hook Command Path Rewriting (`rewrite_hook_command_paths`, line 779-795)

When emitting hook artifacts, relative `command` paths in hook handler JSON are
rewritten to absolute paths. The function:

1. Parses the hooks content as JSON (line 780-783).
2. Derives the project root from `source_path` (which is `.claude/settings.json`)
   by taking the grandparent directory (line 786-788).
3. Recursively walks the JSON via `rewrite_commands_recursive` (line 798-822).
4. For each object with `"type": "command"`, rewrites the `"command"` field
   via `rewrite_single_command` (line 825-843).

`rewrite_single_command` splits the command string on the first whitespace. If
the script portion is a relative path (starts with `./` or contains `/` but is
not absolute), it is resolved against the project root to produce an absolute
path. Arguments after the first whitespace are preserved.

### Referenced Script Copying (`copy_referenced_scripts`, line 632-674)

Scripts referenced in artifact content are copied to `<plugin_dir>/scripts/`.
The function:

1. Normalizes each script path by stripping a leading `./` prefix (line 642-645).
2. For hook artifacts, resolves the source path against the project root
   (grandparent of `source_path`); for all other artifacts, resolves against
   `artifact.source_path` (lines 649-660).
3. Checks if the source file exists (line 662).
4. Strips a `scripts/` prefix from the destination path to avoid
   `scripts/scripts/` nesting (line 664).
5. Reads and writes the file content.

### Script Reference Detection

Scripts are detected at two points:

- **Skill detector** (`skill_detector.rs:43-44`): Calls
  `skill_common::extract_script_references` with prefix `${CLAUDE_SKILL_DIR}/`.
  This function (`skill_common.rs:84-104`) scans each line for the prefix and
  extracts the path until whitespace or a delimiter character. Only paths
  starting with `scripts/` are kept.

- **Hook detector** (`hook_detector.rs:42`): Calls
  `extract_hook_script_references` which parses the hooks JSON and checks each
  `"type": "command"` handler's `command` field for relative script paths.

## Registrar: Marketplace Registration (`crates/libaipm/src/migrate/registrar.rs`)

### `register_plugins` (line 10-51)

Appends migrated plugins to `.ai/.claude-plugin/marketplace.json`.

1. Returns early if `entries` is empty (line 11-13).
2. Reads and parses the existing `marketplace.json` (lines 16-18).
3. Obtains a mutable reference to the `"plugins"` array (lines 20-26).
4. For each `PluginEntry`, checks if a plugin with that name already exists
   (lines 29-31). Skips duplicates (line 32-34).
5. Appends a new object with `name`, `source` (formatted as `./<name>`), and
   `description` (lines 40-44). When no description is provided, uses the
   fallback `"Migrated from .claude/ configuration"` (line 38).
6. Re-serializes as pretty JSON with a trailing newline and writes back (lines
   47-49).

Existing entries in the `plugins` array and all other top-level fields in the
JSON are preserved unchanged.

## Cleanup: Post-Migration Source Removal (`crates/libaipm/src/migrate/cleanup.rs`)

### `should_skip_for_report` / `should_skip` (lines 14-25)

Returns `true` for source paths whose filename matches `SKIP_FILENAMES`:
`settings.json` and `.mcp.json`. These files are never deleted because they may
contain shared configuration beyond what was migrated.

### `remove_migrated_sources` (lines 35-74)

Removes successfully-migrated source files and prunes empty parent directories.

1. Iterates over `outcome.migrated_sources()` — the source paths from all
   `PluginCreated` actions (line 42).
2. Skips paths matching `should_skip` (line 43-45).
3. For directory sources (`is_dir == true`), calls `fs.remove_dir_all` and
   records `SourceDirRemoved` (lines 48-49).
4. For file sources, calls `fs.remove_file` and records `SourceFileRemoved`
   (lines 51-52).
5. Collects parent directories of all removed paths (lines 55-57).
6. Sorts parent directories deepest-first by component count (line 62).
7. For each parent directory, checks if it is now empty via `fs.read_dir`
   (line 65). If empty, removes it and records `EmptyDirPruned` (lines 67-68).
8. Errors from `read_dir` on parent directories are silently ignored (the
   directory is simply not pruned).

## Handling of "Extra" and "Other" Files

### What the emitter currently handles

The emitter copies **all files** listed in `artifact.files` for two artifact
kinds:

1. **Skills** (`emit_skill_files`, line 130-154): Every file in `artifact.files`
   is copied from `artifact.source_path` to the plugin's `skills/<name>/`
   subdirectory. The only exception is files under `scripts/` that also appear in
   `referenced_scripts` — these are excluded from the skill subdirectory and
   instead copied to the plugin root `scripts/` directory by
   `copy_referenced_scripts`. The `artifact.files` list is populated by
   `skill_common::collect_files_recursive` (`skill_common.rs:107-126`), which
   recursively walks the entire skill source directory. This means any file
   present in the source skill directory (e.g., helper scripts, config files,
   data files) is copied into the plugin skill subdirectory.

2. **Extensions** (`emit_extension_files`, line 579-629): All files in
   `artifact.files` are copied to `extensions/<name>/`, preserving relative
   paths. The only exclusion is the config file whose content was already written
   from `raw_content`.

### What the emitter does NOT handle

For all other artifact kinds — **Command, Agent, McpServer, Hook, OutputStyle,
LspServer** — the `artifact.files` field is either empty or ignored. These
emitters read content directly from `artifact.source_path` or
`artifact.metadata.raw_content` and write a single output file. No additional
files from the source directory are copied.

### Script path references in skills

When a SKILL.md references a script at a relative path like
`${CLAUDE_SKILL_DIR}/scripts/deploy.sh`, the emitter:

1. **Copies the script** from the source skill directory to the plugin root
   `scripts/` directory via `copy_referenced_scripts` (line 93-95).
2. **Rewrites the path** in the SKILL.md content from
   `${CLAUDE_SKILL_DIR}/scripts/` to `${CLAUDE_SKILL_DIR}/../../scripts/`
   via `rewrite_skill_dir_paths` (line 148-149, 771-773).

This path rewriting is a blanket string replacement — it rewrites all
occurrences of the prefix string, regardless of context. It does not parse the
markdown or validate that the referenced file actually exists in the scripts
directory.

### Script path references in hooks

When a hook's `command` field references a relative script path (e.g.,
`./scripts/validate.sh`), the emitter:

1. **Copies the script** to the plugin root `scripts/` directory via
   `copy_referenced_scripts` (line 305-307 in `emit_plugin_with_name`, or
   line 488-489 in `emit_package_artifacts`).
2. **Rewrites the command path** to an absolute path in the hooks JSON via
   `rewrite_hook_command_paths` (line 547 in `emit_hooks_config`).

The hook command rewriting converts relative paths to absolute paths rooted at
the project root — it does **not** rewrite them to point at the copied script in
the plugin directory.

### Files NOT handled

There is no mechanism to:

- Copy arbitrary files referenced by relative paths in skill content (other than
  scripts detected via the `${CLAUDE_SKILL_DIR}/scripts/` pattern).
- Update relative path references in non-SKILL.md files within a skill directory.
- Copy files referenced by agents, commands, or other artifact types.
- Rewrite paths in MCP, LSP, or extension configurations.

## Generated Files

### `plugin.json` (`generate_plugin_json_multi`, line 1013-1059)

A JSON file at `.claude-plugin/plugin.json` containing:

- `name`: The plugin name.
- `version`: Always `"0.1.0"`.
- `description`: From metadata or fallback `"Migrated from .claude/ configuration"`.
- Kind-specific component fields: `skills` (`"./skills/"`), `agents`
  (`"./agents/"`), `mcpServers` (`./.mcp.json`), `hooks`
  (`"./hooks/hooks.json"`), `outputStyles` (`"./"`), `lspServers`
  (`"./lsp.json"`), `extensions` (`"./extensions/"`).

### `aipm.toml` (`generate_plugin_manifest`, line 943-1001)

A TOML manifest containing `[package]` (name, version, type, description) and
`[components]` (skills, agents, mcp_servers, hooks, output_styles, scripts).
Only generated when `--manifest` is passed.

## Relevant Files

- `/workspaces/aipm/crates/libaipm/src/migrate/mod.rs` — Orchestration, data structures, pipeline entry
- `/workspaces/aipm/crates/libaipm/src/migrate/emitter.rs` — Plugin directory creation, file copying, path rewriting
- `/workspaces/aipm/crates/libaipm/src/migrate/registrar.rs` — Marketplace.json registration
- `/workspaces/aipm/crates/libaipm/src/migrate/cleanup.rs` — Post-migration source removal
- `/workspaces/aipm/crates/libaipm/src/migrate/skill_common.rs` — Frontmatter parsing, script extraction, file collection
- `/workspaces/aipm/crates/libaipm/src/migrate/discovery.rs` — Recursive source directory discovery
- `/workspaces/aipm/crates/libaipm/src/migrate/skill_detector.rs` — Skill artifact detection
- `/workspaces/aipm/crates/libaipm/src/migrate/hook_detector.rs` — Hook artifact detection with script extraction
- `/workspaces/aipm/crates/aipm/src/main.rs` — CLI entry point (`cmd_migrate` at line 617)
