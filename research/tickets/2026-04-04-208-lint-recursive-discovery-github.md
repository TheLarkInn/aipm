---
date: 2026-04-04 12:30:00 UTC
researcher: Claude
git_commit: 1abd6773e64f6e35911d3601a761b055e4d6c86b
branch: main
repository: aipm
topic: "Issue #208: source/misplaced-features recursion doesn't work when everything is nested in a .github folder"
tags: [research, codebase, lint, discovery, misplaced-features, github, recursion]
status: complete
last_updated: 2026-04-04
last_updated_by: Claude
---

# Research: Issue #208 — Lint Recursive Discovery for `.github/` Folders

## Research Question

How does `aipm lint` currently discover and scan files, specifically the `source/misplaced-features` rule? Why do files nested in `.github/` (but not `.ai/`) fail to trigger any lint diagnostics? What changes are needed so that recursive file discovery is the default behavior for the entire lint process?

## Summary

The issue has **two root causes**:

1. **The `MisplacedFeatures` rule has a hard `.ai/` existence gate** ([`misplaced_features.rs:49-52`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/lint/rules/misplaced_features.rs#L49-L52)). If `.ai/` does not exist at the project root, the rule immediately returns an empty diagnostics vector. In the user's scenario (`.github/skills/`, `.github/agents/`, no `.ai/`), the discovery mechanism **does** find the `.github/` directory, but the rule silently produces zero diagnostics.

2. **Discovery is source-directory-scoped, not file-scoped.** The current design discovers `.claude/` and `.github/` directories via `discover_source_dirs()`, then runs only the `MisplacedFeatures` rule against those directories. Marketplace rules (skill validation, agent validation, hook validation, etc.) only run against `.ai/`. There is no mechanism to recursively scan all files in the cwd and classify them regardless of which parent directory they live in.

The user's desired behavior — "scan all files recursively, then let rules decide what's relevant" — requires decoupling file discovery from source-type-specific directory patterns.

## Detailed Findings

### 1. Current Discovery Architecture

#### Entry Point: `lint()` function

**Source:** [`crates/libaipm/src/lint/mod.rs:89-130`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/lint/mod.rs#L89-L130)

The lint pipeline has two phases:

- **Phase 1 (lines 93-117): Recursive source directory discovery.** Builds a `source_patterns` vector based on the `--source` flag. Without `--source`, patterns are `[".claude", ".github"]`. Calls `discover_source_dirs()` to recursively walk the project tree looking for directories matching those exact names. Each discovered directory gets `run_rules_for_source()` called on it.

- **Phase 2 (lines 119-130): Flat marketplace check.** If no `--source` is given, checks whether `opts.dir.join(".ai")` exists. If so, runs marketplace rules against it. This is a single root-level existence check — no recursion.

#### The Recursive Walker: `discover_source_dirs()`

**Source:** [`crates/libaipm/src/discovery.rs:61-127`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/discovery.rs#L61-L127)

Uses `ignore::WalkBuilder` for gitignore-aware traversal:

- `hidden(false)` at line 67 — allows traversal into dotfiles (`.claude/`, `.github/`)
- `git_ignore(true)` at line 68 — respects `.gitignore`
- `git_global(true)` / `git_exclude(true)` at lines 69-70 — respects global/exclude gitignore
- `max_depth` applied at lines 72-74 when provided
- **Filter at lines 77-83**: unconditionally excludes any directory named `.ai` from the walk tree, preventing the walker from descending into marketplace directories

At lines 87-121, the walker iterates all entries, skips non-directories, checks if the directory name matches any pattern (line 97), and builds `DiscoveredSource` structs.

**Key behavior**: This function only matches directories by **exact name** (e.g., `.claude`, `.github`). It does not inspect the contents of directories or match by file type. If a directory named `.github` exists anywhere in the tree (respecting gitignore), it will be discovered.

#### Rule Dispatch: `for_source()`

**Source:** [`crates/libaipm/src/lint/rules/mod.rs:66-73`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/lint/rules/mod.rs#L66-L73)

```
".claude" → [MisplacedFeatures { source_type: ".claude" }]
".github" → [MisplacedFeatures { source_type: ".github" }]
".ai"     → [11 marketplace rules: skill/*, agent/*, hook/*, plugin/*]
```

`MisplacedFeatures` is the **only** rule that runs for `.claude` and `.github` sources. All quality-validation rules (skill/missing-name, skill/oversized, hook/unknown-event, etc.) are exclusively bound to `.ai`.

### 2. The `source/misplaced-features` Rule

**Source:** [`crates/libaipm/src/lint/rules/misplaced_features.rs:45-77`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/lint/rules/misplaced_features.rs#L45-L77)

The `check()` method:

1. **Line 49-52: `.ai/` existence gate.** Constructs `self.project_root.join(".ai")` and checks `fs.exists()`. If `.ai/` does NOT exist, returns `Ok(vec![])` immediately. **This is why the user's scenario produces no diagnostics.**

2. **Lines 54-74: Feature directory iteration.** Iterates over 6 feature directory names (`skills`, `commands`, `agents`, `hooks`, `output-styles`, `extensions`). For each, joins it onto `source_dir` and checks existence. Each found directory produces a `Diagnostic` with `Severity::Warning`.

The rule does **no recursion** of its own — it only checks immediate children of the passed `source_dir`.

### 3. The User's Scenario: Why No Diagnostics

Given this directory structure:
```
.github/
  skills/
  agents/
.vscode/
some_other_folder/
```

1. Phase 1 runs `discover_source_dirs()` with patterns `[".claude", ".github"]`
2. The walker finds `.github/` at the project root → creates a `DiscoveredSource`
3. `run_rules_for_source(".github", ".github/", ...)` is called
4. `rules::for_source(".github", ...)` returns `[MisplacedFeatures { source_type: ".github" }]`
5. `MisplacedFeatures::check()` runs:
   - Checks `project_root.join(".ai")` → **does not exist**
   - Returns `Ok(vec![])` — **no diagnostics**
6. Phase 2 checks `opts.dir.join(".ai")` exists → **false** → skips marketplace rules
7. Result: 0 diagnostics, 0 errors, 0 warnings

### 4. The Desired Behavior (from Issue #208)

The user requests:

> `aipm lint` should just scan _all files recursively_ (except for gitignored) in the `cwd` and report on them. This way I can drop `aipm lint` into any repo and it should scan and detect a skill, agent, etc.

> 1 Recursive discovery operation should be default behavior for the entire lint process and then `source/misplaced-features` can simply detect if inside of `.ai` folder or not and trigger.

This implies two architectural changes:

1. **Discovery should be file-level, not directory-level.** Instead of looking for directories named `.claude`/`.github`, the lint should walk all files in the cwd (gitignore-aware) and classify what it finds. Any file that looks like a skill (`SKILL.md`), agent (`*.md` with agent frontmatter), or hook (`hooks.json`) should be detected regardless of its parent directory.

2. **`source/misplaced-features` should invert its logic.** Instead of "warn about features in `.claude`/`.github` when `.ai/` exists", it should be "report all detected features and flag those NOT inside `.ai/`". The `.ai/` existence gate should be removed or made non-blocking.

### 5. Current Test Coverage for This Scenario

#### E2E Tests

**Source:** [`crates/aipm/tests/lint_e2e.rs`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/aipm/tests/lint_e2e.rs)

- **No E2E test creates any `.github/` directory structure.** All `.github/` testing exists only in unit and integration tests.
- Three E2E tests exercise `source/misplaced-features`, all with `.claude/` + `.ai/` present:
  - `lint_monorepo_finds_nested_misplaced_features` (line 428)
  - `lint_source_claude_no_root_dir_succeeds_with_nested` (line 449)
  - `lint_max_depth_cli_flag` (line 470)

#### Unit Tests (misplaced_features.rs)

**Source:** [`crates/libaipm/src/lint/rules/misplaced_features.rs:80-170`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/lint/rules/misplaced_features.rs#L80-L170)

- `github_source_type` (line 142): Only test that creates `.github/skills/` — but with `.ai/` present
- `skills_dir_without_marketplace_no_finding` (line 117): Tests the `.ai/` gate — confirms no diagnostics when `.ai/` is absent
- **No test for `.github/` without `.ai/`** at any level

#### Integration Tests (mod.rs)

**Source:** [`crates/libaipm/src/lint/mod.rs`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/lint/mod.rs)

- `lint_discovers_nested_github_dirs` (line 527): `.ai/` + nested `packages/api/.github/hooks/` — produces diagnostics
- `lint_no_marketplace_no_source_findings` (line 827): `.claude/skills/` without `.ai/` — confirms no diagnostics (documents current behavior, which IS the bug)

#### Coverage Gap Table

| Scenario | E2E | Unit | Integration |
|----------|-----|------|-------------|
| `.github/skills/` + `.ai/` | No | Yes | Yes |
| `.github/skills/` without `.ai/` | **No** | **No** | **No** |
| `.github/agents/` (any scenario) | **No** | **No** | **No** |
| Recursive file discovery (not dir discovery) | **No** | **No** | **No** |

### 6. Key Code Locations

| Component | File | Lines | Purpose |
|-----------|------|-------|---------|
| CLI lint handler | [`crates/aipm/src/main.rs`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/aipm/src/main.rs#L521-L609) | 521-609 | CLI setup, config loading, option construction |
| Lint engine | [`crates/libaipm/src/lint/mod.rs`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/lint/mod.rs#L89-L130) | 89-130 | Two-phase pipeline: recursive discovery + flat marketplace |
| Rule dispatch | [`crates/libaipm/src/lint/rules/mod.rs`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/lint/rules/mod.rs#L66-L73) | 66-73 | Maps source types to rule sets |
| Misplaced features rule | [`crates/libaipm/src/lint/rules/misplaced_features.rs`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/lint/rules/misplaced_features.rs#L45-L77) | 45-77 | `.ai/` gate + feature dir checks |
| `.ai/` gate (root cause) | [`crates/libaipm/src/lint/rules/misplaced_features.rs`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/lint/rules/misplaced_features.rs#L49-L52) | 49-52 | `if !fs.exists(&ai_dir) { return Ok(diagnostics); }` |
| Recursive walker | [`crates/libaipm/src/discovery.rs`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/discovery.rs#L61-L127) | 61-127 | `ignore::WalkBuilder`-based directory discovery |
| `.ai/` filter in walker | [`crates/libaipm/src/discovery.rs`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/discovery.rs#L77-L83) | 77-83 | Unconditionally excludes `.ai/` from walk tree |
| Feature dir constant | [`crates/libaipm/src/lint/rules/misplaced_features.rs`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/lint/rules/misplaced_features.rs#L13-L14) | 13-14 | `["skills", "commands", "agents", "hooks", "output-styles", "extensions"]` |
| Rule trait | [`crates/libaipm/src/lint/rule.rs`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/lint/rule.rs#L16) | 16 | `Rule` trait with `check(source_dir, fs)` signature |
| Scan utilities | [`crates/libaipm/src/lint/rules/scan.rs`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/libaipm/src/lint/rules/scan.rs) | — | Marketplace-only scan (skills, agents, hooks) |
| Config loader | [`crates/aipm/src/main.rs`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/aipm/src/main.rs#L612-L680) | 612-680 | `load_lint_config()` — reads `aipm.toml` |
| Lint E2E tests | [`crates/aipm/tests/lint_e2e.rs`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/crates/aipm/tests/lint_e2e.rs) | — | 20 E2E tests, none test `.github/` scenarios |

## Architecture Documentation

### Current Lint Data Flow

```
aipm lint [DIR] [--source <SRC>] [--max-depth <N>]
  │
  ├─ Phase 1: Recursive Discovery
  │    │
  │    ├─ Build source_patterns from --source flag
  │    │   • No --source: [".claude", ".github"]
  │    │   • --source .claude: [".claude"]
  │    │   • --source .github: [".github"]
  │    │   • --source .ai: [] (skip phase 1)
  │    │
  │    ├─ discover_source_dirs(root, patterns, max_depth)
  │    │   • ignore::WalkBuilder (gitignore-aware)
  │    │   • hidden(false) — enters dotfile dirs
  │    │   • Filters out .ai/ directories
  │    │   • Matches directory names exactly against patterns
  │    │
  │    └─ For each discovered dir:
  │         run_rules_for_source(source_type, dir, root)
  │           • ".claude" → [MisplacedFeatures]
  │           • ".github" → [MisplacedFeatures]
  │           • MisplacedFeatures.check():
  │               IF .ai/ does NOT exist → return [] ← ROOT CAUSE
  │               ELSE check for skills/, agents/, etc.
  │
  └─ Phase 2: Flat Marketplace
       │
       ├─ Check if root/.ai/ exists
       │   • --source .ai: always true
       │   • --source other: always false
       │   • No --source: check fs.exists()
       │
       └─ If exists:
            run_rules_for_source(".ai", root/.ai/, root)
              • 11 marketplace rules (skill/*, agent/*, hook/*, plugin/*)
```

### Issue #208: The Discovery Ownership Problem

Currently, the `source/misplaced-features` rule does NOT own recursive discovery — the lint engine owns it. But the engine's discovery is **directory-pattern-scoped**: it only finds directories named `.claude` or `.github`. The rule then checks children of those directories.

The issue requests inverting this: make recursive file discovery the **default behavior of the entire lint process**, then let `source/misplaced-features` determine whether a detected feature is inside `.ai/` or not. This decouples discovery from source-type-specific directory patterns.

## Historical Context (from research/)

- [`research/docs/2026-03-31-110-aipm-lint-architecture-research.md`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/research/docs/2026-03-31-110-aipm-lint-architecture-research.md) — Documents the original 12-detector architecture (6 Claude + 6 Copilot) that the lint system was modeled after. Lists `discover_source_dirs()` as the shared recursive walker.
- [`research/docs/2026-04-02-aipm-lint-configuration-research.md`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/research/docs/2026-04-02-aipm-lint-configuration-research.md) — Documents lint config system. Notes that the `source/misplaced-features` rule "only fires when `.ai/` exists AND a source dir contains feature subdirectories."
- [`specs/2026-04-02-lint-recursive-discovery.md`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/specs/2026-04-02-lint-recursive-discovery.md) — Previous spec that promoted `migrate/discovery.rs` to a shared module. Documents the evolution from flat v1 to recursive discovery. Did not address the `.ai/` existence gate.
- [`specs/2026-03-31-aipm-lint-command.md`](https://github.com/TheLarkInn/aipm/blob/1abd6773e64f6e35911d3601a761b055e4d6c86b/specs/2026-03-31-aipm-lint-command.md) — Original lint command spec. Architecture diagram shows "Source Discovery: reuses migrate/discovery.rs", confirming recursive discovery was always intended.

## Related Research

- `research/tickets/2026-03-28-110-aipm-lint.md` — Original lint feature ticket
- `research/docs/2026-04-01-migrate-file-discovery-classification.md` — Migrate's file discovery and classification system (potential model for lint)

## Open Questions

1. **Should `source/misplaced-features` fire without `.ai/`?** The current `.ai/` gate was an intentional design choice — only warn about misplaced features when the user has already set up a marketplace. The issue requests removing this gate so the rule always fires. What should the message/help text say when there's no `.ai/` to migrate to?

2. **Should marketplace rules run on features found outside `.ai/`?** If a `SKILL.md` is found in `.github/skills/default/SKILL.md`, should `skill/missing-name`, `skill/oversized`, etc. run on it? Currently these rules are exclusively bound to `.ai/`.

3. **How should discovery classify files found outside known source directories?** If a `SKILL.md` is found in `some_other_folder/skills/default/SKILL.md`, what source_type should it have?

4. **What is the performance impact of full-cwd recursive file scanning?** The current approach only enters directories matching known patterns. Full-cwd scanning would touch every non-gitignored file.
