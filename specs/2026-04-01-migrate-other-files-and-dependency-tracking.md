# `aipm migrate` — Other Files Detection, Dependency Tracking, and Path Rewriting

| Document Metadata      | Details                                                                                                          |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Author(s)              | selarkin                                                                                                         |
| Status                 | Draft (WIP)                                                                                                      |
| Team / Owner           | AI Dev Tooling                                                                                                   |
| Created / Last Updated | 2026-04-01                                                                                                       |
| Research               | [research/tickets/2026-04-01-123-migrate-other-files-handling.md](../research/tickets/2026-04-01-123-migrate-other-files-handling.md) |
| GitHub Issue           | [#123](https://github.com/TheLarkInn/aipm/issues/123)                                                           |
| Depends on             | [specs/2026-03-23-aipm-migrate-command.md](2026-03-23-aipm-migrate-command.md), [specs/2026-03-24-migrate-all-artifact-types.md](2026-03-24-migrate-all-artifact-types.md), [specs/2026-03-27-migrate-destructive-flag.md](2026-03-27-migrate-destructive-flag.md) |

## 1. Executive Summary

When `aipm migrate` runs today, files that don't match any detector pattern (scripts, utilities, READMEs, configuration fragments) are silently dropped. This spec adds three capabilities: (1) an orchestrator-level diff that identifies **every** file in a source directory and flags those not claimed by any detector as "other files," (2) dependency association that links other files to the skill/agent/hook that references them, and (3) migration of those dependencies alongside their parent artifact with correct path rewriting. Unassociated other files are moved to the plugin root and flagged with warnings in the dry-run report. External files (outside `.claude/`) that are referenced by migrated artifacts are warned about but not moved — their paths in the migrated artifact content are rewritten to remain valid.

Key design decisions: orchestrator-level file enumeration (no new `Detector` impl), expanded `extract_script_references()` to match relative paths, agent script extraction parity with skills, and per-artifact-type path rewriting preserved (no unification).

## 2. Context and Motivation

### 2.1 Current State

The migrate pipeline ([`mod.rs:289-530`](../crates/libaipm/src/migrate/mod.rs)) follows a three-stage architecture established in [specs/2026-03-23-aipm-migrate-command.md](2026-03-23-aipm-migrate-command.md):

```
Source Dirs ─► [Discovery] ─► DiscoveredSource[]
                                     │
                                     ▼
                             [Detection] (N detectors per source type)
                                     │
                                     ▼
                                Artifact[]
                                     │
                            ┌────────┴────────┐
                            │                 │
                       [dry_run?]        [emit + register]
                            │                 │
                            ▼                 ▼
                      Markdown Report    .ai/<plugin>/
```

Each detector independently scans for files it recognizes. Results are concatenated. There is no step that enumerates all files in a source directory to compare against what detectors claimed.

Current `.claude/` directory layout:
```
.claude/
├── skills/
│   └── my-skill/
│       ├── SKILL.md          ← detected by SkillDetector
│       └── scripts/
│           └── helper.sh     ← collected as skill files
├── commands/
│   └── deploy.md             ← detected by CommandDetector
├── agents/
│   └── reviewer.md           ← detected by AgentDetector
├── output-styles/
│   └── concise.md            ← detected by OutputStyleDetector
├── settings.json              ← parsed by McpDetector + HookDetector
├── utils/                     ← SILENTLY DROPPED
│   └── shared-lib.sh         ← SILENTLY DROPPED
├── setup.py                   ← SILENTLY DROPPED
└── README.md                  ← SILENTLY DROPPED
```

Dependency tracking is partial ([research/docs/2026-04-01-migrate-dependency-tracking.md](../research/docs/2026-04-01-migrate-dependency-tracking.md)):
- Skills and commands: `extract_script_references()` matches `${CLAUDE_SKILL_DIR}/scripts/*` patterns and copies scripts during emission
- Hooks: Script paths extracted from `"type": "command"` handlers
- Agents: **Hardcode** `referenced_scripts: Vec::new()` — no extraction at all

### 2.2 The Problem

| Problem | Impact |
|---------|--------|
| Files in `.claude/` that don't match any detector are silently dropped during migration | Users lose utility scripts, shared libraries, and documentation without warning |
| The `--dry-run` report has no concept of "unclassified files" | Users cannot review what will be missed before committing to migration |
| `AgentDetector` does not extract script references | Agent-referenced scripts are orphaned after migration |
| `extract_script_references()` only matches `${CLAUDE_SKILL_DIR}/scripts/*` | Scripts referenced via relative paths (`./utils.py`, `../shared/lib.sh`) are missed |
| External file references (outside `.claude/`) become broken after migration | Skills/agents that reference project-root scripts break silently |

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [ ] After all detectors run, enumerate every file in the source directory and identify those not claimed by any detector as "other files"
- [ ] In `--dry-run` mode, report "other files" in a dedicated section with warning markers (⚠️) for easy visual identification
- [ ] In live migration mode, log "other files" to stderr for visibility
- [ ] Expand `extract_script_references()` to also match relative path patterns (`./path`, `../path`, bare `scripts/foo.sh`)
- [ ] Add script reference extraction to `AgentDetector` (parity with `SkillDetector`)
- [ ] Associate "other files" with the skill/agent/hook that references them as dependencies
- [ ] Migrate dependency-associated "other files" into the plugin directory alongside their parent artifact
- [ ] Move unassociated "other files" (no parent artifact references them) to the plugin root directory
- [ ] Rewrite paths in migrated artifact content so references to moved dependencies remain valid
- [ ] For external file references (outside `.claude/`): warn in report but do NOT move the file; DO rewrite the path in the migrated artifact so it remains valid from the new location
- [ ] All four cargo gates pass: `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`
- [ ] Branch coverage remains ≥ 89% per `cargo +nightly llvm-cov`

### 3.2 Non-Goals (Out of Scope)

- [ ] We will NOT add a new `Detector` implementation — file enumeration happens at the orchestrator level
- [ ] We will NOT persist dependency metadata in `plugin.json` or `aipm.toml` — dependency tracking is migration-time only
- [ ] We will NOT unify the path rewriting strategy across artifact types — skills keep relative `../../scripts/` rewrites, hooks keep absolute path rewrites
- [ ] We will NOT move files that live outside the `.claude/` (or `.github/`) source directory boundary — only warn and rewrite paths
- [ ] We will NOT add interactive prompts for "other files" decisions — behavior is deterministic
- [ ] We will NOT change the `Detector` trait signature

## 4. Proposed Solution (High-Level Design)

### 4.1 Architecture

The existing three-stage pipeline gains a fourth stage: **reconciliation**.

```
Source Dirs ─► [Discovery] ─► DiscoveredSource[]
                                     │
                                     ▼
                             [Detection] (N detectors per source type)
                                     │
                                     ▼
                                Artifact[]
                                     │
                                     ▼
                          [Reconciliation]  ◄── NEW
                           │           │
                    claimed files   other files
                           │           │
                           │    ┌──────┴──────┐
                           │    │             │
                           │  matched to   unmatched
                           │  parent artifact  │
                           │    │             │
                           ▼    ▼             ▼
                            ┌────────┴────────┐
                            │                 │
                       [dry_run?]        [emit + register]
                            │                 │
                            ▼                 ▼
                    Markdown Report      .ai/<plugin>/
                    (with ⚠️ section)   (with other files)
```

### 4.2 Architectural Pattern

Extension of the existing Scanner-Detector-Emitter pipeline from [specs/2026-03-23-aipm-migrate-command.md](2026-03-23-aipm-migrate-command.md). The reconciliation stage uses a **set-difference** approach: enumerate all files, subtract claimed files, classify the remainder by checking artifact `referenced_scripts` and content references.

### 4.3 Key Components

| Component | Responsibility | Location | Justification |
|-----------|---------------|----------|---------------|
| File Enumerator | Recursively list all files in a source directory | `libaipm::migrate::discovery` | Reuses existing `collect_files_recursive()` from `skill_common.rs` |
| Reconciler | Compute set difference between all files and claimed files; associate other files with parent artifacts | `libaipm::migrate::reconciler` (new file) | Isolates reconciliation logic from detection and emission |
| Expanded Script Extractor | Match relative paths in addition to `${CLAUDE_SKILL_DIR}/scripts/*` | `libaipm::migrate::skill_common` | Extends existing `extract_script_references()` |
| Dry-Run Other Files Section | Render ⚠️-marked warnings for unclassified files | `libaipm::migrate::dry_run` | Extends existing report generation |
| External Reference Rewriter | Rewrite paths to external files so they remain valid from the plugin's new location | `libaipm::migrate::emitter` | Extends existing path rewriting in emitter |

## 5. Detailed Design

### 5.1 New Type: `OtherFile`

**File:** `crates/libaipm/src/migrate/mod.rs`

Add a struct to represent files not claimed by any detector:

```rust
pub struct OtherFile {
    /// Absolute path to the file
    pub path: PathBuf,
    /// Path relative to the source directory
    pub relative_path: PathBuf,
    /// If this file is referenced by an artifact, the artifact's name
    pub associated_artifact: Option<String>,
    /// Whether this file lives outside the source directory (external reference)
    pub is_external: bool,
}
```

Extend `Artifact` to track which source files it claims:

```rust
// Existing field on Artifact:
pub files: Vec<PathBuf>,

// The `files` field already contains relative paths for skill directories.
// For single-file artifacts (commands, agents, output-styles), `files` is empty
// and `source_path` is the claimed file. The reconciler must account for both.
```

### 5.2 Reconciler Module

**New file:** `crates/libaipm/src/migrate/reconciler.rs`

```rust
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::migrate::{Artifact, OtherFile};
use crate::fs::Fs;
use crate::migrate::Error;

/// Identifies files in `source_dir` that are not claimed by any artifact.
/// Returns a list of OtherFile entries, each optionally associated with
/// a parent artifact if the artifact's content references the file.
pub fn reconcile(
    source_dir: &Path,
    artifacts: &[Artifact],
    fs: &dyn Fs,
) -> Result<Vec<OtherFile>, Error> {
    // 1. Enumerate all files recursively in source_dir
    let all_files = collect_all_files(source_dir, fs)?;

    // 2. Build claimed set from artifacts
    let claimed = build_claimed_set(source_dir, artifacts);

    // 3. Compute other files (set difference)
    let unclaimed: Vec<PathBuf> = all_files
        .into_iter()
        .filter(|f| !claimed.contains(f))
        .collect();

    // 4. For each unclaimed file, check if any artifact references it
    let other_files = associate_with_artifacts(source_dir, &unclaimed, artifacts);

    Ok(other_files)
}
```

**`collect_all_files()`** reuses the recursive walk pattern from [`skill_common::collect_files_recursive()`](../crates/libaipm/src/migrate/skill_common.rs) but returns absolute paths.

**`build_claimed_set()`** iterates all artifacts:
- For each artifact, adds `source_path` (absolute) to the claimed set
- For each artifact with non-empty `files`, resolves each relative path against `source_path.parent()` and adds to claimed set
- For config-based artifacts (McpServer, Hook), adds the `settings.json` path
- Script files inside skill directories are already covered by `files`

**`associate_with_artifacts()`** iterates each unclaimed file and checks:
1. Does any artifact's `referenced_scripts` contain a path that resolves to this file?
2. Does any artifact's `metadata.raw_content` contain the file's name or relative path as a substring?
3. If matched, set `associated_artifact = Some(artifact.name.clone())`
4. If no match, `associated_artifact = None`

### 5.3 Expanded Script Reference Extraction

**File:** `crates/libaipm/src/migrate/skill_common.rs`

Expand `extract_script_references()` to handle three pattern types:

```rust
/// Extracts script/file references from artifact content.
///
/// Matches three patterns:
/// 1. Variable-prefix: `${CLAUDE_SKILL_DIR}/scripts/helper.sh`
/// 2. Relative paths: `./scripts/helper.sh`, `../utils/lib.sh`
/// 3. Bare script invocations: `bash scripts/deploy.sh`, `python utils/run.py`
pub fn extract_script_references(content: &str, variable_prefix: &str) -> Vec<PathBuf> {
    let mut refs = Vec::new();

    for line in content.lines() {
        // Pattern 1: existing ${CLAUDE_SKILL_DIR}/scripts/* matching (unchanged)
        // ...existing logic...

        // Pattern 2: relative path references starting with ./ or ../
        // Match: ./path/to/file.ext or ../path/to/file.ext
        // Terminate at whitespace, quotes, backtick, closing paren
        // ...new logic...

        // Pattern 3: bare script invocations after known interpreters
        // Match: bash|sh|python|python3|node|ruby|perl followed by a path
        // Only if the path contains a directory separator or known extension
        // ...new logic...
    }

    refs.sort();
    refs.dedup();
    refs
}
```

The function signature remains identical — no breaking change. The `variable_prefix` parameter continues to control Pattern 1. Patterns 2 and 3 are unconditional.

### 5.4 Agent Script Reference Extraction

**File:** `crates/libaipm/src/migrate/agent_detector.rs`

Replace the hardcoded empty `referenced_scripts` with actual extraction:

```rust
// Before (current):
referenced_scripts: Vec::new(),

// After:
referenced_scripts: extract_script_references(
    &content,
    "${CLAUDE_AGENT_DIR}",
),
```

Use `${CLAUDE_AGENT_DIR}` as the variable prefix for Pattern 1 matching. Patterns 2 and 3 (relative paths, bare invocations) apply regardless.

Also apply to `CopilotAgentDetector` in `copilot_agent_detector.rs`.

### 5.5 Reconciler Integration into Orchestrator

**File:** `crates/libaipm/src/migrate/mod.rs`

In `migrate_single_source()` after detection (line ~349) and before the dry-run/emit branch:

```rust
// After: artifacts are collected from all detectors
let artifacts: Vec<Artifact> = /* existing detection logic */;

// NEW: Reconcile other files
let other_files = reconciler::reconcile(&source_dir, &artifacts, fs)?;

// Pass other_files to dry_run or emission
```

In `migrate_recursive()` after parallel detection (line ~447) and before the dry-run/emit branch:

```rust
// After: plugin_plans are built
// NEW: For each plan, reconcile other files against its artifacts
for plan in &mut plugin_plans {
    plan.other_files = reconciler::reconcile(&plan.source_dir, &plan.artifacts, fs)?;
}
```

Add `other_files: Vec<OtherFile>` field to `PluginPlan`:

```rust
pub struct PluginPlan {
    pub name: String,
    pub artifacts: Vec<Artifact>,
    pub other_files: Vec<OtherFile>,  // NEW
    pub is_package_scoped: bool,
    pub source_dir: PathBuf,
}
```

### 5.6 Dry-Run Report: Other Files Section

**File:** `crates/libaipm/src/migrate/dry_run.rs`

Add a new section to both `generate_report()` and `generate_recursive_report()`:

```rust
fn write_other_files_section(
    out: &mut String,
    other_files: &[OtherFile],
) {
    if other_files.is_empty() {
        return;
    }

    out.push_str("\n## ⚠️ Other Files (Unclassified)\n\n");
    out.push_str("The following files were found in the source directory but did not match ");
    out.push_str("any known artifact pattern. They will be moved to the plugin root.\n\n");

    // Group: associated with an artifact
    let associated: Vec<_> = other_files.iter()
        .filter(|f| f.associated_artifact.is_some())
        .collect();
    if !associated.is_empty() {
        out.push_str("### Dependencies (referenced by an artifact)\n\n");
        for f in &associated {
            // e.g.: "- `utils/helper.sh` → dependency of **my-skill** (will be migrated together)"
        }
    }

    // Group: unassociated
    let unassociated: Vec<_> = other_files.iter()
        .filter(|f| f.associated_artifact.is_none() && !f.is_external)
        .collect();
    if !unassociated.is_empty() {
        out.push_str("### ⚠️ Unassociated Files\n\n");
        out.push_str("These files are not referenced by any detected artifact. ");
        out.push_str("They will be moved to the plugin root directory.\n\n");
        for f in &unassociated {
            // e.g.: "- ⚠️ `README.md` → moved to plugin root"
        }
    }

    // Group: external references
    let external: Vec<_> = other_files.iter()
        .filter(|f| f.is_external)
        .collect();
    if !external.is_empty() {
        out.push_str("### ⚠️ External References (outside source directory)\n\n");
        out.push_str("These files are referenced by migrated artifacts but live outside the ");
        out.push_str("source directory. They will NOT be moved, but paths will be rewritten.\n\n");
        for f in &external {
            // e.g.: "- ⚠️ `../../scripts/deploy.sh` referenced by **my-hook** → path will be rewritten"
        }
    }
}
```

Update the summary table to include an "Other files" count row.

### 5.7 Emission: Migrate Other Files

**File:** `crates/libaipm/src/migrate/emitter.rs`

Add a function to copy other files into the plugin directory:

```rust
/// Copies "other files" into the plugin directory.
/// - Associated files go alongside their parent artifact's area
/// - Unassociated files go to the plugin root
pub fn emit_other_files(
    other_files: &[OtherFile],
    plugin_dir: &Path,
    fs: &dyn Fs,
) -> Result<(), Error> {
    for file in other_files {
        if file.is_external {
            // External files are NOT copied — only paths are rewritten (§5.8)
            continue;
        }

        let dest = if file.associated_artifact.is_some() {
            // Place in the same relative structure under plugin root
            plugin_dir.join(&file.relative_path)
        } else {
            // Place directly in plugin root, preserving filename only
            plugin_dir.join(
                file.relative_path.file_name().unwrap_or_default()
            )
        };

        if let Some(parent) = dest.parent() {
            fs.create_dir_all(parent)?;
        }
        let content = fs.read_to_string(&file.path)?;
        fs.write(&dest, &content)?;
    }

    Ok(())
}
```

Call `emit_other_files()` from `emit_plugin()`, `emit_plugin_with_name()`, and `emit_package_plugin()` after the existing artifact emission and script copying.

### 5.8 Path Rewriting for External References

**File:** `crates/libaipm/src/migrate/emitter.rs`

When a migrated artifact references a file outside `.claude/`, the reference path must be rewritten so it remains valid from the artifact's new location inside `.ai/<plugin>/`.

```rust
/// Rewrites references to external files in artifact content.
/// Computes the relative path from the artifact's new location back to the
/// external file's original location.
fn rewrite_external_references(
    content: &str,
    artifact_new_dir: &Path,
    external_refs: &[(PathBuf, PathBuf)],  // (old_ref_text, absolute_path)
    project_root: &Path,
) -> String {
    let mut result = content.to_string();
    for (old_ref, abs_path) in external_refs {
        // Compute relative path from artifact_new_dir to abs_path
        if let Some(new_relative) = pathdiff::diff_paths(abs_path, artifact_new_dir) {
            let old_str = old_ref.to_string_lossy();
            let new_str = new_relative.to_string_lossy();
            result = result.replace(old_str.as_ref(), new_str.as_ref());
        }
    }
    result
}
```

This rewriting applies per-artifact-type following the existing dual strategy:
- **Skills:** Rewrite relative paths in SKILL.md content
- **Hooks:** Rewrite relative paths in command strings (extending existing `rewrite_hook_command_paths()`)
- **Agents:** Rewrite relative paths in agent `.md` content (new, mirrors skill behavior)

### 5.9 Live Migration Logging

**File:** `crates/libaipm/src/migrate/mod.rs`

In live (non-dry-run) mode, "other files" should produce log output. Add a new `Action` variant:

```rust
pub enum Action {
    // ...existing variants...
    OtherFileMigrated {
        path: PathBuf,
        destination: PathBuf,
        associated_artifact: Option<String>,
    },
    ExternalReferenceRewritten {
        path: PathBuf,
        referenced_by: String,
    },
}
```

These actions are collected in `Outcome` and printed by `cmd_migrate()` in `main.rs`.

### 5.10 Cleanup Integration

**File:** `crates/libaipm/src/migrate/cleanup.rs`

When `--destructive` is used, "other files" that were migrated (non-external) must be included in the cleanup set. The existing `remove_migrated_sources()` function operates on `Outcome::migrated_sources()` which returns `Action::PluginCreated` source paths. The new `OtherFileMigrated` action paths must also be included.

Update `migrated_sources()` on `Outcome` (or add a new method `migrated_other_files()`) to return the other file source paths for cleanup.

## 6. Alternatives Considered

| Option | Pros | Cons | Decision |
|--------|------|------|----------|
| New `OtherFilesDetector` that runs last and claims unclaimed files | Keeps detection in the detector layer; consistent with existing pattern | Requires passing claimed-file set into `detect()`, changing the `Detector` trait signature; every detector would need updating | **Rejected** — changing the trait is invasive and the orchestrator approach is simpler |
| Content-based heuristic search (scan all artifact content for any filename match) | Broadest coverage for dependency association | High false-positive rate (e.g., a file named `test.py` matching the word "test" in content); unpredictable behavior | **Rejected** — too noisy; expanded regex patterns in `extract_script_references()` give sufficient coverage with predictable results |
| Move external files into the plugin directory | Complete self-contained plugins | Files outside `.claude/` may be shared across multiple tools; moving them could break other systems; user explicitly chose "warn and rewrite" | **Rejected** — per user decision; warn and rewrite paths instead |
| Unified path rewriting strategy (all absolute or all relative) | Consistency across artifact types | Changing existing behavior; absolute paths break plugin portability; relative paths are complex for hooks that resolve against project root | **Rejected** — per user decision; keep dual approach |

## 7. Cross-Cutting Concerns

### 7.1 Safety

- **Path traversal:** The reconciler must canonicalize paths and reject any file that resolves outside the source directory boundary (symlink escape). Use `fs.canonicalize()` and verify the result starts with the source directory.
- **File name collisions:** When moving unassociated files to the plugin root, check for name collisions with existing plugin files. If a collision occurs, suffix the filename (e.g., `README-1.md`).
- **Large directories:** The file enumeration could be expensive in deeply nested source directories. Respect the existing `max_depth` option if applicable.

### 7.2 Testing Strategy

Three test layers:
1. **Unit tests** using `MockFs` for the reconciler module, expanded `extract_script_references()`, and external reference rewriting
2. **E2E tests** in `crates/aipm/tests/migrate_e2e.rs` covering the full pipeline with other files
3. **BDD scenarios** in `tests/features/manifest/migrate.feature` for user-facing behavior

### 7.3 Coverage

All new code must hit the 89% branch coverage gate. The reconciler module is pure logic with no TTY interaction, so full branch coverage is achievable with `MockFs`.

## 8. Migration, Rollout, and Testing

### 8.1 Implementation Order

**Step 1: Expand `extract_script_references()`**

**File:** `crates/libaipm/src/migrate/skill_common.rs`

Add Pattern 2 (relative paths) and Pattern 3 (bare interpreter invocations) to the existing function. The function signature does not change. Add unit tests for each new pattern.

**Step 2: Add script extraction to `AgentDetector`**

**Files:** `crates/libaipm/src/migrate/agent_detector.rs`, `crates/libaipm/src/migrate/copilot_agent_detector.rs`

Replace `referenced_scripts: Vec::new()` with `extract_script_references(&content, "${CLAUDE_AGENT_DIR}")`. Add unit tests verifying agents now detect script references.

**Step 3: Add `OtherFile` type and `reconciler` module**

**Files:** `crates/libaipm/src/migrate/mod.rs` (add `OtherFile` struct, add `pub mod reconciler`), `crates/libaipm/src/migrate/reconciler.rs` (new file)

Implement `reconcile()`, `collect_all_files()`, `build_claimed_set()`, and `associate_with_artifacts()`. Add comprehensive unit tests with `MockFs` covering:
- Source dir with only classified files → empty other files
- Source dir with unclassified files → correct other files list
- Unclassified file referenced by an artifact → correctly associated
- Unclassified file not referenced → unassociated
- External file references → `is_external: true`

**Step 4: Integrate reconciler into orchestrator**

**File:** `crates/libaipm/src/migrate/mod.rs`

Add `other_files` field to `PluginPlan`. Call `reconciler::reconcile()` in both `migrate_single_source()` and `migrate_recursive()`. Thread `other_files` through to dry-run and emission paths.

**Step 5: Add dry-run report section**

**File:** `crates/libaipm/src/migrate/dry_run.rs`

Implement `write_other_files_section()`. Update `generate_report()` and `generate_recursive_report()` to accept and render other files. Add the ⚠️ markers. Update summary table with other files count.

**Step 6: Add emission of other files and path rewriting**

**File:** `crates/libaipm/src/migrate/emitter.rs`

Implement `emit_other_files()` and `rewrite_external_references()`. Integrate into `emit_plugin()`, `emit_plugin_with_name()`, and `emit_package_plugin()`. Add `OtherFileMigrated` and `ExternalReferenceRewritten` action variants.

**Step 7: Update cleanup for other files**

**File:** `crates/libaipm/src/migrate/cleanup.rs`, `crates/libaipm/src/migrate/mod.rs`

Ensure `migrated_sources()` or a new helper includes `OtherFileMigrated` paths in the cleanup set.

**Step 8: Add `cmd_migrate` output for new actions**

**File:** `crates/aipm/src/main.rs`

Handle `OtherFileMigrated` and `ExternalReferenceRewritten` actions in the CLI output. Print warnings for external references.

**Step 9: E2E tests**

**File:** `crates/aipm/tests/migrate_e2e.rs`

Add tests:
- Migrate with other files present → files appear in plugin directory
- Migrate with dependency script referenced by skill → script migrated with skill
- Migrate with external reference → warning printed, path rewritten
- `--dry-run` with other files → report includes ⚠️ section

**Step 10: BDD scenarios**

**File:** `tests/features/manifest/migrate.feature`

Add scenarios for the user-facing behavior (see §8.2).

**Step 11: Cargo gates and coverage**

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
cargo +nightly llvm-cov clean --workspace
cargo +nightly llvm-cov --no-report --workspace --branch
cargo +nightly llvm-cov --no-report --doc
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)'
```

Verify ≥ 89% branch coverage.

### 8.2 Test Plan

#### Unit Tests — Reconciler

| Test | Description |
|------|-------------|
| `reconcile_empty_dir` | Empty source dir returns empty other files |
| `reconcile_all_claimed` | All files claimed by detectors → no other files |
| `reconcile_unclaimed_files` | Files not matching any detector → returned as other files |
| `reconcile_associated_dependency` | Unclaimed file referenced in skill content → `associated_artifact` set |
| `reconcile_unassociated` | Unclaimed file not referenced → `associated_artifact` is `None` |
| `reconcile_external_reference` | Reference to file outside source dir → `is_external: true` |
| `reconcile_settings_json_claimed` | `settings.json` claimed by McpDetector/HookDetector → not in other files |

#### Unit Tests — Expanded `extract_script_references()`

| Test | Description |
|------|-------------|
| `extract_variable_prefix` | Existing `${CLAUDE_SKILL_DIR}/scripts/helper.sh` pattern still works |
| `extract_relative_dot_slash` | `./scripts/helper.sh` matched |
| `extract_relative_dot_dot` | `../utils/lib.sh` matched |
| `extract_bare_invocation` | `bash scripts/deploy.sh` matched |
| `extract_ignores_urls` | `https://example.com/scripts/foo.sh` NOT matched |
| `extract_deduplicates` | Same path referenced twice → one entry |

#### Unit Tests — Agent Script Extraction

| Test | Description |
|------|-------------|
| `agent_detects_script_refs` | Agent `.md` with `${CLAUDE_AGENT_DIR}/scripts/tool.sh` → populated `referenced_scripts` |
| `agent_no_scripts` | Agent `.md` with no script references → empty `referenced_scripts` |

#### Unit Tests — Dry Run Other Files Section

| Test | Description |
|------|-------------|
| `report_no_other_files` | No ⚠️ section when all files classified |
| `report_with_associated_files` | Dependencies section lists associated files |
| `report_with_unassociated_files` | ⚠️ Unassociated section with warning markers |
| `report_with_external_refs` | ⚠️ External section with rewrite notice |

#### Unit Tests — Emitter

| Test | Description |
|------|-------------|
| `emit_associated_other_file` | Associated file placed in relative path under plugin dir |
| `emit_unassociated_other_file` | Unassociated file placed in plugin root |
| `emit_skips_external` | External files not copied |
| `rewrite_external_skill_ref` | Skill content path rewritten to valid relative path |
| `rewrite_external_agent_ref` | Agent content path rewritten to valid relative path |
| `rewrite_external_hook_ref` | Hook command path rewritten to valid path |

#### BDD Scenarios

```gherkin
@p0 @manifest @migrate
Feature: Migrate other files
  As a developer using aipm migrate
  I want all files in my .claude directory to be migrated
  So that no scripts or utilities are silently lost

  Rule: Other files are detected and reported

    Scenario: Dry run reports unclassified files with warnings
      Given a project with a ".claude" directory
      And the ".claude" directory contains a skill "my-skill" with a SKILL.md
      And the ".claude" directory contains a file "utils/helper.sh"
      And the ".claude" directory contains a file "README.md"
      When I run "aipm migrate --dry-run"
      Then the dry-run report contains a section "⚠️ Other Files (Unclassified)"
      And the report lists "utils/helper.sh" as an unassociated file
      And the report lists "README.md" as an unassociated file

    Scenario: Dry run reports dependency files associated with their parent artifact
      Given a project with a ".claude" directory
      And the ".claude" directory contains a skill "deploy" with content referencing "./scripts/deploy.sh"
      And the ".claude" directory contains a file "scripts/deploy.sh"
      When I run "aipm migrate --dry-run"
      Then the dry-run report lists "scripts/deploy.sh" as a dependency of "deploy"

  Rule: Other files are migrated in live mode

    Scenario: Unassociated files are moved to the plugin root
      Given a project with a ".claude" directory
      And the ".claude" directory contains a skill "my-skill" with a SKILL.md
      And the ".claude" directory contains a file "README.md"
      When I run "aipm migrate"
      Then the plugin directory ".ai/my-skill/" contains "README.md" at the root

    Scenario: Dependency files are migrated alongside their parent artifact
      Given a project with a ".claude" directory
      And the ".claude" directory contains a skill "deploy" with content referencing "./scripts/deploy.sh"
      And the ".claude" directory contains a file "scripts/deploy.sh"
      When I run "aipm migrate"
      Then the plugin directory ".ai/deploy/" contains "scripts/deploy.sh"

  Rule: External references are warned but not moved

    Scenario: External file reference produces warning and path rewrite
      Given a project with a ".claude" directory
      And the ".claude" directory contains a skill "build" with content referencing "../../scripts/build.sh"
      And a file exists at "scripts/build.sh" relative to the project root
      When I run "aipm migrate"
      Then "scripts/build.sh" is NOT copied into the plugin directory
      And the migrated SKILL.md content references a valid relative path to "scripts/build.sh"

  Rule: Agent script references are now extracted

    Scenario: Agent with script reference migrates the script
      Given a project with a ".claude" directory
      And the ".claude" directory contains an agent "reviewer" referencing "./scripts/lint.sh"
      And the ".claude" directory contains a file "scripts/lint.sh"
      When I run "aipm migrate"
      Then the plugin directory ".ai/reviewer/" contains "scripts/lint.sh"
```

## 9. Open Questions / Unresolved Issues

All open questions from the research phase have been resolved via user input:

- [x] **"Other files" detector scope:** Orchestrator diff approach — enumerate all files after detection, compute set difference. No new Detector impl.
- [x] **Dependency association heuristics:** Expand `extract_script_references()` to match relative paths (`./`, `../`) and bare interpreter invocations in addition to the existing `${CLAUDE_SKILL_DIR}/scripts/` pattern.
- [x] **Agent script references:** Yes — add `extract_script_references()` to `AgentDetector` and `CopilotAgentDetector` for parity with skills.
- [x] **Path rewriting completeness:** Keep the existing dual approach (relative for skills, absolute for hooks). Extend each strategy to cover "other files" and external references.
- [x] **Non-`.claude/` "other files":** Warn and report in dry-run, do NOT move. DO rewrite the path in the migrated artifact content so the reference remains valid from the new plugin location.
