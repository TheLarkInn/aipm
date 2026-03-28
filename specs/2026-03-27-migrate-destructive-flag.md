# `aipm migrate --destructive` Flag and Post-Migration Cleanup Wizard

| Document Metadata      | Details                                                                                                                                              |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| Author(s)              | selarkin                                                                                                                                             |
| Status                 | Draft (WIP)                                                                                                                                          |
| Team / Owner           | AI Dev Tooling                                                                                                                                       |
| Created / Last Updated | 2026-03-27                                                                                                                                           |
| GitHub Issue           | [#111](https://github.com/TheLarkInn/aipm/issues/111)                                                                                               |
| Research               | [research/tickets/2026-03-27-111-aipm-migrate-destructive-flag.md](../research/tickets/2026-03-27-111-aipm-migrate-destructive-flag.md)              |
| Depends on             | [specs/2026-03-23-aipm-migrate-command.md](2026-03-23-aipm-migrate-command.md), [specs/2026-03-22-interactive-init-wizard.md](2026-03-22-interactive-init-wizard.md) |

## 1. Executive Summary

This spec adds an `aipm migrate --destructive` flag and a post-migration interactive cleanup prompt to the existing migrate command (issue [#111](https://github.com/TheLarkInn/aipm/issues/111)). Today, `aipm migrate` copies artifacts from `.claude/` into `.ai/` plugins but never removes the originals, leaving stale duplicate files. The `--destructive` flag removes successfully-migrated source files after migration completes. When `--destructive` is omitted and the CLI is running in an interactive terminal, a wizard step prompts the user with a yes/no question asking if they want to remove the migrated source files. When `--destructive` is passed, the wizard step is skipped entirely. In non-TTY/CI contexts without `--destructive`, cleanup is silently skipped. The implementation follows established codebase patterns: clap derive for the flag, the two-layer wizard architecture (`wizard.rs` + `wizard_tty.rs`) for the prompt, and the `Fs` trait for testable file deletion.

## 2. Context and Motivation

### 2.1 Current State

The `aipm migrate` command (implemented in [specs/2026-03-23-aipm-migrate-command.md](2026-03-23-aipm-migrate-command.md)) scans `.claude/` directories for skills, commands, agents, MCP servers, hooks, and output styles, then copies each into a plugin directory under `.ai/`. The original files are **never modified or deleted** -- this was an explicit design decision documented in the original spec (line 55: "copied (non-destructive)") and the original research (line 394: "Copy is safer and non-destructive").

```
project/
├── .claude/               <-- originals remain untouched
│   ├── skills/deploy/
│   ├── commands/review.md
│   └── settings.json
├── .ai/                   <-- migrated copies live here
│   ├── deploy/
│   │   └── skills/deploy/SKILL.md
│   ├── review/
│   │   └── skills/review/SKILL.md
│   └── .claude-plugin/
│       └── marketplace.json
```

After a successful migration, the `.claude/` artifacts exist in two places: the original location and the `.ai/` plugin directory. Users must manually delete the originals.

### 2.2 The Problem

| Problem | Impact |
|---------|--------|
| Migrated files remain in `.claude/` after migration | Duplicate configurations create confusion about which is the "source of truth" |
| No automated cleanup path exists | Users must manually identify and delete migrated files |
| No interactive guidance for new users | Users may not realize originals should be removed |
| CI/automation workflows cannot clean up in one step | Requires a separate `rm -rf .claude/` step after migration |

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [ ] Add `--destructive` flag to `aipm migrate` that removes successfully-migrated source files after migration
- [ ] Only migrated artifact files are deleted -- unmigrated files within `.claude/` are preserved
- [ ] Deletion is all-or-nothing: if any artifact fails to migrate, no source files are removed
- [ ] When `--destructive` is not passed and stdin is a terminal, prompt with a yes/no wizard step asking to remove old copies (default: No)
- [ ] When `--destructive` is passed, the wizard step never triggers
- [ ] In non-TTY mode (piped/CI) without `--destructive`, cleanup is silently skipped
- [ ] `--dry-run --destructive` shows what would be deleted in the dry-run report without deleting anything
- [ ] In recursive mode, `--destructive` applies to all discovered `.claude/` directories
- [ ] New `Action` variants report which source files/directories were removed
- [ ] The `Fs` trait's existing `remove_file()` and `remove_dir_all()` methods are used for testable deletion
- [ ] Wizard follows the existing two-layer architecture (`wizard.rs` pure logic + `wizard_tty.rs` TTY bridge)
- [ ] All new code meets the project's 89% branch coverage requirement

### 3.2 Non-Goals (Out of Scope)

- [ ] We will NOT delete the entire `.claude/` directory -- only files that were successfully migrated
- [ ] We will NOT add a `--yes` flag to `migrate` (the `--destructive` flag serves the "skip prompt and delete" purpose)
- [ ] We will NOT modify the `aipm-pack` binary
- [ ] We will NOT change any existing migration behavior (copy logic, detection, emission, registration)
- [ ] We will NOT add rollback/undo functionality (users can re-create from git history)

## 4. Proposed Solution (High-Level Design)

### 4.1 Architecture

The feature adds a post-migration cleanup phase to the existing migrate pipeline. The cleanup phase operates after all artifacts have been successfully emitted and registered.

```
┌─────────────────────────────────────────────────────────┐
│                   aipm migrate                          │
│                                                         │
│  ┌──────────┐   ┌──────────┐   ┌───────────┐          │
│  │ Discover │──▶│ Detect + │──▶│ Register  │          │
│  │ .claude/ │   │  Emit    │   │marketplace│          │
│  └──────────┘   └──────────┘   └─────┬─────┘          │
│                                      │                  │
│                                      ▼                  │
│                              ┌──────────────┐           │
│                              │ Cleanup      │ NEW       │
│                              │ Decision     │           │
│                              └──────┬───────┘           │
│                    ┌────────────────┼────────────────┐  │
│                    ▼                ▼                ▼   │
│             --destructive    Interactive TTY    Non-TTY  │
│             (auto-delete)    (prompt yes/no)  (skip)    │
│                    │                │                    │
│                    ▼                ▼                    │
│              ┌───────────────────────┐                  │
│              │ Remove migrated       │                  │
│              │ source files via Fs   │                  │
│              └───────────────────────┘                  │
└─────────────────────────────────────────────────────────┘
```

### 4.2 Architectural Pattern

The cleanup phase follows the existing **Action-based outcome reporting** pattern. Cleanup actions (`SourceFileRemoved`, `SourceDirRemoved`) are appended to the `Outcome.actions` vector alongside the existing migration actions.

The wizard follows the established **two-layer wizard** pattern from `aipm init` ([specs/2026-03-22-interactive-init-wizard.md](2026-03-22-interactive-init-wizard.md)):
- Pure logic in `wizard.rs` (testable via snapshots)
- TTY bridge in `wizard_tty.rs` (excluded from coverage)

### 4.3 Key Components

| Component | Responsibility | Location | Justification |
|-----------|---------------|----------|---------------|
| `--destructive` flag | CLI argument, skip wizard | `crates/aipm/src/main.rs` | Follows existing clap derive pattern |
| Cleanup decision logic | Determine whether/what to delete | `crates/libaipm/src/migrate/cleanup.rs` (new) | Separation from core migrate logic |
| Migrate wizard | Pure prompt step definitions | `crates/aipm/src/wizard.rs` | Reuse existing `PromptStep`/`PromptKind` types |
| Migrate wizard TTY | Execute cleanup prompt | `crates/aipm/src/wizard_tty.rs` | Reuse existing `execute_prompts()` dispatch |
| `Action::SourceFileRemoved` | Report deleted files | `crates/libaipm/src/migrate/mod.rs` | Extends existing Action enum |
| `Action::SourceDirRemoved` | Report deleted empty dirs | `crates/libaipm/src/migrate/mod.rs` | Extends existing Action enum |

## 5. Detailed Design

### 5.1 CLI Changes

Add the `destructive` flag to the `Commands::Migrate` variant in `crates/aipm/src/main.rs`:

```rust
Migrate {
    // ... existing flags ...

    /// Remove migrated source files after successful migration.
    /// When omitted, an interactive prompt asks whether to clean up (TTY only).
    #[arg(long)]
    destructive: bool,

    // ... dir ...
},
```

Update the dispatch match arm in `run()`:

```rust
Some(Commands::Migrate { dry_run, destructive, source, max_depth, manifest, dir }) => {
    cmd_migrate(dry_run, destructive, source.as_deref(), max_depth, manifest, dir)
},
```

### 5.2 `cmd_migrate()` Handler Changes

The handler gains the `destructive` parameter and a post-migration cleanup phase:

```rust
fn cmd_migrate(
    dry_run: bool,
    destructive: bool,
    source: Option<&str>,
    max_depth: Option<usize>,
    manifest: bool,
    dir: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = resolve_dir(dir)?;
    let opts = libaipm::migrate::Options { dir: &dir, source, dry_run, max_depth, manifest };

    let result = libaipm::migrate::migrate(&opts, &libaipm::fs::Real)?;

    // Print migration actions (existing logic, unchanged)
    let mut stdout = std::io::stdout();
    for action in &result.actions { /* ... existing match arms ... */ }

    // --- NEW: Post-migration cleanup phase ---
    // Skip cleanup if dry-run (deletion info is already in the report),
    // or if no artifacts were migrated.
    if dry_run || !result.has_migrated_artifacts() {
        return Ok(());
    }

    // Determine whether to clean up
    let should_clean = if destructive {
        true
    } else {
        let interactive = std::io::stdin().is_terminal();
        if interactive {
            wizard_tty::resolve_migrate_cleanup(interactive, &result)?
        } else {
            false // Non-TTY without --destructive: silently skip
        }
    };

    if should_clean {
        let cleanup_actions = libaipm::migrate::cleanup::remove_migrated_sources(
            &result, &libaipm::fs::Real,
        )?;
        for action in &cleanup_actions {
            match action {
                libaipm::migrate::Action::SourceFileRemoved { path } => {
                    let _ = writeln!(stdout, "Removed source: {}", path.display());
                },
                libaipm::migrate::Action::SourceDirRemoved { path } => {
                    let _ = writeln!(stdout, "Removed empty directory: {}", path.display());
                },
                _ => {},
            }
        }
    }

    Ok(())
}
```

### 5.3 Data Model Changes

#### 5.3.1 `Options` struct -- no changes

The `destructive` flag is handled at the CLI layer (in `cmd_migrate()`), not in the library's `Options` struct. The library `migrate()` function remains pure: it copies artifacts and reports what it did. Cleanup is a separate post-migration step. This keeps the library layer free of I/O policy decisions.

#### 5.3.2 `Action` enum additions

Add two new variants to `Action` in `crates/libaipm/src/migrate/mod.rs`:

```rust
pub enum Action {
    // ... existing variants ...

    /// A migrated source file was removed.
    SourceFileRemoved {
        /// Path to the removed file.
        path: PathBuf,
    },
    /// An empty source directory was removed after file cleanup.
    SourceDirRemoved {
        /// Path to the removed directory.
        path: PathBuf,
    },
}
```

#### 5.3.3 `Outcome` helper method

Add a method to `Outcome` to check whether any artifacts were actually migrated:

```rust
impl Outcome {
    /// Returns `true` if at least one `PluginCreated` action exists.
    pub fn has_migrated_artifacts(&self) -> bool {
        self.actions.iter().any(|a| matches!(a, Action::PluginCreated { .. }))
    }

    /// Returns the source paths of all successfully migrated artifacts.
    pub fn migrated_source_paths(&self) -> Vec<&Path> {
        self.actions.iter().filter_map(|a| match a {
            Action::PluginCreated { source, .. } => Some(source.as_path()),
            _ => None,
        }).collect()
    }
}
```

### 5.4 Cleanup Module

Create `crates/libaipm/src/migrate/cleanup.rs`:

```rust
use super::{Action, Outcome};
use crate::fs::Fs;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Remove successfully-migrated source files and any resulting empty directories.
///
/// Only files whose `source_path` appears in a `PluginCreated` action are removed.
/// After file removal, empty parent directories are pruned up to (but not including)
/// the `.claude/` directory itself.
pub fn remove_migrated_sources(
    outcome: &Outcome,
    fs: &dyn Fs,
) -> Result<Vec<Action>, std::io::Error> {
    let mut actions = Vec::new();
    let mut dirs_to_check: BTreeSet<PathBuf> = BTreeSet::new();

    for source_path in outcome.migrated_source_paths() {
        if source_path.is_file() {
            fs.remove_file(source_path)?;
            actions.push(Action::SourceFileRemoved { path: source_path.to_path_buf() });
            if let Some(parent) = source_path.parent() {
                dirs_to_check.insert(parent.to_path_buf());
            }
        } else if source_path.is_dir() {
            // Skill directories contain multiple files -- remove recursively
            fs.remove_dir_all(source_path)?;
            actions.push(Action::SourceDirRemoved { path: source_path.to_path_buf() });
            if let Some(parent) = source_path.parent() {
                dirs_to_check.insert(parent.to_path_buf());
            }
        }
    }

    // Prune empty parent directories (e.g., .claude/skills/ if all skills migrated)
    // Walk from deepest to shallowest. Stop at .claude/ (do not remove .claude/ itself
    // unless it becomes completely empty after all child removals).
    // Sort by depth descending so deeper dirs are checked first.
    let mut sorted_dirs: Vec<PathBuf> = dirs_to_check.into_iter().collect();
    sorted_dirs.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

    for dir in sorted_dirs {
        if is_dir_empty(fs, &dir)? {
            fs.remove_dir_all(&dir)?;
            actions.push(Action::SourceDirRemoved { path: dir });
        }
    }

    Ok(actions)
}

fn is_dir_empty(fs: &dyn Fs, path: &Path) -> Result<bool, std::io::Error> {
    let entries = fs.read_dir(path)?;
    Ok(entries.is_empty())
}
```

#### 5.4.1 What gets deleted -- artifact-level granularity

The cleanup module uses `Outcome::migrated_source_paths()` to identify what to delete. Each `Action::PluginCreated` already carries a `source: PathBuf` field pointing to the original artifact location. This is the unit of deletion:

| Artifact Kind | `source` Path | What Gets Removed |
|--------------|---------------|-------------------|
| Skill | `.claude/skills/deploy/` | Entire directory recursively |
| Command | `.claude/commands/review.md` | Single file |
| Agent | `.claude/agents/my-agent.md` | Single file |
| MCP Server | `.mcp.json` (project root) | Single file |
| Hook | `.claude/settings.json` | **NOT removed** -- hooks are extracted *from* `settings.json` but the file may contain non-hook config |
| Output Style | `.claude/output-styles/concise.md` | Single file |

**Special case -- hooks and settings.json:** The `HookDetector` reads the `hooks` key from `.claude/settings.json` but does not own the entire file. Since `settings.json` may contain other configuration (e.g., `enabledTools`, `allowedDirectories`), it must NOT be deleted during cleanup. The cleanup module should skip source paths that point to `settings.json`. Similarly, `.mcp.json` is at the project root and may be used by tools other than Claude Code -- it should also be skipped by default. These files should be flagged in the wizard output as "not removed (shared config)".

**Revised skip list:** Source files named `settings.json` or `.mcp.json` are excluded from deletion. The `PluginCreated` action still records them, but the cleanup module filters them out.

#### 5.4.2 Empty directory pruning

After individual files/directories are removed, the cleanup module checks parent directories bottom-up:
- `.claude/skills/` -- removed if empty (all skills migrated)
- `.claude/commands/` -- removed if empty (all commands migrated)
- `.claude/agents/` -- removed if empty
- `.claude/output-styles/` -- removed if empty
- `.claude/` itself -- removed only if completely empty after all child removals

### 5.5 Wizard Implementation

#### 5.5.1 Pure logic layer (`crates/aipm/src/wizard.rs`)

Add a function to generate the migrate cleanup prompt step:

```rust
/// Generates the post-migration cleanup prompt step.
/// Returns an empty vec if there is nothing to clean up.
pub fn migrate_cleanup_prompt_steps(migrated_count: usize) -> Vec<PromptStep> {
    if migrated_count == 0 {
        return Vec::new();
    }

    vec![PromptStep {
        label: "Remove original source files that were migrated?",
        kind: PromptKind::Confirm { default: false },
        help: Some(
            "The migrated files have been copied to .ai/ plugins. \
             Answering 'yes' removes the originals from .claude/. \
             Use --destructive to skip this prompt."
        ),
    }]
}

/// Resolves the migrate cleanup wizard answer.
/// Returns `true` if the user chose to remove source files.
pub fn resolve_migrate_cleanup_answer(answers: &[PromptAnswer]) -> bool {
    matches!(answers.first(), Some(PromptAnswer::Bool(true)))
}
```

#### 5.5.2 TTY bridge layer (`crates/aipm/src/wizard_tty.rs`)

Add a public function for the migrate cleanup prompt:

```rust
/// Prompts the user about removing migrated source files.
/// Returns `true` if the user confirmed cleanup.
pub fn resolve_migrate_cleanup(
    interactive: bool,
    outcome: &libaipm::migrate::Outcome,
) -> Result<bool, Box<dyn std::error::Error>> {
    if !interactive {
        return Ok(false);
    }

    let migrated_count = outcome.migrated_source_paths().len();
    let steps = wizard::migrate_cleanup_prompt_steps(migrated_count);

    if steps.is_empty() {
        return Ok(false);
    }

    inquire::set_global_render_config(wizard::styled_render_config());
    let answers = execute_prompts(&steps)?;
    Ok(wizard::resolve_migrate_cleanup_answer(&answers))
}
```

### 5.6 Dry-Run Report Changes

When `--dry-run --destructive` is passed, the dry-run report should include a "Cleanup Plan" section listing what would be deleted. This is handled in `dry_run.rs`:

Add to `generate_report()` and `generate_recursive_report()`:

```markdown
## Cleanup Plan (--destructive)

The following source files would be removed after migration:

- `.claude/skills/deploy/` (directory)
- `.claude/commands/review.md` (file)
- `.claude/agents/my-agent.md` (file)

**Skipped (shared config):**
- `.claude/settings.json` (contains non-hook configuration)
- `.mcp.json` (may be used by other tools)
```

The `Options` struct does NOT gain a `destructive` field. Instead, `cmd_migrate()` passes a `destructive: bool` parameter to the dry-run report generator via a new `DryRunOptions` struct or by extending the existing report generation function signature.

### 5.7 Interaction Matrix

| `--destructive` | `--dry-run` | TTY? | Behavior |
|-----------------|-------------|------|----------|
| No | No | Yes | Migrate, then prompt "Remove originals?" (default: No) |
| No | No | No | Migrate only, no cleanup, no prompt |
| Yes | No | Yes | Migrate, then auto-remove migrated sources (no prompt) |
| Yes | No | No | Migrate, then auto-remove migrated sources (no prompt) |
| No | Yes | Any | Generate report (no cleanup section) |
| Yes | Yes | Any | Generate report WITH cleanup plan section |

### 5.8 All-or-Nothing Error Semantics

If the migration itself encounters an error (e.g., `FrontmatterParse`, `ConfigParse`, `Io`), the `migrate()` function returns `Err(...)` and no `Outcome` is produced. In this case, `cmd_migrate()` propagates the error and cleanup never runs.

If migration succeeds for some artifacts but skips others (producing `Action::Skipped`), the migration is still considered successful -- only the `PluginCreated` sources are candidates for cleanup. Skipped artifacts are not touched.

The all-or-nothing guarantee applies to the cleanup phase itself: if any `remove_file()` or `remove_dir_all()` call fails during cleanup, the error is propagated and the remaining files are left in place. The actions already recorded (files already deleted before the error) are printed so the user knows what was removed.

## 6. Alternatives Considered

| Option | Pros | Cons | Reason for Rejection |
|--------|------|------|---------------------|
| `--destructive` deletes entire `.claude/` | Simpler implementation, clean slate | Destroys unmigrated config (`settings.json`, undetected files) | Too aggressive -- users may have custom config not handled by detectors |
| Separate `aipm clean` command | Clear separation of concerns | Requires tracking "what was migrated" across invocations | Over-engineering for v1; can be added later if needed |
| `--move` instead of `--destructive` | Familiar `mv` semantics | Misleading -- migration is not a simple move (files are transformed, paths rewritten) | The term "destructive" better communicates the irreversible nature |
| Always prompt (no `--destructive` flag) | Simpler API surface | Breaks CI/automation, can't be scripted | CI users need a non-interactive path |

## 7. Cross-Cutting Concerns

### 7.1 Safety

- **No data loss without intent:** Cleanup only runs when the user explicitly passes `--destructive` or answers "yes" to the prompt.
- **Non-TTY safety:** When stdin is not a terminal and `--destructive` is not passed, cleanup is silently skipped. This prevents accidental data loss in CI pipelines.
- **Shared config preservation:** `settings.json` and `.mcp.json` are never deleted because they contain configuration beyond what the migrate command handles.
- **Fs trait testability:** All deletion goes through the `Fs` trait, enabling mock-based testing without touching the real filesystem.

### 7.2 Testing Strategy

| Layer | Test Type | What's Covered |
|-------|-----------|----------------|
| `wizard.rs` | Unit (insta snapshots) | Prompt step generation, answer resolution, edge cases (0 migrated) |
| `cleanup.rs` | Unit (mock Fs) | File removal, directory pruning, skip list, error propagation |
| `mod.rs` | Unit | `Outcome::has_migrated_artifacts()`, `migrated_source_paths()` |
| `dry_run.rs` | Unit | Report includes cleanup plan when destructive=true |
| E2E (`migrate_e2e.rs`) | Integration | `--destructive` removes source files, originals preserved without flag |
| BDD (`.feature`) | Acceptance | New scenarios for destructive flag and source preservation |

### 7.3 Coverage

The wizard TTY bridge (`wizard_tty.rs`) is excluded from coverage per existing convention. All other new code must meet the 89% branch coverage requirement. The `cleanup.rs` module should target near-100% branch coverage since it handles destructive operations.

## 8. Migration, Rollout, and Testing

### 8.1 Implementation Order

1. Add `Action::SourceFileRemoved` and `Action::SourceDirRemoved` variants to the `Action` enum
2. Add `Outcome::has_migrated_artifacts()` and `Outcome::migrated_source_paths()` helper methods
3. Implement `cleanup.rs` module with `remove_migrated_sources()` and unit tests (mock Fs)
4. Add `--destructive` flag to `Commands::Migrate` and update `cmd_migrate()` handler
5. Add wizard prompt step in `wizard.rs` with snapshot tests
6. Add TTY bridge function in `wizard_tty.rs`
7. Update dry-run report generation for `--destructive` flag
8. Add BDD scenarios to `migrate.feature`
9. Add E2E tests to `migrate_e2e.rs`
10. Run full coverage check

### 8.2 New BDD Scenarios

```gherkin
Rule: Source files can be removed after migration

  Scenario: --destructive removes migrated skill source files
    Given an empty directory "my-project"
    And a workspace initialized in "my-project"
    And a skill "deploy" exists in "my-project"
    When the user runs "aipm migrate --destructive" in "my-project"
    Then the command succeeds
    And a plugin directory exists at ".ai/deploy/" in "my-project"
    And there is no file ".claude/skills/deploy/SKILL.md" in "my-project"
    And there is no directory ".claude/skills/deploy/" in "my-project"

  Scenario: --destructive preserves settings.json
    Given an empty directory "my-project"
    And a workspace initialized in "my-project"
    And hooks exist in "my-project" settings.json
    When the user runs "aipm migrate --destructive" in "my-project"
    Then the command succeeds
    And a file ".claude/settings.json" exists in "my-project"

  Scenario: Without --destructive, source files are preserved (non-TTY)
    Given an empty directory "my-project"
    And a workspace initialized in "my-project"
    And a skill "deploy" exists in "my-project"
    When the user runs "aipm migrate" in "my-project"
    Then a file ".claude/skills/deploy/SKILL.md" exists in "my-project"

  Scenario: --destructive with --dry-run shows cleanup plan
    Given an empty directory "my-project"
    And a workspace initialized in "my-project"
    And a skill "deploy" exists in "my-project"
    When the user runs "aipm migrate --dry-run --destructive" in "my-project"
    Then a file "aipm-migrate-dryrun-report.md" exists in "my-project"
    And the file "aipm-migrate-dryrun-report.md" in "my-project" contains "Cleanup Plan"
    And a file ".claude/skills/deploy/SKILL.md" exists in "my-project"

  Scenario: --destructive in recursive mode cleans all discovered sources
    Given an empty directory "my-project"
    And a workspace initialized in "my-project"
    And a skill "deploy" exists in sub-package "auth" of "my-project"
    And a skill "lint" exists in "my-project"
    When the user runs "aipm migrate --destructive" in "my-project"
    Then the command succeeds
    And there is no directory ".claude/skills/lint/" in "my-project"
    And there is no directory "auth/.claude/skills/deploy/" in "my-project"

  Scenario: Empty .claude/ subdirectories are pruned after cleanup
    Given an empty directory "my-project"
    And a workspace initialized in "my-project"
    And a skill "deploy" exists in "my-project"
    When the user runs "aipm migrate --destructive" in "my-project"
    Then there is no directory ".claude/skills/" in "my-project"
```

### 8.3 Test Plan

- **Unit Tests:**
  - `cleanup.rs`: File removal with mock Fs, directory pruning, skip list for `settings.json`/`.mcp.json`, error propagation on `remove_file` failure, empty `Outcome` produces no actions
  - `wizard.rs`: Snapshot tests for `migrate_cleanup_prompt_steps()` with 0 and N migrated artifacts, `resolve_migrate_cleanup_answer()` with Yes/No/empty
  - `mod.rs`: `has_migrated_artifacts()` and `migrated_source_paths()` with various action combinations
- **Integration Tests:**
  - `migrate_e2e.rs`: `--destructive` flag removes files, without flag files preserved, `--dry-run --destructive` shows plan, recursive cleanup
- **End-to-End Tests:**
  - BDD scenarios in `migrate.feature` as defined above

## 9. Open Questions / Unresolved Issues

All open questions from the research phase have been resolved via user input:

- [x] **Scope of deletion:** Only migrated files (not entire `.claude/`) -- resolved
- [x] **Partial failure:** All-or-nothing (no deletion if any migration fails) -- resolved
- [x] **`--dry-run` interaction:** Show cleanup plan in report -- resolved
- [x] **Recursive scope:** Applies to all discovered directories -- resolved
- [x] **Wizard default:** Default to "No" (safer) -- resolved
- [x] **Non-TTY behavior:** Silently skip cleanup -- resolved

**Remaining implementation detail:**
- [ ] Should the wizard show a count of files to be deleted, or list them individually? (Recommend: show count for brevity, with `--dry-run --destructive` for the full list)
