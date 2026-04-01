---
date: 2026-04-01
researcher: Claude
repository: aipm
topic: "Documentation of the aipm migrate dry-run report generation system"
tags: [research, codebase, migrate, dry-run, report, documentation]
status: complete
last_updated: 2026-04-01
---

# Dry-Run Report Generation System in `aipm migrate`

## Overview

The dry-run report system generates a Markdown document describing what `aipm migrate` would do without performing any actual file operations. It is implemented in `/workspaces/aipm/crates/libaipm/src/migrate/dry_run.rs` and invoked from the main migration pipeline in `/workspaces/aipm/crates/libaipm/src/migrate/mod.rs`. There are two report variants: a **single-source report** (`generate_report`) for when `--source` is explicitly provided, and a **recursive report** (`generate_recursive_report`) for when recursive directory discovery is used.

## Entry Points and Invocation

### Single-Source Mode (`mod.rs:354-364`)

When `opts.dry_run` is true and `opts.source` is `Some(...)`, the function `migrate_single_source` calls `dry_run::generate_report()` at line 355. It passes:
- `&all_artifacts` — all artifacts collected from running detectors against the source directory
- `&existing_plugins` — plugin names already present in `.ai/`
- `source` — the source folder name string (e.g., `".claude"`)
- `manifest` — whether `--manifest` was passed
- `destructive` — whether `--destructive` was passed

The generated report string is written to `{dir}/aipm-migrate-dryrun-report.md` via `fs.write_file()` at line 363. The function then returns an `Outcome` containing a single `Action::DryRunReport { path }` action.

### Recursive Mode (`mod.rs:460-469`)

When `opts.dry_run` is true and `opts.source` is `None`, the function `migrate_recursive` calls `dry_run::generate_recursive_report()` at line 461. It passes:
- `&discovered` — the list of `DiscoveredSource` structs from directory walking
- `&plugin_plans` — the list of `PluginPlan` structs (each grouping artifacts into a planned plugin)
- `&existing_plugins` — existing plugin names in `.ai/`
- `destructive` — whether `--destructive` was passed

The report is written to the same path (`aipm-migrate-dryrun-report.md`) and the same single-action `Outcome` is returned.

## Report Format

Both reports produce **Markdown** output. All write operations use `writeln!()` from `std::fmt::Write` into a `String` buffer.

---

## Single-Source Report (`generate_report`, `dry_run.rs:12-86`)

### Structure

1. **Header** (lines 22-24): An H1 title `# aipm migrate — Dry Run Report`, followed by the source name and total artifact count.

2. **Artifact Sections Grouped by Kind** (lines 27-70): Artifacts are partitioned into eight vectors by `ArtifactKind`:
   - Skills (`ArtifactKind::Skill`)
   - Legacy Commands (`ArtifactKind::Command`)
   - Agents (`ArtifactKind::Agent`)
   - MCP Servers (`ArtifactKind::McpServer`)
   - Hooks (`ArtifactKind::Hook`)
   - Output Styles (`ArtifactKind::OutputStyle`)
   - LSP Servers (`ArtifactKind::LspServer`)
   - Extensions (`ArtifactKind::Extension`)

   Each non-empty group gets an H2 heading (e.g., `## Skills`). Empty groups are omitted entirely. The section order is fixed as listed above (defined in the `sections` slice at lines 44-53).

3. **Per-Artifact Detail** (written by `write_artifact_section`, lines 283-355): Each artifact within a group gets an H3 heading with its name, followed by bullet points:
   - **Source:** the `source_path` display
   - **Target:** `.ai/{target_name}/` (where `target_name` may be renamed if a conflict exists)
   - **Files to copy:** listed if `artifact.files` is non-empty
   - **Manifest changes:** either "New aipm.toml with type = ..." (if `manifest` is true) or "No aipm.toml (pass --manifest to generate)"
   - **marketplace.json:** `append entry "{target_name}"`
   - **Path rewrites:** shown only if `artifact.referenced_scripts` is non-empty; displays the `${CLAUDE_SKILL_DIR}` path rewrite pattern
   - **Hooks extracted:** "yes" or "no" based on `artifact.metadata.hooks`
   - **Conflict:** "renamed to {new_name}" or "none"

4. **Summary Table** (lines 73-79): A Markdown table with columns `Action` and `Count`:
   - Plugins to create (total artifact count)
   - Marketplace entries to add (total artifact count)
   - Name conflicts (auto-renamed) (counter incremented during artifact rendering)
   - Hooks to extract (counter incremented during artifact rendering)

5. **Cleanup Plan** (lines 81-83, conditional): Only appended when `destructive` is true. Rendered by `write_cleanup_plan()` (lines 180-217).

### Name Conflict Resolution (lines 296-304)

A mutable `used_names` set is seeded from `existing_plugins`. For each artifact, if its name already exists in `used_names`, a suffix `-renamed-{counter}` is appended. The counter increments globally across all artifacts in the report. The renamed name is inserted into `used_names` so subsequent artifacts also avoid it.

---

## Recursive Report (`generate_recursive_report`, `dry_run.rs:89-177`)

### Structure

1. **Header** (lines 97-99): Same H1 title, plus `**Mode:** Recursive discovery` and a count of discovered source directories.

2. **Discovery Table** (rendered by `write_discovery_table`, lines 220-265): A Markdown table with columns:
   - Location (relative path + source type, e.g., `./packages/auth/.claude`)
   - Package Name (or `(root)` if none)
   - Skills count
   - Commands count
   - Agents count
   - MCP count
   - Hooks count
   - Other count (OutputStyle, LspServer, Extension all count as "Other")

   Counts are computed by iterating `plugin_plans` filtered by matching `source_dir`, then folding over artifacts by kind (lines 242-256).

3. **Planned Plugins** (lines 104-158): An H2 section. For each `PluginPlan`:
   - H3 with the final plugin name and source label (`from {name}` if package-scoped, `from root source` otherwise)
   - Type: determined by counting distinct `ArtifactKind` values — if more than one kind, "composite"; otherwise the single kind's `to_type_string()` value
   - Components: if a single artifact, one line with the component path; if multiple, a bulleted list with each component path. Commands get a `(converted from command)` suffix.

   Component paths are generated by `component_path()` (lines 268-280):
   - Skill/Command → `skills/{name}/SKILL.md`
   - Agent → `agents/{name}.md`
   - McpServer → `.mcp.json`
   - Hook → `hooks/hooks.json`
   - OutputStyle → `{name}.md`
   - LspServer → `lsp.json`
   - Extension → `extensions/{name}/`

4. **Name Conflicts** (lines 161-168): An H2 section listing all conflicts as `{original}` → `{renamed}`, or `(none)` if no conflicts occurred.

5. **Cleanup Plan** (lines 170-174, conditional): Same as single-source mode, but flattens all artifacts from all `plugin_plans` into a single list for the cleanup rendering.

### Differences from Single-Source Report

| Aspect | Single-Source | Recursive |
|--------|--------------|-----------|
| Grouping | Artifacts grouped by ArtifactKind (H2 per kind) | Artifacts grouped by PluginPlan (H3 per plugin) |
| Discovery table | Not present | Present with per-source-directory counts |
| Name conflict display | Inline per-artifact ("Conflict: renamed to ...") + summary count | Separate "Name Conflicts" section listing all renames |
| Per-artifact detail | Full detail (source, target, files, manifest, hooks, etc.) | Summarized (type, components list only) |
| Summary table | Present (plugins, marketplace entries, conflicts, hooks) | Not present |
| Mode indicator | Not shown | `**Mode:** Recursive discovery` |

---

## Cleanup Plan (`write_cleanup_plan`, `dry_run.rs:180-217`)

This section is appended only when `destructive` is true. It lists source files/directories that would be removed post-migration.

### Skip Logic (`cleanup.rs:18-25`)

The function `should_skip_for_report(path)` delegates to `should_skip(path)` which checks the file name component against `SKIP_FILENAMES`: `["settings.json", ".mcp.json"]` (line 14). If the file name matches, the artifact is excluded from the removal list.

### Cleanup Section Content

- H2: `## Cleanup Plan (--destructive)`
- For each artifact whose source path is NOT skipped: a bullet with the path and a label — `(directory)` for `ArtifactKind::Skill`, `(file)` for everything else (lines 195-196).
- If no non-skipped artifacts exist: `(no files to remove)`.
- If any artifacts were skipped: a `**Skipped (shared config):**` sub-section listing each skipped path with a reason:
  - Hook → "contains non-hook configuration"
  - McpServer → "may be used by other tools"
  - All others → "shared configuration"

---

## The `Outcome` Struct (`mod.rs:190-213`)

The `Outcome` struct contains a single field: `actions: Vec<Action>`.

### `Action` Enum Variants (`mod.rs:133-187`)

| Variant | Fields | Description |
|---------|--------|-------------|
| `PluginCreated` | `name`, `source`, `plugin_type`, `source_is_dir` | A plugin directory was created |
| `MarketplaceRegistered` | `name` | A plugin was registered in marketplace.json |
| `Renamed` | `original_name`, `new_name`, `reason` | A plugin was renamed due to a name conflict |
| `Skipped` | `name`, `reason` | An artifact was skipped |
| `DryRunReport` | `path` | A dry-run report file was generated |
| `SourceFileRemoved` | `path` | A migrated source file was removed |
| `SourceDirRemoved` | `path` | A migrated source directory was removed |
| `EmptyDirPruned` | `path` | An empty parent directory was pruned |

### Helper Methods

- `has_migrated_artifacts()` (line 197): Returns `true` if any `PluginCreated` action exists.
- `migrated_sources()` (line 202): Collects `(source_path, is_dir)` tuples from all `PluginCreated` actions.

### How Results Are Aggregated

In **dry-run mode**, the `Outcome` contains exactly one action: `Action::DryRunReport { path }`. No `PluginCreated`, `Renamed`, or other actions are generated because no mutation occurs.

In **live mode** (single-source, `mod.rs:367-394`):
1. Each artifact is passed to `emitter::emit_plugin()`, which returns a list of actions (typically `PluginCreated`, possibly `Renamed`).
2. After all emissions, `registrar::register_plugins()` is called, and `MarketplaceRegistered` actions are appended.

In **live mode** (recursive, `mod.rs:472-529`):
1. Name resolution happens sequentially, producing `Renamed` actions.
2. Emission happens in parallel via `rayon`, producing `PluginCreated` actions.
3. `MarketplaceRegistered` actions are appended after registration.

---

## Critical Finding: No Unclassified File Reporting

**The dry-run report does not currently show any information about files that were not classified by any detector.**

The detection pipeline works as follows (`mod.rs:339-350` for single-source, `mod.rs:418-421` for recursive):
1. Detectors are selected based on source type (`detector::detectors_for_source`).
2. Each detector scans a specific subdirectory pattern (e.g., `skills/`, `commands/`, `agents/`).
3. Detectors return only the artifacts they recognize.
4. Files that exist in the source directory but are not matched by any detector are silently ignored.

There is no mechanism to:
- Track which files in a source directory were visited vs. not visited
- Collect a list of "unclassified" or "unmatched" files
- Emit warnings about files that no detector recognized
- Include an "unclassified files" section in either report variant

The `Detector` trait (`detector.rs:13-19`) returns `Result<Vec<Artifact>, Error>` — there is no secondary return channel for unmatched files. The `Artifact` struct (`mod.rs:96-110`) represents only successfully classified items. The `Action` enum has a `Skipped` variant, but it is used for artifacts that were detected but intentionally skipped, not for files that were never detected in the first place.

The report's "Artifacts found" count (`dry_run.rs:24`) reflects only successfully detected artifacts — it does not indicate how many total files existed in the source directory.

---

## Relevant Files

| File | Purpose |
|------|---------|
| `/workspaces/aipm/crates/libaipm/src/migrate/dry_run.rs` | Report generation functions and helpers |
| `/workspaces/aipm/crates/libaipm/src/migrate/mod.rs` | Migration pipeline, `Outcome`/`Action` types, invocation of dry-run |
| `/workspaces/aipm/crates/libaipm/src/migrate/cleanup.rs` | `should_skip_for_report()` and `SKIP_FILENAMES` constant |
| `/workspaces/aipm/crates/libaipm/src/migrate/detector.rs` | `Detector` trait and detector selection |
