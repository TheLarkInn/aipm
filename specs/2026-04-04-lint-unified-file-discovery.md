# `aipm lint` Unified File Discovery — Technical Design Document

| Document Metadata      | Details                          |
| ---------------------- | -------------------------------- |
| Author(s)              | Sean Larkin                      |
| Status                 | Draft (WIP)                      |
| Team / Owner           | AIPM Core                        |
| Created / Last Updated | 2026-04-04 / 2026-04-04         |

## 1. Executive Summary

This spec replaces the current two-phase lint discovery model (directory-name-scoped recursive walk + flat `.ai/` check) with a **single unified recursive file walk** that discovers all AI plugin features (skills, agents, hooks, etc.) anywhere in the working directory tree. All applicable lint rules run on every discovered feature regardless of its parent directory. The `source/misplaced-features` rule's `.ai/` existence gate is removed so it fires unconditionally. Trace-level diagnostics are added throughout the walk and detection pipeline for debugging.

**Research basis:**
- [research/tickets/2026-04-04-208-lint-recursive-discovery-github.md](../research/tickets/2026-04-04-208-lint-recursive-discovery-github.md) — Issue #208 research
- [specs/2026-04-02-lint-recursive-discovery.md](2026-04-02-lint-recursive-discovery.md) — Previous recursive discovery spec (directory-scoped)
- [research/docs/2026-04-02-aipm-lint-configuration-research.md](../research/docs/2026-04-02-aipm-lint-configuration-research.md) — Lint configuration research

---

## 2. Context and Motivation

### 2.1 Current State

The lint pipeline in [`lint/mod.rs:89-130`](../crates/libaipm/src/lint/mod.rs#L89-L130) uses a two-phase architecture:

**Phase 1 — Recursive directory discovery** (lines 93-117): Walks the project tree using `ignore::WalkBuilder` looking for directories named `.claude` or `.github`. Each discovered directory gets only the `MisplacedFeatures` rule run against it.

**Phase 2 — Flat marketplace check** (lines 119-130): Checks if `<root>/.ai/` exists. If so, runs 11 marketplace quality rules (`skill/*`, `agent/*`, `hook/*`, `plugin/*`) against it.

Rule dispatch at [`rules/mod.rs:66-73`](../crates/libaipm/src/lint/rules/mod.rs#L66-L73):
- `.claude` / `.github` → only `MisplacedFeatures`
- `.ai` → 11 marketplace rules
- anything else → empty

### 2.2 The Problem

**Issue [#208](https://github.com/TheLarkInn/aipm/issues/208):** Given this directory structure:

```
.github/
  skills/
  agents/
.vscode/
some_other_folder/
```

Running `aipm lint` produces zero diagnostics because:

1. **Discovery finds `.github/`** — the recursive walker matches the directory name correctly.
2. **`MisplacedFeatures.check()` short-circuits** at [`misplaced_features.rs:49-52`](../crates/libaipm/src/lint/rules/misplaced_features.rs#L49-L52) — it checks `fs.exists(project_root.join(".ai"))`, and since `.ai/` doesn't exist, returns `Ok(vec![])`.
3. **No marketplace rules run** — they're exclusively bound to `.ai/` sources.

Result: the user gets "no issues found" despite having detectable plugin features in `.github/`.

**Secondary problems:**
- Marketplace quality rules (e.g., `skill/missing-name`) never run on features outside `.ai/`, so users get no validation feedback until after migration.
- `scan_skills()` is called 8 separate times by different rules — no shared scan pass.
- No trace-level logging exists in the discovery or detection pipeline, making debugging difficult.

---

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [ ] `aipm lint` performs a **single unified recursive walk** of the entire cwd (gitignore-aware), discovering all AI plugin features regardless of parent directory
- [ ] The `source/misplaced-features` rule fires **without** requiring `.ai/` to exist; help text suggests `aipm init` when `.ai/` is absent
- [ ] All applicable quality rules (`skill/*`, `agent/*`, `hook/*`, `plugin/*`) run on features found **anywhere** in the tree, not just inside `.ai/`
- [ ] The `--source` filter and `--max-depth` flag continue to work
- [ ] Trace-level diagnostics (`tracing::trace!`) are emitted throughout the walk and detection pipeline for every directory entered, feature detected, and rule dispatched
- [ ] Existing lint tests continue to pass (behavior-compatible for all currently-tested scenarios)
- [ ] Branch coverage remains ≥ 89%

### 3.2 Non-Goals (Out of Scope)

- [ ] Shared/cached scan pass across marketplace rules (optimization — follow-up issue)
- [ ] New lint rules beyond what currently exists
- [ ] Changes to the reporter/output format
- [ ] Changes to the config system (`aipm.toml` `[workspace.lints]`)
- [ ] Verbosity/subscriber wiring (covered by [specs/2026-04-03-verbosity-levels.md](2026-04-03-verbosity-levels.md))

---

## 4. Proposed Solution (High-Level Design)

### 4.1 Architecture: Before vs. After

**Before (current two-phase model):**

```
aipm lint
  ├─ Phase 1: discover_source_dirs([".claude", ".github"])
  │    └─ For each found dir → run MisplacedFeatures only
  └─ Phase 2: if .ai/ exists → run 11 marketplace rules
```

**After (unified walk model):**

```
aipm lint
  └─ Single walk: discover_all_features(cwd)
       ├─ For each found feature:
       │    ├─ Classify: is it inside .ai/? .claude/? .github/? other?
       │    ├─ Run applicable quality rules (skill/*, agent/*, hook/*)
       │    └─ If NOT inside .ai/ → also run source/misplaced-features
       └─ For .ai/ plugins specifically:
            └─ Run plugin/broken-paths (needs manifest-level context)
```

### 4.2 Architectural Pattern

**File-based discovery with path-based classification.** Instead of discovering directories and dispatching rules by source type, we discover individual feature files and classify them by inspecting their path. Rules receive individual files (or small file groups) rather than entire source directories.

### 4.3 Key Components

| Component | Responsibility | Current Location | Change |
|-----------|---------------|-----------------|--------|
| Unified Walker | Recursively walk cwd, find feature files | `discovery.rs` | New function `discover_features()` |
| Feature Classifier | Determine context from file path | N/A (new) | New: inspects path for `.ai/`, `.claude/`, `.github/`, or other |
| Lint Engine | Orchestrate walk → classify → dispatch rules | `lint/mod.rs` | Replace two-phase with single-pass |
| `MisplacedFeatures` | Warn about features outside `.ai/` | `lint/rules/misplaced_features.rs` | Remove `.ai/` gate, update help text |
| Marketplace Rules | Validate feature quality | `lint/rules/skill_*.rs`, etc. | Change `check()` to accept individual feature files instead of scanning a marketplace dir |
| Scan Utilities | Find features inside a directory | `lint/rules/scan.rs` | Superseded by unified walker for discovery; retained for file parsing |

---

## 5. Detailed Design

### 5.1 New Discovery: `discover_features()`

**Location:** `crates/libaipm/src/discovery.rs`

A new public function alongside the existing `discover_source_dirs()`:

```rust
/// A discovered feature file and its context.
#[derive(Debug, Clone)]
pub struct DiscoveredFeature {
    /// Absolute path to the feature file (e.g., `.github/skills/default/SKILL.md`).
    pub file_path: PathBuf,
    /// The feature kind: Skill, Agent, Hook, Plugin (for aipm.toml manifests).
    pub kind: FeatureKind,
    /// Path context: which source directory this feature lives under, if any.
    /// `None` for features found outside any recognized source directory.
    pub source_context: Option<SourceContext>,
    /// Relative path from project root.
    pub relative_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeatureKind {
    Skill,
    Agent,
    Hook,
    Plugin,  // aipm.toml manifest
}

#[derive(Debug, Clone)]
pub struct SourceContext {
    /// The recognized source directory (e.g., ".ai", ".claude", ".github").
    pub source_type: String,
    /// The plugin name derived from directory structure (for .ai/) or None.
    pub plugin_name: Option<String>,
}
```

**Walk behavior:**
- Uses `ignore::WalkBuilder` with `hidden(false)`, `git_ignore(true)`, `git_global(true)`, `git_exclude(true)` — same as current.
- **No `.ai/` exclusion filter** — `.ai/` is walked like everything else.
- **Early directory filtering**: uses `filter_entry` to skip directories that clearly cannot contain features:
  - Skip `node_modules/`, `target/`, `.git/`, `vendor/`, `__pycache__/`, `dist/`, `build/`
  - Only descend into directories with known feature parent names (`skills/`, `agents/`, `hooks/`, `commands/`, `output-styles/`, `extensions/`) or their ancestors
- **Feature file detection** is based on file name patterns:
  - `SKILL.md` → `FeatureKind::Skill`
  - `*.md` inside an `agents/` parent directory → `FeatureKind::Agent`
  - `hooks.json` inside a `hooks/` parent directory → `FeatureKind::Hook`
  - `aipm.toml` inside `.ai/<plugin>/` → `FeatureKind::Plugin`
- **Path-based classification**: after detecting a feature, inspect ancestor path components to derive `SourceContext`:
  - If path contains `/.ai/` → `source_type: ".ai"`, extract plugin name from the `.ai/<plugin>/` segment
  - If path contains `/.claude/` → `source_type: ".claude"`
  - If path contains `/.github/` → `source_type: ".github"`
  - Otherwise → `source_context: None`
- **Trace logging**: emit `tracing::trace!` for:
  - Every directory entered: `trace!(dir = %path.display(), "entering directory")`
  - Every directory skipped by filter: `trace!(dir = %path.display(), reason = %reason, "skipping directory")`
  - Every feature file detected: `trace!(file = %path.display(), kind = ?kind, source = ?context, "feature detected")`
- **`max_depth`** applied via `builder.max_depth()` as before.
- **`--source` filtering**: applied post-walk — filter `DiscoveredFeature` results by `source_context.source_type` if `--source` is specified. This means the walker always walks everything, and filtering is a simple `Vec::retain()` pass.
- Results sorted by `file_path` for deterministic output.

### 5.2 Updated Lint Engine: `lint()`

**Location:** `crates/libaipm/src/lint/mod.rs`

Replace the current two-phase pipeline with:

```rust
pub fn lint(opts: &Options, fs: &dyn Fs) -> Result<Outcome, Error> {
    let mut all_diagnostics = Vec::new();
    let mut sources_scanned = Vec::new();

    // Single-pass: discover all features in the project tree
    let features = crate::discovery::discover_features(
        &opts.dir,
        opts.max_depth,
    )?;

    // Apply --source filter if provided
    let features: Vec<_> = if let Some(ref source_filter) = opts.source {
        features.into_iter().filter(|f| {
            f.source_context.as_ref()
                .is_some_and(|ctx| ctx.source_type == *source_filter)
        }).collect()
    } else {
        features
    };

    // Track which source types were scanned
    for f in &features {
        let src = f.source_context.as_ref()
            .map(|ctx| ctx.source_type.as_str())
            .unwrap_or("other");
        if !sources_scanned.contains(&src.to_string()) {
            sources_scanned.push(src.to_string());
        }
    }

    // Check if .ai/ marketplace exists (for misplaced-features messaging)
    let ai_exists = fs.exists(&opts.dir.join(".ai"));

    // Run rules per discovered feature
    for feature in &features {
        run_rules_for_feature(
            feature,
            &opts.dir,
            ai_exists,
            fs,
            &opts.config,
            &mut all_diagnostics,
        )?;
    }

    // Sort and return
    all_diagnostics.sort_by(|a, b| {
        a.file_path.cmp(&b.file_path)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.col.cmp(&b.col))
    });

    let error_count = all_diagnostics.iter().filter(|d| d.severity == Severity::Error).count();
    let warning_count = all_diagnostics.iter().filter(|d| d.severity == Severity::Warning).count();

    Ok(Outcome {
        diagnostics: all_diagnostics,
        error_count,
        warning_count,
        sources_scanned,
    })
}
```

### 5.3 New Rule Dispatch: `run_rules_for_feature()`

**Location:** `crates/libaipm/src/lint/mod.rs`

Replace `run_rules_for_source()` with a feature-level dispatch:

```rust
fn run_rules_for_feature(
    feature: &DiscoveredFeature,
    project_root: &Path,
    ai_exists: bool,
    fs: &dyn Fs,
    config: &config::Config,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Error> {
    tracing::trace!(
        feature = %feature.file_path.display(),
        kind = ?feature.kind,
        source = ?feature.source_context,
        "dispatching rules for feature"
    );

    let is_inside_ai = feature.source_context.as_ref()
        .is_some_and(|ctx| ctx.source_type == ".ai");

    // 1. Quality rules — run on ALL features regardless of location
    let quality_rules = rules::quality_rules_for_kind(&feature.kind);
    for rule in &quality_rules {
        if config.is_suppressed(rule.id()) { continue; }
        let rule_diagnostics = rule.check_file(&feature.file_path, fs)?;
        // ... apply severity overrides, ignore paths, help text (same as current)
    }

    // 2. Misplaced-features — run on features NOT inside .ai/
    if !is_inside_ai {
        let rule = rules::misplaced_features_rule(feature, ai_exists);
        if !config.is_suppressed(rule.id()) {
            let rule_diagnostics = rule.check_file(&feature.file_path, fs)?;
            // ... apply severity overrides, ignore paths, help text
        }
    }

    // 3. Plugin-level rules (broken-paths) — only for .ai/ manifests
    if is_inside_ai && feature.kind == FeatureKind::Plugin {
        // run plugin/broken-paths
    }

    Ok(())
}
```

### 5.4 Updated Rule Trait

**Location:** `crates/libaipm/src/lint/rule.rs`

Add a new method to the `Rule` trait for file-level checking:

```rust
pub trait Rule: Send + Sync {
    // ... existing methods unchanged ...

    /// Check a single feature file. Default delegates to the directory-based `check()`.
    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        // Default: derive the parent directory and call check()
        // This allows existing rules to work without modification initially
        if let Some(parent) = file_path.parent() {
            self.check(parent, fs)
        } else {
            Ok(vec![])
        }
    }
}
```

Marketplace rules that currently use `scan_skills()`, `scan_agents()`, `scan_hook_files()` internally will be refactored to accept individual files via `check_file()` instead of scanning entire directory trees. The scan utilities in `scan.rs` are retained for **file parsing** (`parse_skill()`, `parse_agent()`, etc.) but no longer for discovery.

### 5.5 Updated `MisplacedFeatures` Rule

**Location:** `crates/libaipm/src/lint/rules/misplaced_features.rs`

Key changes:

1. **Remove the `.ai/` existence gate** at lines 49-52.
2. **Add `ai_exists: bool` field** to the struct for help text branching.
3. **Update help text**: when `.ai/` exists, keep `"run \"aipm migrate\" to move into the .ai/ marketplace"`; when `.ai/` does not exist, use `"run \"aipm init\" to create a marketplace, then \"aipm migrate\" to move features"`.
4. **The rule no longer iterates feature dirs itself** — it receives individual features from the engine and produces one diagnostic per feature found outside `.ai/`.

```rust
pub(crate) struct MisplacedFeatures {
    pub ai_exists: bool,
}

impl Rule for MisplacedFeatures {
    fn id(&self) -> &'static str { "source/misplaced-features" }
    fn default_severity(&self) -> Severity { Severity::Warning }

    fn help_text(&self) -> Option<&'static str> {
        if self.ai_exists {
            Some("run \"aipm migrate\" to move into the .ai/ marketplace")
        } else {
            Some("run \"aipm init\" to create a marketplace, then \"aipm migrate\"")
        }
    }

    fn check_file(&self, file_path: &Path, _fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        // The engine only calls this for features NOT inside .ai/,
        // so every call produces exactly one diagnostic.
        Ok(vec![Diagnostic {
            rule_id: self.id().to_string(),
            severity: self.default_severity(),
            message: format!(
                "plugin feature found outside .ai/ marketplace: {}",
                file_path.display()
            ),
            file_path: file_path.to_path_buf(),
            // ... remaining fields
        }])
    }
}
```

### 5.6 Updated Rule Registry

**Location:** `crates/libaipm/src/lint/rules/mod.rs`

Replace the source-type dispatch with kind-based dispatch:

```rust
/// Get quality rules applicable to a feature kind.
pub(crate) fn quality_rules_for_kind(kind: &FeatureKind) -> Vec<Box<dyn Rule>> {
    match kind {
        FeatureKind::Skill => vec![
            Box::new(skill_missing_name::MissingName),
            Box::new(skill_missing_desc::MissingDescription),
            Box::new(skill_oversized::Oversized),
            Box::new(skill_name_too_long::NameTooLong),
            Box::new(skill_name_invalid::NameInvalidChars),
            Box::new(skill_desc_too_long::DescriptionTooLong),
            Box::new(skill_invalid_shell::InvalidShell),
        ],
        FeatureKind::Agent => vec![
            Box::new(agent_missing_tools::MissingTools),
        ],
        FeatureKind::Hook => vec![
            Box::new(hook_unknown_event::UnknownEvent),
            Box::new(hook_legacy_event::LegacyEventName),
        ],
        FeatureKind::Plugin => vec![
            Box::new(broken_paths::BrokenPaths),
        ],
    }
}

/// Construct a MisplacedFeatures rule instance.
pub(crate) fn misplaced_features_rule(
    feature: &DiscoveredFeature,
    ai_exists: bool,
) -> MisplacedFeatures {
    MisplacedFeatures { ai_exists }
}
```

The existing `for_source()` function is retained temporarily for backwards compatibility but marked `#[deprecated]`.

### 5.7 Trace Diagnostics

All trace-level logging uses `tracing::trace!` with structured fields. These produce no output unless a subscriber is configured (see [specs/2026-04-03-verbosity-levels.md](2026-04-03-verbosity-levels.md)).

| Location | Event | Fields |
|----------|-------|--------|
| `discover_features()` | Directory entered | `dir`, `depth` |
| `discover_features()` | Directory skipped (filter) | `dir`, `reason` |
| `discover_features()` | Feature file detected | `file`, `kind`, `source_context` |
| `discover_features()` | Walk complete | `total_features`, `total_dirs_walked` |
| `run_rules_for_feature()` | Dispatching rules | `feature`, `kind`, `source_context` |
| `run_rules_for_feature()` | Rule skipped (suppressed) | `rule_id` |
| `run_rules_for_feature()` | Rule produced diagnostics | `rule_id`, `count` |
| `run_rules_for_feature()` | Diagnostic filtered (ignore) | `rule_id`, `path`, `pattern` |

---

## 6. Alternatives Considered

| Option | Pros | Cons | Reason for Rejection |
|--------|------|------|---------------------|
| A: Remove `.ai/` gate only | Minimal change, quick fix | Doesn't address "scan all files" request; marketplace rules still `.ai/`-only | Doesn't solve the broader discovery problem. |
| B: Add more directory patterns to existing walker | Reuses `discover_source_dirs()` | Still directory-scoped; can't find features in arbitrary locations; doesn't run quality rules everywhere | Still misses features outside `.claude/`/`.github/`/`.ai/`. |
| C: Full-cwd walk, classify on the fly (selected variant with early filtering) | Solves all use cases; rules run everywhere; single-pass | More complex than current model; performance cost on large repos | **Selected.** Early filtering mitigates perf cost. The `ignore` crate is battle-tested (powers ripgrep). |
| D: Keep directory-pattern discovery, make rules accept individual files | Less discovery change | Still misses features outside known dirs; hybrid model is confusing | Doesn't address the core issue. |

---

## 7. Cross-Cutting Concerns

### 7.1 Performance

- The `ignore` crate is highly optimized (powers ripgrep) and handles large repos efficiently.
- Early `filter_entry` skips known-irrelevant directories (`node_modules/`, `target/`, `.git/`, etc.) at the walker level before allocating entries.
- Feature detection is cheap: string comparison on file names.
- The walk is single-pass — no directory is visited twice.

### 7.2 Backwards Compatibility

- **`--source` flag**: continues to work via post-walk filtering.
- **`--max-depth` flag**: continues to work via `builder.max_depth()`.
- **Config overrides**: `[workspace.lints]` in `aipm.toml` continues to work — same rule IDs, same suppression/override mechanism.
- **Exit codes**: unchanged — exit 1 if any error-severity diagnostics, exit 0 otherwise.
- **JSON output**: same `diagnostics` and `summary` schema. New `source_type` values may appear (e.g., features found outside any recognized source dir will have no `source_type`).

### 7.3 Observability

- Trace-level diagnostics (§5.7) enable debugging with `-vvv` once the verbosity spec is implemented.
- All `tracing::trace!` calls use structured fields for machine-parseable logs.

---

## 8. Migration, Rollout, and Testing

### 8.1 Implementation Order

1. Add `DiscoveredFeature`, `FeatureKind`, `SourceContext` types to `discovery.rs`
2. Implement `discover_features()` with trace logging and early directory filtering
3. Add `check_file()` default method to `Rule` trait
4. Refactor marketplace rules to implement `check_file()` (accept individual files instead of scanning directories)
5. Update `MisplacedFeatures`: remove `.ai/` gate, add `ai_exists` field, update help text
6. Update `rules/mod.rs`: add `quality_rules_for_kind()`, retain `for_source()` as deprecated
7. Replace `lint()` two-phase pipeline with single-pass feature-based pipeline
8. Update CLI handler to remove `.ai/`-specific validation (discovery handles it now)
9. Update all tests

### 8.2 Test Plan

**Unit Tests (discovery.rs):**
- `discover_features()` finds `SKILL.md` in `.ai/`, `.claude/`, `.github/`, and arbitrary directories
- `discover_features()` respects `.gitignore`
- `discover_features()` skips `node_modules/`, `target/`, `.git/`
- `discover_features()` classifies source context correctly from path
- `discover_features()` respects `max_depth`
- `discover_features()` returns deterministic (sorted) output

**Unit Tests (misplaced_features.rs):**
- Rule fires when `.ai/` does NOT exist (the bug fix)
- Rule fires when `.ai/` exists
- Help text varies based on `ai_exists`
- Rule does not fire for features inside `.ai/`

**Unit Tests (lint/mod.rs):**
- Quality rules run on features inside `.ai/`
- Quality rules run on features inside `.claude/`
- Quality rules run on features inside `.github/`
- Quality rules run on features outside any recognized source dir
- `--source .claude` filters to only `.claude/` features
- `--source .ai` filters to only `.ai/` features
- `--source .github` filters to only `.github/` features
- Config suppression works for quality rules on non-`.ai/` features
- Config severity override works for `source/misplaced-features`
- Ignore paths filter features in all locations

**E2E Tests (lint_e2e.rs):**
- `.github/skills/` without `.ai/` → produces `source/misplaced-features` warning
- `.github/agents/` without `.ai/` → produces `source/misplaced-features` warning
- `.github/skills/default/SKILL.md` with missing name → produces both `source/misplaced-features` and `skill/missing-name`
- `.claude/skills/default/SKILL.md` without `.ai/` → produces `source/misplaced-features` with "aipm init" help text
- Features in arbitrary directory (not `.ai/`/`.claude/`/`.github/`) → detected and linted
- `--source .github` with mixed features → only `.github/` features reported
- Monorepo with nested `.claude/skills/` → found and linted

### 8.3 Coverage Gate

All changes must maintain ≥ 89% branch coverage per the project's coverage requirements.

---

## 9. Open Questions / Unresolved Issues

- [ ] **Scan utility refactoring scope**: Should marketplace rules be fully refactored to accept individual `DiscoveredFeature` files in this PR, or can we use the `check_file()` default (delegate to directory-based `check()`) as a transitional step? Yes.
- [ ] **`plugin/broken-paths` rule**: This rule validates manifest path references and needs the full `.ai/<plugin>/` directory context. Should it remain directory-scoped or be adapted to work per-file? Yes. 
- [ ] **Feature detection false positives**: A `SKILL.md` file outside any `skills/` directory (e.g., `docs/SKILL.md`) would be detected as a skill. Should detection require both file name AND parent directory name matching? Yes.
- [ ] **`discover_source_dirs()` deprecation timeline**: The existing function is still used by `aipm migrate`. Should it be updated to use `discover_features()` internally, or kept separate? Yes.
