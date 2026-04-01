---
date: 2026-04-01 16:35:43 UTC
researcher: Claude Code (Opus 4.6)
git_commit: d381cd9fadbff3f637f3cbf4227e444e8794ffe0
branch: main
repository: aipm
topic: "[Issue #123] aipm migrate — handle unclassified 'other files', dependency tracking, and path rewriting"
tags: [research, codebase, migrate, file-discovery, dependency-tracking, dry-run, path-rewriting]
status: complete
last_updated: 2026-04-01
last_updated_by: Claude Code (Opus 4.6)
---

# Research: Issue #123 — Migrate Other Files Handling

## Research Question

**GitHub Issue:** [TheLarkInn/aipm#123](https://github.com/TheLarkInn/aipm/issues/123)

Document the current implementation of `aipm migrate`, focusing on: (1) how files are discovered and classified during migration, (2) the `--dry-run` report generation pipeline, (3) how plugin features (skills/agents/hooks) are parsed and what dependency tracking exists, (4) how files are moved and paths are rewritten, and (5) identify gaps where "other files" (unclassified files like scripts/utilities) are currently handled or dropped.

## Summary

The `aipm migrate` command implements a three-stage pipeline: **discovery** (find source directories) -> **detection** (classify files via detectors) -> **emission** (generate plugin output). The system currently has **no mechanism for tracking or reporting files that don't match any detector**. Files that exist in source directories but aren't recognized by any of the 12 detectors are silently dropped. Partial dependency tracking exists for skills and hooks (via `extract_script_references`), but agents have no script extraction, and dependency information is not persisted in output manifests.

## Detailed Findings

### 1. File Discovery

**Entry point:** [`discover_source_dirs()`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/discovery.rs#L54)

The `DiscoveredSource` struct ([`discovery.rs:10-22`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/discovery.rs#L10-L22)) holds:
- `path`: The discovered `.claude/` or `.github/` directory path
- `source`: The `Source` enum variant (Claude or Copilot)

Discovery functions:
- `discover_claude_dirs()` — Recursively walks directories up to `max_depth`, looking for `.claude/` directories
- `discover_source_dirs()` — Orchestrates discovery for all supported sources

Discovery enumerates **directories**, not individual files. It finds `.claude/` and `.github/` directories, then passes them to detectors for file-level classification.

### 2. File Classification (Detector System)

**Trait:** [`Detector`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/detector.rs#L13-L19)

```
pub trait Detector {
    fn detect(&self, source_dir: &Path) -> Result<Vec<Artifact>, Error>;
}
```

**Factory functions** ([`detector.rs:23-53`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/detector.rs#L23-L53)):
- `claude_detectors()` — Returns: SkillDetector, CommandDetector, AgentDetector, McpDetector, HookDetector, OutputStyleDetector
- `copilot_detectors()` — Returns: CopilotSkillDetector, CopilotAgentDetector, CopilotMcpDetector, CopilotHookDetector, CopilotLspDetector, CopilotExtensionDetector
- `detectors_for_source()` — Dispatches to the correct factory based on `Source`

**ArtifactKind enum** ([`mod.rs:29-47`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/mod.rs#L29-L47)) — 8 variants:
1. `Skill`
2. `LegacyCommand`
3. `Agent`
4. `McpServer`
5. `Hook`
6. `OutputStyle`
7. `LspServer`
8. `Extension`

**Detector matching patterns:**

| Detector | Scans | Pattern | Extension |
|---|---|---|---|
| SkillDetector | `.claude/skills/*/` | Directories with `SKILL.md` | `.md` |
| CommandDetector | `.claude/commands/` | Files in commands dir | `.md` |
| AgentDetector | `.claude/agents/` | Files in agents dir | `.md` |
| McpDetector | `.claude/settings.json` | `mcpServers` key in JSON | `.json` |
| HookDetector | `.claude/settings.json` | `hooks` key in JSON | `.json` |
| OutputStyleDetector | `.claude/output-styles/` | Files in output-styles dir | `.md` |

**What falls through the cracks (silently dropped):**
- Root-level files in `.claude/` (e.g., `CLAUDE.md`, `README.md`, custom scripts)
- Non-`.md` files in `commands/`, `agents/`, or `output-styles/`
- Skill directories without a `SKILL.md` file
- Unknown subdirectories of `.claude/` (anything not `skills/`, `commands/`, `agents/`, `output-styles/`)
- Non-`hooks`/`mcpServers` keys in `settings.json`
- Standalone script files (`.sh`, `.py`, `.js`) not inside a skill directory

**Critical gap:** The pipeline is purely additive. Each detector independently scans for what it recognizes. No step enumerates all files in a source directory to compare against claimed files. The `Action::Skipped` variant exists but is never constructed during detection.

### 3. Dry-Run Report

**Entry point:** [`generate_report()`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/dry_run.rs#L12-L86)

The report is **Markdown** output written to `{project_root}/aipm-migrate-dryrun-report.md`.

**Single-source report** groups artifacts into 8 sections (Skills, Legacy Commands, Agents, MCP Servers, Hooks, Output Styles, LSP Servers, Extensions). Each artifact gets bullet points covering:
- Source path
- Target directory
- Files to copy
- Manifest changes
- Marketplace entry
- Path rewrites
- Hooks extraction status
- Conflict status

A summary table at the end counts plugins, marketplace entries, conflicts, and hooks.

**Recursive report** ([`generate_recursive_report()`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/dry_run.rs#L89-L177)) opens with a discovery table showing per-source-directory artifact counts, then lists each `PluginPlan` with type info and component paths, ending with a Name Conflicts section.

**Cleanup plan** ([`write_cleanup_plan()`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/dry_run.rs#L180-L217)) appended only when `destructive` is true. Uses `SKIP_FILENAMES` constant ([`cleanup.rs:14`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/cleanup.rs#L14)) to exclude `settings.json` and `.mcp.json`.

**Outcome struct** ([`mod.rs:190-213`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/mod.rs#L190-L213)):
- In dry-run mode: holds one `Action::DryRunReport { path }`
- In live mode: accumulates `PluginCreated`, `MarketplaceRegistered`, `Renamed`, `Skipped`

**Critical gap:** The dry-run report has **zero support for reporting unclassified files**. The "Artifacts found" count reflects only detected artifacts, not total files present. There is no warnings section and no "unclassified files" concept.

### 4. Dependency Tracking

**Script extraction:** [`extract_script_references()`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/skill_common.rs#L84-L104) scans content line-by-line for a variable prefix (e.g., `${CLAUDE_SKILL_DIR}/`), extracts paths until a terminator character, and only keeps paths starting with `scripts/`. Returns `Vec<PathBuf>`.

**Artifact struct** ([`mod.rs:96-110`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/mod.rs#L96-L110)):
- `referenced_scripts: Vec<PathBuf>` — populated by some detectors, used during emission

**ArtifactMetadata** ([`mod.rs:67-79`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/mod.rs#L67-L79)):
- Fields: `name`, `description`, `hooks`, `model_invocation_disabled`, `raw_content`
- **No** `dependencies` or `references` field

**Per-detector dependency tracking:**

| Detector | Tracks scripts? | Mechanism |
|---|---|---|
| SkillDetector | Yes | `extract_script_references()` on SKILL.md content |
| CommandDetector | Yes | `extract_script_references()` on command content |
| HookDetector | Yes | Walks hooks JSON, finds `"type": "command"` handlers, extracts first token of `command` string if it looks like a script path |
| AgentDetector | **No** | Hardcodes `referenced_scripts: Vec::new()` |
| McpDetector | **No** | N/A |
| OutputStyleDetector | **No** | N/A |

**Script copying during emission:** [`copy_referenced_scripts()`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/emitter.rs#L632-L674) copies referenced scripts into `scripts/` within the plugin output. Triggered for any artifact with non-empty `referenced_scripts`.

**Critical gaps:**
- Agents have **no** script extraction
- Dependency data is **not** persisted in `plugin.json` or `aipm.toml`
- `extract_script_references()` only matches the `${CLAUDE_SKILL_DIR}/scripts/` pattern — other relative path formats are missed

### 5. File Movement and Path Rewriting

**Emitter:** [`emit_plugin()`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/emitter.rs) creates standardized plugin directories under `.ai/<plugin-name>/`.

**Plugin directory structure:**
```
.ai/<plugin-name>/
  skills/<name>/SKILL.md
  agents/<name>.md
  hooks/<hook-event>.json
  output-styles/<name>.md
  scripts/              # referenced scripts copied here
  plugin.json
```

**Two path rewriting mechanisms exist:**

1. **Skill path rewriting** ([`emitter.rs:771-773`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/emitter.rs#L771-L773)): Blanket string replacement converts `${CLAUDE_SKILL_DIR}/scripts/` to `${CLAUDE_SKILL_DIR}/../../scripts/` to account for nesting change when skills move into `skills/<name>/SKILL.md`.

2. **Hook command path rewriting** ([`emitter.rs:779-843`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/emitter.rs#L779-L843)): Relative script paths in hook `"command"` fields are resolved to absolute paths against the project root.

**What is NOT handled:**
- Arbitrary files referenced by relative paths outside the `${CLAUDE_SKILL_DIR}/scripts/` pattern
- No path rewriting in non-SKILL.md files within skill directories
- No path rewriting in MCP, LSP, or extension configurations
- No handling of "extra" files beyond scripts matching the known pattern

### 6. Orchestration (`cmd_migrate`)

**Entry point:** [`cmd_migrate()`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/aipm/src/main.rs#L617-L730)

Flow:
1. Parse CLI args into `Options`
2. Call `libaipm::migrate::migrate(options)`
3. Handle `Outcome`: print dry-run report path or list created plugins
4. If `destructive`: run cleanup wizard via `migrate_cleanup_prompt_steps()`

## Code References

- [`crates/libaipm/src/migrate/mod.rs`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/mod.rs) — Main orchestrator, types, `migrate()` entry
- [`crates/libaipm/src/migrate/discovery.rs`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/discovery.rs) — Source directory discovery
- [`crates/libaipm/src/migrate/detector.rs`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/detector.rs) — Detector trait and factories
- [`crates/libaipm/src/migrate/skill_common.rs`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/skill_common.rs) — Script extraction and shared utilities
- [`crates/libaipm/src/migrate/emitter.rs`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/emitter.rs) — Plugin file generation and path rewriting
- [`crates/libaipm/src/migrate/dry_run.rs`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/dry_run.rs) — Dry-run report generation
- [`crates/libaipm/src/migrate/cleanup.rs`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/cleanup.rs) — Post-migration cleanup
- [`crates/libaipm/src/migrate/registrar.rs`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/src/migrate/registrar.rs) — Marketplace registration
- [`crates/aipm/src/main.rs:617-730`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/aipm/src/main.rs#L617-L730) — CLI entry point
- [`crates/aipm/tests/migrate_e2e.rs`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/aipm/tests/migrate_e2e.rs) — E2E tests
- [`crates/libaipm/tests/bdd.rs`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/crates/libaipm/tests/bdd.rs) — BDD step definitions
- [`tests/features/manifest/migrate.feature`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/tests/features/manifest/migrate.feature) — Cucumber feature file

## Architecture Documentation

### Three-Stage Pipeline
```
Source Dirs -> [Discovery] -> DiscoveredSource[]
                                   |
                                   v
                           [Detection] (N detectors per source type)
                                   |
                                   v
                              Artifact[]
                                   |
                          +--------+--------+
                          |                 |
                     [dry_run?]        [emit + register]
                          |                 |
                          v                 v
                    Markdown Report    .ai/<plugin>/
```

### Detector Architecture
Each detector is independent and additive. The `Detector` trait's `detect()` method receives a source directory path and returns `Vec<Artifact>`. All detector results are concatenated — there is no subtraction step to identify unclaimed files.

### Key Types
- `ArtifactKind` — 8-variant enum classifying file types
- `Artifact` — Detected file with kind, metadata, source path, referenced_scripts
- `ArtifactMetadata` — Name, description, hooks, model settings, raw content
- `PluginPlan` — Groups artifacts into plugins for emission
- `PluginEntry` — A single plugin output (name, artifacts, target path)
- `Options` — CLI args (dry_run, destructive, source, max_depth, manifest, dir)
- `Action` — Enum of migration outcomes (PluginCreated, Skipped, DryRunReport, etc.)
- `Outcome` — Aggregates Vec<Action> results

## Historical Context (from research/)

- [`research/docs/2026-03-23-aipm-migrate-command.md`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/research/docs/2026-03-23-aipm-migrate-command.md) — Original migrate command research
- [`research/docs/2026-03-23-recursive-claude-discovery-parallel-migrate.md`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/research/docs/2026-03-23-recursive-claude-discovery-parallel-migrate.md) — Recursive discovery research
- [`research/docs/2026-03-24-migrate-all-artifact-types.md`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/research/docs/2026-03-24-migrate-all-artifact-types.md) — All artifact types expansion
- [`research/tickets/2026-03-27-111-aipm-migrate-destructive-flag.md`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/research/tickets/2026-03-27-111-aipm-migrate-destructive-flag.md) — Destructive flag ticket research
- [`specs/2026-03-23-aipm-migrate-command.md`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/specs/2026-03-23-aipm-migrate-command.md) — Original migrate spec
- [`specs/2026-03-24-migrate-all-artifact-types.md`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/specs/2026-03-24-migrate-all-artifact-types.md) — All artifact types spec

## Related Research

Sub-agent research documents created during this investigation:
- [`research/docs/2026-04-01-migrate-file-discovery-classification.md`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/research/docs/2026-04-01-migrate-file-discovery-classification.md) — Deep dive on discovery and classification
- [`research/docs/2026-04-01-migrate-dry-run-report.md`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/research/docs/2026-04-01-migrate-dry-run-report.md) — Dry-run report generation analysis
- [`research/docs/2026-04-01-migrate-dependency-tracking.md`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/research/docs/2026-04-01-migrate-dependency-tracking.md) — Dependency tracking and script references
- [`research/docs/2026-04-01-migrate-file-movement-paths.md`](https://github.com/TheLarkInn/aipm/blob/d381cd9fadbff3f637f3cbf4227e444e8794ffe0/research/docs/2026-04-01-migrate-file-movement-paths.md) — File movement and path rewriting

## Open Questions

1. **"Other files" detector scope:** Should a new detector enumerate ALL files in source directories, or should the orchestrator compare claimed files against a full directory listing? The latter avoids touching the detector trait interface.

2. **Dependency association heuristics:** Issue #123 wants "other files" matched to the skill/agent that references them. Currently `extract_script_references()` only matches the `${CLAUDE_SKILL_DIR}/scripts/` pattern. What about other reference patterns (e.g., `./utils.py`, `../shared/lib.sh`)?

3. **Agent script references:** `AgentDetector` hardcodes `referenced_scripts: Vec::new()`. Should agents support the same `extract_script_references()` mechanism as skills?

4. **Path rewriting completeness:** Hook commands are rewritten to absolute paths, but skills use relative `../../scripts/` paths. Should there be a unified path rewriting strategy?

5. **Non-`.claude/` "other files":** Should the system look for referenced files outside the `.claude/` directory itself (e.g., scripts at the project root referenced by hooks)?
