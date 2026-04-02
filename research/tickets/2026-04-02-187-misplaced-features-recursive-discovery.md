---
date: 2026-04-02 10:15:00 PDT
researcher: Claude Code
git_commit: 2b8e9ea5c822c16916fc2e42c7784f20882cec5b
branch: main
repository: aipm
topic: "[lint] misplaced-features rule needs recursive discovery model from migrate"
tags: [research, codebase, lint, migrate, discovery, misplaced-features, monorepo]
status: complete
last_updated: 2026-04-02
last_updated_by: Claude Code
---

# Research: Issue #187 -- Misplaced Features Rule Needs Recursive Discovery

## Research Question

When running `aipm lint`, the `source/misplaced-features` rule should use the same recursive search algorithm that `aipm migrate` uses (via `discover_source_dirs()` in `discovery.rs`) to find `.claude/` and `.github/` directories throughout a monorepo, rather than only checking the project root.

**GitHub Issue:** [TheLarkInn/aipm#187](https://github.com/TheLarkInn/aipm/issues/187)

## Summary

The lint pipeline currently performs **flat, root-only** source detection -- it checks whether `.claude/`, `.github/`, and `.ai/` exist at the project root via `fs.exists()` calls. The migrate pipeline, in contrast, performs **recursive, gitignore-aware** tree walking using the `ignore` crate's `WalkBuilder` to find all `.claude/` and `.github/` directories at any depth. This means `aipm lint` misses misplaced features in nested source directories (e.g., `packages/auth/.claude/skills/`), while `aipm migrate` correctly finds and processes them.

The fix requires integrating the recursive discovery model into the lint pipeline so that the `MisplacedFeatures` rule runs against every discovered source directory, not just the root one. This introduces a design tension: discovery uses real filesystem I/O (via the `ignore` crate), while lint rules use the `&dyn Fs` trait for mock-based testing.

## Detailed Findings

### 1. Current Lint Discovery Flow (`lint/mod.rs`)

The lint entry point is [`lint()`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/lint/mod.rs#L42-L112). When no `--source` filter is provided, it auto-discovers sources with three flat `fs.exists()` checks:

```rust
// lint/mod.rs:47-63
let source_types: Vec<&str> = opts.source.as_deref().map_or_else(
    || {
        let mut found = Vec::new();
        if fs.exists(&opts.dir.join(".claude")) { found.push(".claude"); }
        if fs.exists(&opts.dir.join(".github")) { found.push(".github"); }
        if fs.exists(&opts.dir.join(".ai"))     { found.push(".ai"); }
        found
    },
    |s| vec![s],
);
```

It then iterates these source types and runs rules against `opts.dir.join(source_type)`:

```rust
// lint/mod.rs:65-80
for source_type in &source_types {
    let scan_dir = opts.dir.join(source_type);
    let all_rules = rules::for_source(source_type);
    for rule in &all_rules {
        let rule_diagnostics = rule.check(&scan_dir, fs)?;
        // ... apply config overrides, collect diagnostics
    }
}
```

**Key limitation:** This only ever checks a single directory per source type at the project root. The `Options.max_depth` field ([`lint/mod.rs:124`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/lint/mod.rs#L124)) is declared but described as "reserved for future use" -- it is never passed to any discovery logic.

### 2. Current `MisplacedFeatures` Rule (`lint/rules/misplaced_features.rs`)

The [`MisplacedFeatures`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/lint/rules/misplaced_features.rs#L17-L64) rule receives a single `source_dir` (e.g., `.claude/`) and checks whether any of 6 known feature subdirectories exist:

```rust
// misplaced_features.rs:13-14
const FEATURE_DIRS: &[&str] =
    &["skills", "commands", "agents", "hooks", "output-styles", "extensions"];
```

For each, it does `fs.exists(&dir)` and emits a diagnostic if found. It also gates on `.ai/` existence -- if no marketplace exists, it produces no warnings ([`misplaced_features.rs:39-43`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/lint/rules/misplaced_features.rs#L39-L43)).

**What it misses:** Given a monorepo like:
```
project/
  .ai/                    # marketplace exists
  .claude/skills/         # root -- DETECTED
  packages/auth/.claude/skills/   # nested -- NOT DETECTED
  packages/api/.claude/hooks/     # nested -- NOT DETECTED
```

Only the root `.claude/skills/` would be flagged. The nested directories are invisible to the current lint pipeline.

### 3. Migrate's Recursive Discovery Model (`migrate/discovery.rs`)

The [`discover_source_dirs()`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/migrate/discovery.rs#L54-L120) function performs a gitignore-aware recursive walk:

```rust
// discovery.rs:59-76
let mut builder = ignore::WalkBuilder::new(project_root);
builder.hidden(false);     // find dotdirs like .claude/
builder.git_ignore(true);  // respect .gitignore
builder.git_global(true);
builder.git_exclude(true);
if let Some(depth) = max_depth {
    builder.max_depth(Some(depth));
}
builder.filter_entry(|entry| {
    // Skip .ai/ to avoid scanning marketplace plugins
    let file_name = entry.file_name().to_string_lossy();
    if entry.file_type().is_some_and(|ft| ft.is_dir()) && file_name == ".ai" {
        return false;
    }
    true
});
```

It returns [`Vec<DiscoveredSource>`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/migrate/discovery.rs#L10-L22), each containing:
- `source_dir` -- absolute path to the found directory
- `source_type` -- which pattern matched (e.g., `".claude"`)
- `package_name` -- derived from parent directory (`None` for root)
- `relative_path` -- path from project root to the parent of the source dir

Results are sorted by path for deterministic output.

### 4. How Migrate Orchestrates Discovery + Detection

[`migrate_recursive()`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/migrate/mod.rs#L444-L528) calls discovery, then parallelizes detection:

```rust
// mod.rs:455
let discovered = discovery::discover_source_dirs(dir, &[".claude", ".github"], max_depth)?;

// mod.rs:461-469
let detection_results: Vec<Result<Vec<PluginPlan>, Error>> = discovered
    .par_iter()
    .map(|src| {
        let detectors = detector::detectors_for_source(&src.source_type);
        let mut all_artifacts = Vec::new();
        for det in &detectors {
            let artifacts = det.detect(&src.source_dir, fs)?;
            all_artifacts.extend(artifacts);
        }
        // ... reconciliation and plan creation
    })
    .collect();
```

**Each `DiscoveredSource` gets its own complete set of detectors, running independently.** There is no cross-source aggregation during detection.

### 5. The Gap: `&dyn Fs` vs Real Filesystem

This is the core design tension for integrating discovery into lint.

| Aspect | Lint System | Migrate Discovery |
|--------|------------|-------------------|
| **Filesystem access** | `&dyn Fs` trait object | `ignore::WalkBuilder` (real I/O) |
| **Directory enumeration** | `fs.read_dir()` one level at a time | Recursive walk with gitignore filtering |
| **Gitignore awareness** | None | Built-in via `ignore` crate |
| **Hidden directory handling** | `fs.exists()` with explicit paths | `builder.hidden(false)` to include them |
| **Test strategy** | In-memory `MockFs` with `HashSet`/`HashMap` | Real `tempfile::tempdir()` on disk |
| **Mockability** | Fully mockable | Not mockable -- hardwired to real FS |

The `Fs` trait ([`fs.rs:25-84`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/fs.rs#L25-L84)) provides `exists`, `read_dir`, `read_to_string`, `create_dir_all`, and `write_file` as core methods. It does NOT provide a recursive walk capability.

**How migrate already bridges this gap:** In `migrate_recursive()`, discovery (real FS via `ignore`) produces a list of paths, then those paths are handed to `&dyn Fs`-based detectors. The `&dyn Fs` is threaded from the top-level `migrate()` call through to the detection loop, but it is never used by discovery itself.

### 6. Trait Comparison: `Detector` vs `Rule`

| Aspect | `Detector` ([`detector.rs:13-20`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/migrate/detector.rs#L13-L20)) | `Rule` ([`rule.rs:16-31`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/lint/rule.rs#L16-L31)) |
|--------|-----------|--------|
| Trait bounds | None (enforced by Rayon at use-site) | `Send + Sync` |
| Identification | `name() -> &'static str` | `id() -> &'static str` + `name() -> &'static str` |
| Severity | N/A | `default_severity() -> Severity` |
| Core method | `detect(&self, source_dir, fs) -> Result<Vec<Artifact>, Error>` | `check(&self, source_dir, fs) -> Result<Vec<Diagnostic>, Error>` |
| Signature shape | `(&self, &Path, &dyn Fs)` | `(&self, &Path, &dyn Fs)` |

Both traits share the same core scan signature. The `Rule` trait doc comment explicitly notes: "Mirrors the `Detector` trait pattern from the migrate pipeline." This means the `MisplacedFeatures` rule can already accept any `source_dir` path -- the limitation is purely in the calling code (`lint/mod.rs`), not in the rule itself.

### 7. Impact on the Rule Set Architecture

Currently, rules are dispatched per source type via factory functions in [`lint/rules/mod.rs`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/lint/rules/mod.rs#L26-L62):

```rust
pub fn for_claude() -> Vec<Box<dyn Rule>> {
    vec![Box::new(misplaced_features::MisplacedFeatures { source_type: ".claude" })]
}
pub fn for_copilot() -> Vec<Box<dyn Rule>> {
    vec![Box::new(misplaced_features::MisplacedFeatures { source_type: ".github" })]
}
```

The `for_source()` dispatch at line 55 maps `".claude"` -> `for_claude()`, `".github"` -> `for_copilot()`, `".ai"` -> `for_marketplace()`.

With recursive discovery, the flow would change from "check one root dir per source type" to "discover all dirs, then check each with its source-type-appropriate rules." The `MisplacedFeatures` rule itself needs no changes -- it already accepts any `source_dir` and checks for `FEATURE_DIRS` subdirectories relative to it. The `source_type` field on the struct already distinguishes `.claude` vs `.github` in diagnostic messages.

### 8. Diagnostic Enrichment Opportunity

The current `MisplacedFeatures` diagnostic message is:

```
skills/ found in .claude instead of .ai/ marketplace
```

With recursive discovery, diagnostics from nested directories should include the package context. The `DiscoveredSource` struct provides `package_name` and `relative_path` that could enrich messages:

```
skills/ found in packages/auth/.claude instead of .ai/ marketplace
```

The [`Diagnostic`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/lint/diagnostic.rs) struct already has a `file_path` field that would naturally reflect the full path.

## Code References

- [`crates/libaipm/src/lint/mod.rs:42-112`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/lint/mod.rs#L42-L112) -- Lint entry point with flat source discovery
- [`crates/libaipm/src/lint/mod.rs:47-63`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/lint/mod.rs#L47-L63) -- Flat `fs.exists()` source detection
- [`crates/libaipm/src/lint/mod.rs:124`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/lint/mod.rs#L124) -- Unused `max_depth` field
- [`crates/libaipm/src/lint/rules/misplaced_features.rs:17-64`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/lint/rules/misplaced_features.rs#L17-L64) -- `MisplacedFeatures` rule implementation
- [`crates/libaipm/src/lint/rules/misplaced_features.rs:13-14`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/lint/rules/misplaced_features.rs#L13-L14) -- `FEATURE_DIRS` constant
- [`crates/libaipm/src/lint/rules/mod.rs:26-62`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/lint/rules/mod.rs#L26-L62) -- Rule factory functions and dispatch
- [`crates/libaipm/src/lint/rule.rs:16-31`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/lint/rule.rs#L16-L31) -- `Rule` trait definition
- [`crates/libaipm/src/migrate/discovery.rs:42-120`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/migrate/discovery.rs#L42-L120) -- `discover_source_dirs()` recursive walker
- [`crates/libaipm/src/migrate/discovery.rs:10-22`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/migrate/discovery.rs#L10-L22) -- `DiscoveredSource` struct
- [`crates/libaipm/src/migrate/mod.rs:444-528`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/migrate/mod.rs#L444-L528) -- `migrate_recursive()` orchestration
- [`crates/libaipm/src/migrate/mod.rs:455`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/migrate/mod.rs#L455) -- Discovery call site in migrate
- [`crates/libaipm/src/migrate/detector.rs:13-20`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/migrate/detector.rs#L13-L20) -- `Detector` trait for comparison
- [`crates/libaipm/src/fs.rs:25-84`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/fs.rs#L25-L84) -- `Fs` trait interface
- [`crates/libaipm/src/lint/rules/test_helpers.rs:12-189`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/libaipm/src/lint/rules/test_helpers.rs#L12-L189) -- Lint `MockFs`
- [`crates/aipm/src/main.rs:498-545`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/aipm/src/main.rs#L498-L545) -- CLI `cmd_lint()` handler

## Architecture Documentation

### Current Lint Data Flow
```
cmd_lint() -> lint(opts, &Real)
  -> fs.exists(dir/.claude)  -> rules::for_claude()  -> rule.check(.claude/, fs)
  -> fs.exists(dir/.github)  -> rules::for_copilot() -> rule.check(.github/, fs)
  -> fs.exists(dir/.ai)      -> rules::for_marketplace() -> rule.check(.ai/, fs)
```

### Current Migrate Data Flow (Recursive)
```
cmd_migrate() -> migrate(opts, &Real)
  -> discover_source_dirs(dir, [".claude", ".github"], max_depth)  [real FS via ignore crate]
  -> for each DiscoveredSource:
       detectors_for_source(source_type) -> det.detect(source_dir, fs)  [&dyn Fs]
```

### Desired Lint Data Flow (After #187)
```
cmd_lint() -> lint(opts, &Real)
  -> discover_source_dirs(dir, [".claude", ".github"], max_depth)  [real FS via ignore crate]
  -> for each DiscoveredSource:
       rules::for_source(source_type) -> rule.check(source_dir, fs)  [&dyn Fs]
  -> if .ai/ exists:
       rules::for_marketplace() -> rule.check(.ai/, fs)              [&dyn Fs]
```

This mirrors exactly how migrate orchestrates discovery + detection, following the established pattern where discovery produces paths (real FS) and rules/detectors process them (mockable FS).

## Historical Context (from research/)

- [`research/tickets/2026-03-28-110-aipm-lint.md`](../tickets/2026-03-28-110-aipm-lint.md) -- Initial lint research for Issue #110. Names the "no plugin features inside `.claude` subfolders" rule and identifies recursive discovery from migrate as a pattern lint could reuse.
- [`research/docs/2026-03-31-110-aipm-lint-architecture-research.md`](../docs/2026-03-31-110-aipm-lint-architecture-research.md) -- Most comprehensive lint architecture research. Explicitly raises the question of whether lint scans source directories for misplaced features and documents the recursive discovery system from `discovery.rs`.
- [`research/docs/2026-03-23-recursive-claude-discovery-parallel-migrate.md`](../docs/2026-03-23-recursive-claude-discovery-parallel-migrate.md) -- Foundational document for recursive `.claude/` directory discovery. Documents the three changes needed: scanner using `ignore` crate, `Send + Sync` bounds, and emit sequencing.
- [`research/docs/2026-04-01-migrate-file-discovery-classification.md`](../docs/2026-04-01-migrate-file-discovery-classification.md) -- Detailed documentation of the migrate file discovery and classification system. Covers `discover_source_dirs()` and detector routing.
- [`research/docs/2026-03-24-migrate-all-artifact-types.md`](../docs/2026-03-24-migrate-all-artifact-types.md) -- Documents all artifact types and their source locations. Reference for what the misplaced-features rule needs to detect.
- [`specs/2026-03-31-aipm-lint-command.md`](../../specs/2026-03-31-aipm-lint-command.md) -- Lint spec. Line 319 describes `source/misplaced-features` as checking for plugin features in `.claude/` or `.github/` subfolders. Line 728 has an open question about behavior when no `.ai/` exists. The spec's architecture diagram (line 92) shows "Source Discovery: reuses migrate/discovery.rs" -- confirming the intent to share discovery was part of the original design.

## Open Questions

1. **Should recursive discovery apply to ALL lint rules for `.claude/` and `.github/`, or only to `misplaced-features`?** Currently `for_claude()` only returns the `MisplacedFeatures` rule, so the distinction is moot today. But if future source rules are added, they would automatically get recursive behavior.

2. **Integration test coverage:** The existing lint E2E tests ([`crates/aipm/tests/lint_e2e.rs`](https://github.com/TheLarkInn/aipm/blob/2b8e9ea5c822c16916fc2e42c7784f20882cec5b/crates/aipm/tests/lint_e2e.rs)) only test flat directory structures. New E2E tests would need monorepo-style nested `.claude/` directories.

3. **Unit test strategy for the `MisplacedFeatures` rule:** The rule itself is already testable via `MockFs` since it just checks `fs.exists()` on subdirectories. The discovery integration would be tested at the `lint()` function level or as E2E tests with real filesystem (`tempfile::tempdir()`), mirroring how `discovery.rs` tests work.

4. **Should the `lint()` function's discovery be gated on `.claude`/`.github` source types only?** The `.ai/` marketplace doesn't need recursive discovery -- it's always at the project root. Discovery should only apply to source types that could appear at nested locations.

5. **Error type bridging:** `discover_source_dirs()` returns `migrate::Error::DiscoveryFailed(String)`. The lint pipeline uses `lint::Error`. A conversion or a shared error variant would be needed.
