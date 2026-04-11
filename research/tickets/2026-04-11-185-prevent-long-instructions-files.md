---
date: 2026-04-11 14:30:00 UTC
researcher: Claude Opus 4.6
git_commit: 325ab6bdfadc11b929e975498692455e777acba6
branch: fix/vscode-extension-launch-lsp
repository: aipm
topic: "[lint] new rule — prevent long instructions files (Issue #185)"
tags: [research, lint, instructions, claude-md, agents-md, copilot-md, file-size, token-limit]
status: complete
last_updated: 2026-04-11
last_updated_by: Claude Opus 4.6
last_updated_note: "Added follow-up research on recursive subdirectory detection of instruction files (lazy-loaded per-directory pattern)"
---

# Research: Prevent Long Instructions Files — Issue #185

## Research Question

What is required to implement a new `aipm lint` rule that warns when AI instruction files (CLAUDE.md, AGENTS.md, COPILOT.md, INSTRUCTIONS.md) exceed configurable line or character limits?

## Issue Summary

**GitHub Issue**: [TheLarkInn/aipm#185](https://github.com/TheLarkInn/aipm/issues/185)
**Title**: `[lint] new rule — prevent long instructions files`
**Author**: @TheLarkInn
**Created**: 2026-04-02
**State**: Open
**Labels**: `lint`

**Problem statement**: Most AI models re-inject instruction files (CLAUDE.md, agents.md, etc.) into every turn of a conversation. Excessively long instruction files waste context window tokens on every interaction. A configurable lint rule should warn users when their instruction files are too long.

**Requested detectors**: AGENTS.md, INSTRUCTIONS.md, CLAUDE.md, COPILOT.md (for now).

**Configurable options** from the issue:
- `lines` (number), default: 100
- `characters` (number), default: ? (unspecified)
- `ignore` pattern (possibly already built into framework?)

## Summary

This rule requires a new category of lint target — **project-root instruction files** — which the current lint pipeline does not discover or classify. The existing discovery pipeline (`discover_features()`) only finds feature files inside `.ai/`, `.claude/`, and `.github/` directory structures (skills, agents, hooks, plugins). Root-level `.md` files like `CLAUDE.md` and `AGENTS.md` are completely invisible to the current lint system.

Implementation requires:
1. A new file discovery mechanism (or extension of the existing one) to find instruction files at project root and known subdirectories
2. A new `FeatureKind` variant (e.g., `Instructions`) or a standalone rule that bypasses the feature-kind dispatch
3. A new lint rule struct with configurable thresholds (unlike all existing rules which use hardcoded constants)
4. Extension of the `aipm.toml` config parsing to support per-rule options beyond severity/ignore

## Detailed Findings

### 1. Instruction File Locations (Known Conventions)

Based on research from the Copilot CLI source analysis ([`research/docs/2026-03-28-copilot-cli-source-code-analysis.md`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/research/docs/2026-03-28-copilot-cli-source-code-analysis.md)) and Claude Code defaults ([`research/docs/2026-03-16-claude-code-defaults.md`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/research/docs/2026-03-16-claude-code-defaults.md)):

| Tool | Convention Dir | Filename | Scope |
|------|---------------|----------|-------|
| Claude Code | `.` (project root) | `CLAUDE.md` | Project instructions |
| Claude Code | `.claude/` | `CLAUDE.md` | Project instructions (alternative) |
| Claude Code | `~/.claude/` | `CLAUDE.md` | Personal/global instructions |
| Copilot CLI | `.github/` | `copilot-instructions.md` | Project instructions |
| Copilot CLI | `.github/instructions/` | `*.instructions.md` | Scoped instructions |
| Copilot CLI | `.` (project root) | `AGENTS.md` | Agent instructions |
| Copilot CLI | `.` (project root) | `GEMINI.md` | Gemini instructions |
| General | `.` (project root) | `COPILOT.md` | Copilot instructions |
| General | `.` (project root) | `INSTRUCTIONS.md` | Generic instructions |

**Key finding**: Copilot CLI reads `CLAUDE.md`, `AGENTS.md`, and `GEMINI.md` in addition to its own `copilot-instructions.md`. This cross-reading behavior makes the issue's concern about re-injection particularly relevant.

### 2. Current Lint Discovery Pipeline — Gap Analysis

The lint pipeline discovers files via `discover_features()` at [`crates/libaipm/src/discovery.rs:280-350`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/crates/libaipm/src/discovery.rs#L280-L350).

**How discovery currently works**:
- Uses `ignore::WalkBuilder` for gitignore-aware recursive directory walking
- Each file is classified by `classify_feature_kind()` at [`discovery.rs:233-278`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/crates/libaipm/src/discovery.rs#L233-L278)
- Classification is based on filename + parent/grandparent directory naming conventions
- Files that don't match any `FeatureKind` pattern are silently skipped (`None` returned)

**What the current system classifies**:
- `SKILL.md` in `skills/` directories → `FeatureKind::Skill`
- `*.md` in `agents/` directories → `FeatureKind::Agent`
- `hooks.json` in `hooks/` directories → `FeatureKind::Hook`
- `aipm.toml` in `.ai/<plugin>/` → `FeatureKind::Plugin`
- `marketplace.json` in `.ai/.claude-plugin/` → `FeatureKind::Marketplace`
- `plugin.json` in `.ai/<plugin>/.claude-plugin/` → `FeatureKind::PluginJson`

**What is NOT classified** (and therefore invisible to lint):
- `CLAUDE.md` at project root or `.claude/CLAUDE.md`
- `AGENTS.md` at project root
- `COPILOT.md` at project root
- `INSTRUCTIONS.md` at project root
- `.github/copilot-instructions.md`
- `.github/instructions/*.instructions.md`

### 3. Existing Size-Based Rule: `skill/oversized`

The closest existing pattern is the `skill/oversized` rule at [`crates/libaipm/src/lint/rules/skill_oversized.rs`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/crates/libaipm/src/lint/rules/skill_oversized.rs):

- Uses a hardcoded `SKILL_CHAR_BUDGET = 15_000` constant (line 15)
- Checks `skill.content.len() > SKILL_CHAR_BUDGET`
- Returns a `Diagnostic` with the actual character count in the message
- Has both `check()` (directory scan) and `check_file()` (single file) implementations
- No configurable threshold — the constant is fixed at compile time

**Differences from what #185 needs**:
- `skill/oversized` operates on already-discovered `FeatureKind::Skill` files
- Issue #185 targets files that are NOT currently discovered
- Issue #185 requests configurable thresholds (lines AND characters), not hardcoded constants
- Issue #185 targets multiple filename patterns, not a single `SKILL.md` pattern

### 4. Lint Configuration System — Configurable Options Gap

The current config system at [`crates/libaipm/src/lint/config.rs`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/crates/libaipm/src/lint/config.rs) supports:

- **Severity override**: Change a rule's severity (`error`, `warn`, `allow`)
- **Per-rule ignore paths**: Glob patterns to skip specific files for a specific rule
- **Global ignore paths**: Glob patterns to skip files across all rules

**What it does NOT support**:
- **Per-rule custom options** (e.g., `lines = 100`, `characters = 5000`)

The `RuleOverride` enum has three variants:
```rust
pub enum RuleOverride {
    Allow,
    Level(Severity),
    Detailed { level: Severity, ignore: Vec<String> },
}
```

To support configurable thresholds for the new rule, one of these approaches is needed:
1. Extend `RuleOverride` with an additional variant or field for rule-specific options
2. Add a separate config section for rule-specific options (e.g., `[workspace.lints.instructions/oversized.options]`)
3. Make the rule read its own config key from the TOML (similar to how `load_lint_config()` at [`main.rs:748-836`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/crates/aipm/src/main.rs#L748-L836) already navigates the TOML tree)

### 5. Rule Registration Pattern

New rules are registered in two places in [`crates/libaipm/src/lint/rules/mod.rs`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/crates/libaipm/src/lint/rules/mod.rs):

1. **`quality_rules_for_kind()`** (line 38-64): Dispatches rules based on `FeatureKind`. A new rule needs either:
   - A new `FeatureKind` variant (e.g., `Instructions`) with its own rule set
   - Or a separate dispatch mechanism (like `misplaced_features` which runs outside the kind-based dispatch)

2. **`catalog()`** (line 72-92): Returns all rules for LSP/tooling. The new rule must be added here too.

### 6. Rule Trait Interface

The `Rule` trait at [`crates/libaipm/src/lint/rule.rs:16-50`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/crates/libaipm/src/lint/rule.rs#L16-L50):

```rust
pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn default_severity(&self) -> Severity;
    fn help_url(&self) -> Option<&'static str>;
    fn help_text(&self) -> Option<&'static str>;
    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error>;
    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error>;
}
```

All current rules are **zero-sized unit structs** (stateless). A configurable rule would need to either:
- Be a struct with fields for the thresholds (e.g., `pub struct InstructionsOversized { max_lines: usize, max_chars: usize }`)
- Or read thresholds from a static/global config (less idiomatic)

The struct-with-fields approach is compatible with the trait since `Rule` requires `Send + Sync` but not `Default` or zero-size.

### 7. File Detection Approach — Design Considerations

Since instruction files are NOT inside feature directories, the rule likely needs its own detection logic. Two approaches:

**Approach A — Extend `FeatureKind`**:
- Add `FeatureKind::Instructions` to the enum
- Extend `classify_feature_kind()` to match known instruction filenames at expected locations
- The file would flow through the normal lint pipeline
- Pro: Cleanest integration with existing pipeline
- Con: Instruction files aren't really "features" in the plugin sense

**Approach B — Standalone rule with own file scanning**:
- Similar to how `misplaced_features` is dispatched separately from quality rules
- The rule runs its own targeted file scan (check project root, `.claude/`, `.github/` for known filenames)
- Dispatch it in `lint()` alongside or after the feature loop
- Pro: No changes to discovery/FeatureKind needed
- Con: Introduces a second scanning pattern

### 8. Known Instruction File Patterns to Detect

Based on the issue and research, the initial set of detectors should cover:

| Filename Pattern | Locations to Check |
|---|---|
| `CLAUDE.md` | Project root, `.claude/CLAUDE.md` |
| `AGENTS.md` | Project root |
| `COPILOT.md` | Project root |
| `INSTRUCTIONS.md` | Project root |
| `copilot-instructions.md` | `.github/copilot-instructions.md` |
| `*.instructions.md` | `.github/instructions/` (recursive) |

The issue specifically mentions: "We should have detectors for AGENTS, INSTRUCTIONS, CLAUDE, COPILOT.md for now."

### 9. Suggested Rule ID Convention

Following the existing hierarchical naming pattern:
- `instructions/oversized` — aligns with `skill/oversized` naming
- Alternative: `instructions/too-long` — more descriptive of line-based checking

### 10. Default Thresholds — Research

The issue proposes `lines: 100` as default. For characters, relevant benchmarks:
- `skill/oversized` uses 15,000 characters (derived from Copilot CLI's `SKILL_CHAR_BUDGET`)
- A 100-line markdown file is typically 3,000–8,000 characters depending on content
- Claude's context window is ~200K tokens; a 100-line CLAUDE.md is ~500-1500 tokens
- The concern is cumulative cost across many turns, not single-turn overflow

## Code References

- `crates/libaipm/src/lint/rules/skill_oversized.rs` — Existing size-based rule (closest pattern)
- `crates/libaipm/src/lint/rules/mod.rs:38-64` — Rule dispatch table (`quality_rules_for_kind`)
- `crates/libaipm/src/lint/rules/mod.rs:72-92` — Full rule catalog
- `crates/libaipm/src/lint/rule.rs:16-50` — `Rule` trait definition
- `crates/libaipm/src/lint/config.rs:1-54` — Config/override structs
- `crates/libaipm/src/lint/mod.rs:115-162` — Lint pipeline entry point
- `crates/libaipm/src/lint/mod.rs:68-104` — Per-feature rule dispatch
- `crates/libaipm/src/discovery.rs:233-278` — Feature classification (gap: no instruction file support)
- `crates/libaipm/src/discovery.rs:280-350` — Feature discovery walk
- `crates/libaipm/src/lint/diagnostic.rs` — Diagnostic/Severity structures
- `crates/libaipm/src/lint/rules/test_helpers.rs` — MockFs test infrastructure
- `crates/aipm/src/main.rs:748-836` — TOML config loading (`load_lint_config`)

## Architecture Documentation

### Current Lint Pipeline Flow

```
aipm lint (CLI)
  → cmd_lint() [main.rs:666]
    → load_lint_config() [main.rs:748] → reads aipm.toml
    → libaipm::lint::lint() [mod.rs:115]
      → discover_features() [discovery.rs:280] → single recursive walk
      → for each feature:
        → quality_rules_for_kind(feature.kind) → dispatch rules
        → misplaced_features_rule() → if outside .ai/
      → sort diagnostics
      → return Outcome
    → reporter.report() → Text/Human/Json/CiGitHub/CiAzure
```

### Key Architectural Observations

1. **All current rules operate on discovered features** — the new rule targets files that are not currently "features"
2. **No rule currently has configurable thresholds** — all use compile-time constants
3. **The config system supports severity overrides and ignores** but not arbitrary per-rule options
4. **The `ignore` framework from the issue IS already built into the system** — `aipm.toml`'s `[workspace.lints.ignore]` and per-rule `ignore` patterns provide path-based filtering

## Historical Context (from research/)

- [`research/tickets/2026-03-28-110-aipm-lint.md`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/research/tickets/2026-03-28-110-aipm-lint.md) — Original lint research (Issue #110) referenced a BDD scenario with a "5000 token limit" for skill files; the shipped implementation uses 15,000 characters
- [`research/docs/2026-03-28-copilot-cli-source-code-analysis.md`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/research/docs/2026-03-28-copilot-cli-source-code-analysis.md) — Documents that Copilot CLI reads `CLAUDE.md`, `AGENTS.md`, `GEMINI.md` as instruction files (lines 317-321) and notes lint rules should check for these (line 434)
- [`research/docs/2026-03-16-claude-code-defaults.md`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/research/docs/2026-03-16-claude-code-defaults.md) — Documents Claude Code's CLAUDE.md locations at project root, `.claude/CLAUDE.md`, and `~/.claude/CLAUDE.md` (line 38)
- [`research/docs/2026-04-06-feature-status-audit.md`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/research/docs/2026-04-06-feature-status-audit.md) — Lists Issue #185 as an open non-roadmap issue under "New rule" category

## Related Research

- `research/docs/2026-03-31-110-aipm-lint-architecture-research.md` — Lint architecture deep dive
- `research/docs/2026-04-02-aipm-lint-configuration-research.md` — Lint config system documentation
- `specs/2026-03-31-aipm-lint-command.md` — Core lint spec
- `specs/2026-04-07-lint-marketplace-plugin-json-rules.md` — Most recent new-rule spec (template for #185's spec)

## Open Questions

1. **Character default**: The issue leaves the default character limit as "?". Should it be derived from token estimates (e.g., ~4 chars/token × 1000 tokens = 4,000 chars)? Or based on the 100-line default (~5,000-8,000 chars)?
2. **GEMINI.md**: Copilot CLI also reads `GEMINI.md` — should this be included in the initial set of detectors?
3. **Scoped instructions**: Should `.github/instructions/*.instructions.md` files be included? These are scoped (not injected every turn) but could still be oversized.
4. **Personal instruction files**: Should `~/.claude/CLAUDE.md` be checked? The lint command currently operates on project directories, not user-global paths.
5. **FeatureKind extension vs. standalone scan**: Should instruction files be integrated into the discovery pipeline as a new `FeatureKind`, or should the rule use its own file scanning approach?
6. **Config schema for custom options**: How should configurable thresholds be represented in `aipm.toml`? E.g.:
   ```toml
   [workspace.lints."instructions/oversized"]
   level = "warn"
   lines = 150
   characters = 8000
   ```
   This would require extending the TOML parsing in `load_lint_config()` and the `RuleOverride` enum.
7. **Both or either**: Should the rule trigger when EITHER the line limit OR the character limit is exceeded? Or only when both are exceeded?

---

## Follow-up Research 2026-04-11

### Recursive Subdirectory Detection of Instruction Files

Many real-world repos place instruction files in product-area subdirectories rather than (or in addition to) the project root. These files are designed to be **lazy-loaded**: the AI tool only injects them into context when work is occurring inside that subdirectory or the directory is otherwise relevant, saving top-level context budget. This pattern is in widespread use across both Claude Code and GitHub Copilot ecosystems.

#### How Lazy-Loading Works Per Tool

**Claude Code — hierarchical CLAUDE.md**

Claude Code performs a directory walk-up at session start: it reads every `CLAUDE.md` found from the current working directory up to the project root. Files deeper in the directory tree are therefore loaded only when the working directory is inside that subtree. A common pattern:

```
repo-root/
├── CLAUDE.md                    # always loaded — repo-wide instructions
├── packages/
│   ├── auth/
│   │   └── CLAUDE.md            # loaded only when working inside packages/auth/
│   ├── payments/
│   │   └── CLAUDE.md            # loaded only when working inside packages/payments/
│   └── ui/
│       └── CLAUDE.md
├── scripts/
│   └── CLAUDE.md                # loaded only when working in scripts/
└── .claude/
    └── CLAUDE.md                # equivalent to root CLAUDE.md
```

This is documented in the Claude Code defaults research (`research/docs/2026-03-16-claude-code-defaults.md`, line 155): CLAUDE.md "supports `@path/to/import` syntax" — files can also chain imports to other subdirectory files.

**GitHub Copilot — scoped `.instructions.md` files**

Copilot CLI's `.github/instructions/` directory uses a glob-matched lazy-loading model: each `*.instructions.md` file has an `applyTo` frontmatter field (a glob pattern) specifying which files should trigger its inclusion:

```
.github/
└── instructions/
    ├── frontend.instructions.md       # applyTo: "src/frontend/**"
    ├── auth-service.instructions.md   # applyTo: "services/auth/**"
    ├── testing.instructions.md        # applyTo: "**/*.test.ts"
    └── database.instructions.md       # applyTo: "src/db/**"
```

These files are scoped (not always injected) but can still accumulate to large sizes individually.

Source: `research/docs/2026-03-28-copilot-cli-source-code-analysis.md`, lines 323-326.

**AGENTS.md in subdirectories**

The OpenAI agents convention (`AGENTS.md`) follows the same hierarchical walk-up pattern as `CLAUDE.md`. Subdirectory-scoped `AGENTS.md` files are loaded by the tool when context is relevant to that subtree.

#### Implication: Repo-Wide Recursive Scan Is Required

A rule that only checks known root-level locations (`./CLAUDE.md`, `./.claude/CLAUDE.md`, `./.github/copilot-instructions.md`) would produce **false negatives** for every subdirectory-scoped instruction file — which, in monorepos, may be the majority of instruction files present.

The rule must perform a **full recursive scan of the project tree**, matching files by name regardless of depth. Pseudocode for the detection logic:

```
Walk entire project tree (gitignore-aware):
  For each file encountered:
    If filename matches any of the known instruction file patterns:
      Collect as a candidate for the rule check
```

Known instruction file patterns (by filename only — location is arbitrary):

| Filename pattern | Tool | Lazy-loaded? |
|---|---|---|
| `CLAUDE.md` | Claude Code | Yes — per directory depth |
| `AGENTS.md` | OpenAI / Copilot | Yes — per directory depth |
| `COPILOT.md` | Copilot | Yes — per directory depth |
| `INSTRUCTIONS.md` | Generic | Yes — per directory depth |
| `GEMINI.md` | Google Gemini CLI | Yes — per directory depth |
| `*.instructions.md` | GitHub Copilot | Yes — `applyTo` scoped |

#### Alignment with Existing Discovery Infrastructure

The existing `discover_features()` function at [`crates/libaipm/src/discovery.rs:280-350`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/crates/libaipm/src/discovery.rs#L280-L350) already performs a gitignore-aware recursive walk via `ignore::WalkBuilder`. The same walk that discovers `SKILL.md` files anywhere in the tree can be extended to match instruction file naming patterns.

Two viable implementation paths:

**Option A — Extend `classify_feature_kind()` with a new `FeatureKind::Instructions` variant**

Modify `classify_feature_kind()` at [`discovery.rs:233`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/crates/libaipm/src/discovery.rs#L233) to recognise instruction file names:

```rust
// Instruction files — match by filename only, any depth
const INSTRUCTION_FILENAMES: &[&str] = &[
    "CLAUDE.md", "AGENTS.md", "COPILOT.md", "INSTRUCTIONS.md", "GEMINI.md",
];

if INSTRUCTION_FILENAMES.contains(&file_name.as_ref()) {
    return Some(FeatureKind::Instructions);
}
// Also match *.instructions.md (Copilot scoped instructions)
if file_name.ends_with(".instructions.md") {
    return Some(FeatureKind::Instructions);
}
```

This integrates naturally with the existing single-pass walk. The `instructions/oversized` rule would be added to `quality_rules_for_kind()` under `FeatureKind::Instructions`, and the rule's `check_file()` receives each instruction file path directly.

**Option B — Standalone scan inside the rule's `check()` method**

The rule implements its own targeted walk (similar to how `scan::scan_skills()` iterates a directory tree). This avoids touching `discovery.rs` and `FeatureKind` but creates a second traversal of the filesystem per lint run.

Given that Option A integrates with the existing single-pass design principle and the existing skip-list (`SKIP_DIRS`) already handles `node_modules`, `target`, etc., **Option A is architecturally preferred**.

#### Handling the `source_type` Field

The `Diagnostic.source_type` field currently holds `.ai`, `.claude`, or `.github`. Instruction files found at arbitrary subdirectory depths won't always be inside one of these directories. The `source_type_from_path()` function at [`scan.rs:169-180`](https://github.com/TheLarkInn/aipm/blob/325ab6bdfadc11b929e975498692455e777acba6/crates/libaipm/src/lint/rules/scan.rs#L169-L180) already returns `"other"` for paths outside recognized source dirs. For instruction files, returning `"other"` or a new `"instructions"` label (or the first recognized ancestor dir, or `"project"`) needs a decision.

#### Updated Open Questions

8. **Depth limit**: Should the recursive scan apply a maximum depth limit to avoid flagging instruction files buried deep in `node_modules`-style subtrees that slipped past the skip list? (The existing `max_depth` option from the CLI could apply.)
9. **Filename matching — case sensitivity**: On case-insensitive filesystems (macOS, Windows) `claude.md` and `CLAUDE.md` are the same file. The detection should match case-insensitively, or document that only the uppercase convention is checked.
10. **`.instructions.md` scoped files**: Because these files are `applyTo`-scoped (not always injected), their size budget concern is different from always-injected files. Should they be a separate rule variant or have a different default threshold?
11. **`@path/to/import` chains**: Claude Code's `@import` syntax means a short root `CLAUDE.md` may pull in large imported files. A size check on the root file alone could give false confidence. Is checking the direct file content sufficient for the initial rule, with import-chain checking deferred?
