# Lint Rule: `instructions/oversized` — Prevent Long Instruction Files

| Document Metadata      | Details                                                                      |
| ---------------------- | ---------------------------------------------------------------------------- |
| Author(s)              | Sean Larkin                                                                  |
| Status                 | Draft                                                                        |
| Team / Owner           | aipm                                                                         |
| Created / Last Updated | 2026-04-11                                                                   |
| Issues                 | #185                                                                         |
| Research               | `research/tickets/2026-04-11-185-prevent-long-instructions-files.md`         |

## 1. Executive Summary

This spec adds a new lint rule `instructions/oversized` that warns when AI instruction files (`CLAUDE.md`, `AGENTS.md`, `COPILOT.md`, `INSTRUCTIONS.md`, `GEMINI.md`, `*.instructions.md`) exceed configurable line or character limits. Most AI models re-inject these instruction files into every conversation turn, so excessively long files waste context window tokens on every interaction. The rule performs a recursive, case-insensitive scan of the entire project tree via a new `FeatureKind::Instructions` variant in the discovery pipeline, introduces configurable per-rule options in `aipm.toml` (a first for the lint system), and supports an opt-in `resolve-imports` mode that follows `@path` imports and relative markdown links to report total resolved content size.

## 2. Context and Motivation

### 2.1 Current State

The `aipm lint` pipeline discovers feature files (SKILL.md, agent .md, hooks.json, aipm.toml, marketplace.json, plugin.json) via a gitignore-aware recursive walk in `crates/libaipm/src/discovery.rs`. Each file is classified by `FeatureKind` and dispatched to rules via `quality_rules_for_kind()` in `crates/libaipm/src/lint/rules/mod.rs:38-64`.

Currently, 17 lint rules exist — targeting skill frontmatter, agent frontmatter, hook JSON events, broken file paths, marketplace/plugin.json validation, and misplaced features. **None target instruction files** like `CLAUDE.md`, `AGENTS.md`, or `COPILOT.md`.

The closest precedent is `skill/oversized` (`crates/libaipm/src/lint/rules/skill_oversized.rs`), which checks that `SKILL.md` files stay under a hardcoded 15,000-character budget. However, it only operates on files already classified as `FeatureKind::Skill`.

**Key architectural gaps:**

- **Discovery gap**: Root-level instruction files (`CLAUDE.md`, `AGENTS.md`, etc.) and subdirectory instruction files are invisible to the current discovery pipeline — `classify_feature_kind()` returns `None` for them (ref: `discovery.rs:233-278`).
- **No configurable thresholds**: All 17 existing rules use hardcoded compile-time constants. The `RuleOverride` enum in `config.rs` only supports severity overrides and ignore paths — not arbitrary per-rule options.
- **No instruction file awareness**: The codebase has zero references to `CLAUDE.md`, `AGENTS.md`, `COPILOT.md`, or `INSTRUCTIONS.md` as string literals in any detection or scanning logic. (Ref: research pattern-finder agent findings.)

### 2.2 The Problem

- **User Impact:** AI tools (Claude Code, GitHub Copilot, Gemini CLI) re-inject instruction files into every turn of a conversation. Long instruction files silently consume context window tokens, reducing the effective context available for actual work. Users have no automated feedback about this cost.
- **Monorepo amplification:** Many repos place instruction files in product-area subdirectories (`packages/auth/CLAUDE.md`, `services/payments/CLAUDE.md`) for lazy-loading. A single monorepo may have dozens of instruction files, each independently contributing to context bloat when their subtree is in scope.
- **Cross-tool reading:** Copilot CLI reads `CLAUDE.md`, `AGENTS.md`, and `GEMINI.md` in addition to its own `copilot-instructions.md` (ref: `research/docs/2026-03-28-copilot-cli-source-code-analysis.md`, lines 317-321). This means instruction file bloat affects multiple tools, not just the one the file was authored for.

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [ ] **`instructions/oversized` rule**: Warn when any instruction file exceeds the line limit (default: 100) or character limit (default: 15,000). Emit up to two separate diagnostics per file (one for lines, one for characters) using OR logic.
- [ ] **Recursive detection**: Discover instruction files at any depth in the project tree via a new `FeatureKind::Instructions` variant in the discovery pipeline. Case-insensitive filename matching.
- [ ] **Detected filenames**: `CLAUDE.md`, `AGENTS.md`, `COPILOT.md`, `INSTRUCTIONS.md`, `GEMINI.md`, `*.instructions.md` — all matched case-insensitively.
- [ ] **Configurable options**: Support `lines`, `characters`, and `resolve-imports` options inline under the rule key in `aipm.toml`. This extends the config system to support per-rule custom options (a new capability).
- [ ] **`resolve-imports` option**: When enabled (default: `false`), follow `@path/to/file` imports and relative markdown links (`[text](./path/to/file.md)`) to compute the total resolved content size. Track visited files to prevent circular import loops. Report both resolved total and direct file size in diagnostics.
- [ ] **`source_type` for diagnostics**: Use `"project"` for instruction files found outside `.ai/`, `.claude/`, `.github/` directories. Use the existing source type (`.claude`, `.github`, etc.) when the file is inside a recognized source directory.
- [ ] All rules configurable via `aipm.toml` `[workspace.lints]` (allow, warn, error, plus custom options).
- [ ] Rule has `help_text` and `help_url` guiding the user to reduce file size.
- [ ] Unit tests using `MockFs` covering: small file (pass), at-limit (pass), over-limit lines (fail), over-limit chars (fail), both exceeded (two diagnostics), case-insensitive detection, recursive subdirectory detection, `resolve-imports` with circular refs, `resolve-imports` with `@import` and markdown links, empty file, missing file.
- [ ] All four `cargo build/test/clippy/fmt` gates pass with zero warnings.
- [ ] Branch coverage remains >= 89%.

### 3.2 Non-Goals (Out of Scope)

- [ ] Checking personal/user-global instruction files (`~/.claude/CLAUDE.md`). The lint command operates on project directories only.
- [ ] Different thresholds for scoped `.instructions.md` files vs. always-injected files. Same defaults apply; users override via config.
- [ ] Depth limits on recursive scanning. The existing `SKIP_DIRS` list + gitignore filtering is sufficient.
- [ ] Auto-fix capability (splitting files, extracting sections). Future work.
- [ ] Validating instruction file content quality or structure (frontmatter correctness, etc.). This rule only checks size.
- [ ] Configurable import pattern syntax. Only `@path` and inline relative markdown links to local `.md` files are recognized.

## 4. Proposed Solution (High-Level Design)

### 4.1 Discovery Extension

Extend `FeatureKind` in `crates/libaipm/src/discovery.rs` with a new variant:

```
FeatureKind::Instructions — triggered by case-insensitive filename match against known instruction file patterns
```

Detection logic in `classify_feature_kind()` (case-insensitive):

| Pattern | Match Logic |
|---|---|
| `CLAUDE.md` | `file_name.eq_ignore_ascii_case("CLAUDE.md")` |
| `AGENTS.md` | `file_name.eq_ignore_ascii_case("AGENTS.md")` |
| `COPILOT.md` | `file_name.eq_ignore_ascii_case("COPILOT.md")` |
| `INSTRUCTIONS.md` | `file_name.eq_ignore_ascii_case("INSTRUCTIONS.md")` |
| `GEMINI.md` | `file_name.eq_ignore_ascii_case("GEMINI.md")` |
| `*.instructions.md` | `file_name_lower.ends_with(".instructions.md")` |

These checks apply at **any depth** — the existing `discover_features()` walk already recurses through the full project tree (gitignore-aware, skipping `SKIP_DIRS`).

### 4.2 Rule Dispatch

Add a new arm to `quality_rules_for_kind()` in `crates/libaipm/src/lint/rules/mod.rs`:

```rust
FeatureKind::Instructions => vec![
    Box::new(instructions_oversized::Oversized::default()),
],
```

And add the rule to `catalog()` for LSP integration.

### 4.3 Config Extension

Extend the TOML parsing in `load_lint_config()` (`crates/aipm/src/main.rs:748-836`) and the `RuleOverride` enum (`crates/libaipm/src/lint/config.rs`) to support per-rule custom options.

Config surface in `aipm.toml`:

```toml
[workspace.lints."instructions/oversized"]
level = "warn"
lines = 150
characters = 20000
resolve-imports = true
ignore = ["vendor/**"]
```

### 4.4 System Flow

```
aipm lint (CLI)
  → cmd_lint()
    → load_lint_config()           → reads aipm.toml (including custom options)
    → libaipm::lint::lint()
      → discover_features()        → walks tree, classifies FeatureKind::Instructions
      → for each Instructions feature:
        → instructions_oversized::Oversized.check_file()
          → read file content
          → count lines, count characters
          → if resolve-imports enabled:
            → parse @imports and relative markdown links
            → recursively resolve (tracking visited set)
            → compute resolved totals
          → emit diagnostics for exceeded limits
      → sort diagnostics
      → return Outcome
    → reporter.report()
```

### 4.5 Key Components

| Component | Responsibility | File Location | Justification |
|---|---|---|---|
| `FeatureKind::Instructions` | Classify instruction files during discovery | `crates/libaipm/src/discovery.rs` | Integrates with single-pass walk, reuses SKIP_DIRS |
| `instructions_oversized::Oversized` | Rule struct with configurable thresholds | `crates/libaipm/src/lint/rules/instructions_oversized.rs` | Follows established rule pattern (skill_oversized.rs) |
| `RuleOverride` extension | Support per-rule custom options | `crates/libaipm/src/lint/config.rs` | Required for configurable thresholds |
| `import_resolver` module | Follow @imports and markdown links | `crates/libaipm/src/lint/rules/import_resolver.rs` | Encapsulated import resolution logic |
| Config parsing extension | Parse custom options from TOML | `crates/aipm/src/main.rs` | Extends existing `load_lint_config()` |

## 5. Detailed Design

### 5.1 Discovery Changes (`discovery.rs`)

#### 5.1.1 `FeatureKind` Enum

```rust
pub enum FeatureKind {
    Skill,
    Agent,
    Hook,
    Plugin,
    Marketplace,
    PluginJson,
    Instructions,  // NEW
}
```

#### 5.1.2 `classify_feature_kind()` Extension

Add instruction file detection **before** the existing checks (to avoid a `.md` file in an `agents/` directory being misclassified if it happens to also match an instruction filename):

```rust
const INSTRUCTION_FILENAMES: &[&str] = &[
    "claude.md", "agents.md", "copilot.md", "instructions.md", "gemini.md",
];

fn classify_feature_kind(file_path: &Path) -> Option<FeatureKind> {
    let file_name = file_path.file_name()?.to_string_lossy();
    let file_name_lower = file_name.to_ascii_lowercase();

    // Instruction files — match by filename only (case-insensitive), any depth.
    if INSTRUCTION_FILENAMES.contains(&file_name_lower.as_ref()) {
        return Some(FeatureKind::Instructions);
    }
    if file_name_lower.ends_with(".instructions.md") {
        return Some(FeatureKind::Instructions);
    }

    // ... existing classification logic unchanged ...
}
```

**Important ordering consideration**: Instruction file checks MUST come first because a file like `agents/CLAUDE.md` should be classified as `Instructions`, not `Agent`. The existing `Agent` check matches `*.md` in an `agents/` directory — without this ordering, `CLAUDE.md` inside `agents/` would be misclassified. However, `AGENTS.md` inside an `agents/` directory is an instruction file, not an agent definition. The case-insensitive filename match on the known instruction filenames takes priority.

#### 5.1.3 `SourceContext` for Instructions

Instruction files at arbitrary depths may not be inside `.ai/`, `.claude/`, or `.github/`. The existing `classify_source_context()` already returns `None` for such files, and `source_type_from_path()` returns `"other"`. The rule will use `"project"` as the source type for instruction files outside recognized source directories:

```rust
// In the rule's check_file():
let source_type = match scan::source_type_from_path(file_path) {
    "other" => "project",
    s => s,
};
```

### 5.2 Rule Implementation (`instructions_oversized.rs`)

#### 5.2.1 Rule Struct

Unlike existing zero-sized unit structs, this rule carries configurable state:

```rust
/// Default line limit for instruction files.
const DEFAULT_MAX_LINES: usize = 100;

/// Default character limit for instruction files.
const DEFAULT_MAX_CHARS: usize = 15_000;

/// Checks that instruction files don't exceed line or character limits.
pub struct Oversized {
    pub max_lines: usize,
    pub max_chars: usize,
    pub resolve_imports: bool,
}
```

A `Default` implementation provides the defaults:

```rust
impl Default for Oversized {
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_MAX_LINES,
            max_chars: DEFAULT_MAX_CHARS,
            resolve_imports: false,
        }
    }
}
```

#### 5.2.2 Rule Trait Implementation

```rust
impl Rule for Oversized {
    fn id(&self) -> &'static str { "instructions/oversized" }
    fn name(&self) -> &'static str { "oversized instruction file" }
    fn default_severity(&self) -> Severity { Severity::Warning }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/instructions/oversized.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("reduce instruction file size; AI tools re-inject these files every conversation turn")
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        // ... see detailed logic below
    }

    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        // Instruction files are discovered by FeatureKind dispatch,
        // so check() delegates to check_file() via the default implementation,
        // or iterates the directory for instruction files.
        // ...
    }
}
```

#### 5.2.3 `check_file()` Logic

```
1. Read file content via fs.read_to_string(file_path)
   - If file doesn't exist or can't be read, return Ok(vec![])

2. Compute direct metrics:
   - direct_lines = content.lines().count()
   - direct_chars = content.len()

3. If resolve_imports is enabled:
   - Parse @imports and relative markdown links from content
   - Recursively resolve imports (see §5.3)
   - Compute resolved_lines and resolved_chars (sum of all unique files)
   - Use resolved totals for threshold comparison
   - Include both resolved and direct sizes in diagnostic message
   Else:
   - Use direct_lines and direct_chars for threshold comparison

4. Emit diagnostics (OR logic — up to 2 per file):
   a. If effective_lines > max_lines:
      → Diagnostic with message including line count and limit
   b. If effective_chars > max_chars:
      → Diagnostic with message including char count and limit

5. Return collected diagnostics
```

**Diagnostic message format** (without resolve-imports):
```
CLAUDE.md exceeds 100 line limit (247 lines)
CLAUDE.md exceeds 15000 character limit (23450 chars)
```

**Diagnostic message format** (with resolve-imports enabled):
```
CLAUDE.md exceeds 100 line limit (350 lines resolved from 4 imports, 120 lines in file directly)
CLAUDE.md exceeds 15000 character limit (23450 chars resolved from 4 imports, 5200 chars in file directly)
```

### 5.3 Import Resolution (`import_resolver.rs`)

A new module encapsulating import chain resolution logic:

#### 5.3.1 Recognized Import Syntaxes

1. **`@path/to/file` imports** (Claude Code convention):
   - Regex: `^@([^\s]+\.md)\s*$` (line must start with `@`, path must end in `.md`)
   - Resolved relative to the directory containing the importing file

2. **Relative markdown inline links** to local `.md` files:
   - Regex: `\[([^\]]*)\]\(([^)]+\.md)\)` where the URL does not start with `http://` or `https://`
   - Only links pointing to `.md` files are followed
   - Resolved relative to the directory containing the importing file

#### 5.3.2 Resolution Algorithm

```
fn resolve_imports(
    file_path: &Path,
    fs: &dyn Fs,
    visited: &mut HashSet<PathBuf>,
) -> (usize, usize)  // (total_lines, total_chars)
{
    1. Canonicalize file_path
    2. If visited.contains(file_path), return (0, 0)  // circular ref — skip
    3. Insert file_path into visited
    4. Read file content
    5. Accumulate content.lines().count() and content.len()
    6. For each @import and relative markdown link found:
       a. Resolve the target path relative to file_path's parent directory
       b. Reject paths containing ".." traversal or absolute paths (security)
       c. Recursively call resolve_imports(target_path, fs, visited)
       d. Add returned (lines, chars) to accumulator
    7. Return accumulated totals
}
```

**Circular import handling**: Track visited file paths in a `HashSet<PathBuf>`. If a file has already been visited, skip it silently and return `(0, 0)`. No error, no warning — the content is simply not double-counted.

**Path traversal protection**: Reject import targets containing `..` segments or that resolve to absolute paths. This mirrors the existing path traversal protection in `broken_paths.rs`.

### 5.4 Config System Extension

#### 5.4.1 `RuleOverride` Extension (`config.rs`)

Extend the `Detailed` variant to carry optional per-rule options:

```rust
pub enum RuleOverride {
    Allow,
    Level(Severity),
    Detailed {
        level: Severity,
        ignore: Vec<String>,
        options: BTreeMap<String, toml::Value>,  // NEW: per-rule custom options
    },
}
```

Add a method to extract options:

```rust
impl RuleOverride {
    pub fn rule_options(&self) -> &BTreeMap<String, toml::Value> {
        static EMPTY: BTreeMap<String, toml::Value> = BTreeMap::new();
        match self {
            Self::Detailed { options, .. } => options,
            _ => &EMPTY,
        }
    }
}
```

#### 5.4.2 Config Loading (`main.rs`)

Extend `load_lint_config()` to parse custom keys from the TOML table. Currently, the function reads `level` and `ignore` from table values. Additional keys are passed through into the `options` map:

```rust
// When processing a TOML table for a rule:
// Known keys: "level", "ignore"
// Everything else: forwarded to options BTreeMap
for (key, value) in table {
    match key.as_str() {
        "level" => { /* existing parsing */ },
        "ignore" => { /* existing parsing */ },
        _ => { options.insert(key.clone(), value.clone()); },
    }
}
```

#### 5.4.3 Rule Construction with Config

The lint pipeline needs to pass config options to the rule constructor. Currently, rules are constructed statelessly in `quality_rules_for_kind()`. For the `Instructions` kind, the rule must be constructed with options from config:

```rust
FeatureKind::Instructions => {
    let opts = config.rule_options("instructions/oversized");
    let max_lines = opts.get("lines")
        .and_then(|v| v.as_integer())
        .map(|v| v as usize)
        .unwrap_or(DEFAULT_MAX_LINES);
    let max_chars = opts.get("characters")
        .and_then(|v| v.as_integer())
        .map(|v| v as usize)
        .unwrap_or(DEFAULT_MAX_CHARS);
    let resolve_imports = opts.get("resolve-imports")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    vec![Box::new(Oversized { max_lines, max_chars, resolve_imports })]
}
```

**Signature change**: `quality_rules_for_kind()` currently takes only `kind: &FeatureKind`. It needs to also accept `config: &Config` to extract rule options. This is a minimal API change since the function is `pub(crate)`.

```rust
pub(crate) fn quality_rules_for_kind(kind: &FeatureKind, config: &Config) -> Vec<Box<dyn Rule>> {
    // ... existing arms unchanged (they ignore config) ...
    // ... new Instructions arm uses config ...
}
```

### 5.5 TOML Config Surface

Full example of the configurable options in `aipm.toml`:

```toml
# Override severity and thresholds
[workspace.lints."instructions/oversized"]
level = "error"           # Override severity (default: "warn")
lines = 200               # Max lines per file (default: 100)
characters = 20000        # Max characters per file (default: 15000)
resolve-imports = true    # Follow @imports and markdown links (default: false)
ignore = ["vendor/**"]    # Skip specific paths

# Or suppress entirely
[workspace.lints]
"instructions/oversized" = "allow"
```

### 5.6 Diagnostic Examples

**Basic (resolve-imports = false):**

```
warning[instructions/oversized]: CLAUDE.md exceeds 100 line limit (247 lines)
   --> CLAUDE.md:1
    |
  1 | # CLAUDE.md — Project Rules for AI Agents
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: reduce instruction file size; AI tools re-inject these files every conversation turn
```

**With resolve-imports = true:**

```
warning[instructions/oversized]: CLAUDE.md exceeds 15000 character limit (23450 chars resolved from 4 imports, 5200 chars in file directly)
   --> CLAUDE.md:1
    |
  1 | # CLAUDE.md — Project Rules for AI Agents
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: reduce instruction file size; AI tools re-inject these files every conversation turn
```

**Subdirectory file:**

```
warning[instructions/oversized]: packages/auth/CLAUDE.md exceeds 100 line limit (187 lines)
   --> packages/auth/CLAUDE.md:1
```

**Scoped instruction file:**

```
warning[instructions/oversized]: .github/instructions/frontend.instructions.md exceeds 100 line limit (340 lines)
   --> .github/instructions/frontend.instructions.md:1
```

## 6. Alternatives Considered

| Option | Pros | Cons | Reason for Rejection |
|---|---|---|---|
| **Standalone scan (no FeatureKind)** | No changes to discovery.rs; fully self-contained rule | Second filesystem traversal per lint run; doesn't reuse SKIP_DIRS; inconsistent with single-pass design | Rejected: violates the single-pass walk principle |
| **Hardcoded thresholds only** | Simpler config; consistent with all 17 existing rules | Users can't tune for their project; issue explicitly requests configurability | Rejected: issue requirements mandate configurability |
| **Separate rules for lines vs. chars** | Cleaner rule IDs; independent suppression | Doubles the number of rules; over-engineered for the use case | Rejected: one rule with OR logic is simpler |
| **Token-based limit instead of chars** | More accurate cost model (tokens ≠ chars) | Requires a tokenizer dependency; different models tokenize differently | Rejected: character counting is a good-enough proxy (skill/oversized uses chars too) |
| **Stub resolve-imports (defer implementation)** | Faster initial delivery | Import chains are a real source of hidden bloat; users requested full implementation | Rejected per decision in spec wizard |
| **Depth-limited scan** | Avoids false positives in deeply nested trees | SKIP_DIRS + gitignore already filter junk; depth limits miss legitimate deep instruction files | Rejected: existing filtering is sufficient |
| **Exact case matching only** | No false positives on differently-cased files | Misses `claude.md`, `Claude.md` etc. on case-sensitive filesystems; confusing UX | Rejected: case-insensitive is more forgiving |

## 7. Cross-Cutting Concerns

### 7.1 Performance

- **Single-pass walk**: Instruction files are discovered during the existing `discover_features()` walk. No additional filesystem traversal is needed.
- **resolve-imports cost**: When enabled, each instruction file triggers additional file reads for imports. This is bounded by the `visited` set (each file read at most once) and path traversal rejection. The cost is proportional to the number of unique import targets, not the depth of the import chain.
- **Case-insensitive matching**: `.to_ascii_lowercase()` on each filename during the walk adds negligible overhead.

### 7.2 Compatibility

- **Config backward compatibility**: Existing `aipm.toml` files that don't mention `instructions/oversized` are unaffected — the rule uses defaults. The config extension (forwarding unknown TOML keys to an `options` map) is backward-compatible since existing rules don't read options.
- **`quality_rules_for_kind()` signature change**: Adding `config: &Config` parameter is a breaking change to the `pub(crate)` API. All call sites within the crate must be updated. External consumers are unaffected since the function is not `pub`.
- **`FeatureKind::Instructions` addition**: This is an enum variant addition. All `match` arms on `FeatureKind` must add a new branch. This affects `quality_rules_for_kind()`, `catalog()`, and any display/serialization implementations.

### 7.3 LSP Integration

The new rule must be added to `catalog()` in `rules/mod.rs` so the LSP's `build_rule_index()` (`crates/aipm/src/lsp/helpers.rs:32`) includes it in hover/completion/diagnostic features. The `lint_file()` function in `lsp/helpers.rs` will automatically dispatch the rule for files classified as `FeatureKind::Instructions`.

## 8. Migration, Rollout, and Testing

### 8.1 Deployment Strategy

This is a new lint rule with no migration. It ships as a `Warning` by default. Users can:
- Suppress: `"instructions/oversized" = "allow"` in `aipm.toml`
- Escalate: `level = "error"` in `aipm.toml`
- Tune: Adjust `lines` and `characters` thresholds

### 8.2 Test Plan

#### Unit Tests (`instructions_oversized.rs`)

| Test Case | Description | Expected |
|---|---|---|
| `small_file_no_finding` | File under both limits | No diagnostics |
| `exactly_at_line_limit` | Exactly 100 lines | No diagnostics |
| `exactly_at_char_limit` | Exactly 15,000 characters | No diagnostics |
| `over_line_limit` | 101+ lines, under char limit | 1 diagnostic (lines) |
| `over_char_limit` | Under line limit, 15,001+ characters | 1 diagnostic (chars) |
| `both_limits_exceeded` | Over both limits | 2 diagnostics |
| `empty_file` | Empty instruction file | No diagnostics |
| `missing_file` | File doesn't exist | No diagnostics (empty vec) |
| `case_insensitive_detection` | `claude.md` (lowercase) detected | File is discovered and checked |
| `instructions_md_suffix` | `frontend.instructions.md` detected | File is discovered and checked |
| `subdirectory_detection` | `packages/auth/CLAUDE.md` | File is discovered and checked |
| `custom_thresholds` | Rule constructed with lines=50 | Triggers at 51 lines |
| `source_type_project` | File at project root | `source_type == "project"` |
| `source_type_claude` | File at `.claude/CLAUDE.md` | `source_type == ".claude"` |

#### Import Resolver Tests (`import_resolver.rs`)

| Test Case | Description | Expected |
|---|---|---|
| `at_import_basic` | File with `@path/to/file.md` | Follows import, sums sizes |
| `markdown_link_basic` | File with `[text](./file.md)` | Follows link, sums sizes |
| `external_url_ignored` | `[text](https://example.com/file.md)` | Not followed |
| `non_md_link_ignored` | `[text](./config.json)` | Not followed |
| `circular_import` | A imports B, B imports A | No infinite loop, each file counted once |
| `path_traversal_rejected` | `@../../etc/passwd` | Import ignored |
| `absolute_path_rejected` | `@/etc/passwd` | Import ignored |
| `nested_imports` | A → B → C (chain of 3) | All three files counted |
| `missing_import_target` | `@nonexistent.md` | Import silently skipped |
| `mixed_imports` | File with both `@import` and markdown links | Both followed |

#### Discovery Tests (`discovery.rs`)

| Test Case | Description | Expected |
|---|---|---|
| `classify_claude_md` | `CLAUDE.md` at root | `FeatureKind::Instructions` |
| `classify_claude_md_lowercase` | `claude.md` at root | `FeatureKind::Instructions` |
| `classify_agents_md_in_agents_dir` | `agents/AGENTS.md` | `FeatureKind::Instructions` (not Agent) |
| `classify_instructions_md_suffix` | `frontend.instructions.md` | `FeatureKind::Instructions` |
| `classify_regular_agent_unchanged` | `agents/security-reviewer.md` | `FeatureKind::Agent` (unchanged) |

#### Integration Tests

| Test Case | Description |
|---|---|
| `lint_discovers_instruction_files` | Full `lint()` call discovers and reports instruction file violations |
| `lint_config_overrides_thresholds` | Custom `lines`/`characters` in config are respected |
| `lint_config_allow_suppresses` | `"allow"` in config suppresses the rule |

## 9. Implementation Checklist

Files to create:
- [ ] `crates/libaipm/src/lint/rules/instructions_oversized.rs` — Rule struct and trait impl
- [ ] `crates/libaipm/src/lint/rules/import_resolver.rs` — Import resolution module

Files to modify:
- [ ] `crates/libaipm/src/discovery.rs` — Add `FeatureKind::Instructions`, extend `classify_feature_kind()`
- [ ] `crates/libaipm/src/lint/rules/mod.rs` — Add module declarations, extend `quality_rules_for_kind()` (with config param), extend `catalog()`
- [ ] `crates/libaipm/src/lint/config.rs` — Extend `RuleOverride::Detailed` with `options` field, add `rule_options()` method
- [ ] `crates/libaipm/src/lint/mod.rs` — Pass `config` to `quality_rules_for_kind()` calls
- [ ] `crates/aipm/src/main.rs` — Extend `load_lint_config()` to forward custom TOML keys to `options`
- [ ] `crates/aipm/src/lsp/helpers.rs` — Verify `build_rule_index()` picks up new rule via `catalog()` (may need no changes if catalog() is already called generically)

## 10. Open Questions / Unresolved Issues

All questions from the research document have been resolved through the spec wizard. No outstanding questions remain. Decisions are recorded in this spec.

Summary of key decisions:
1. Character default: **15,000** (matches `skill/oversized` precedent)
2. Include GEMINI.md: **Yes**
3. Include `.instructions.md` scoped files: **Yes, same thresholds**
4. Personal files: **No, project-only**
5. Architecture: **FeatureKind::Instructions** (extend discovery pipeline)
6. Config schema: **Inline under rule key** in `aipm.toml`
7. Trigger logic: **OR** (either limit, up to 2 diagnostics)
8. Depth limit: **None** (rely on SKIP_DIRS + gitignore)
9. Case sensitivity: **Case-insensitive**
10. Scoped threshold: **Same as always-injected**
11. Import resolution: **Implement now**, `resolve-imports` option, default off
12. Circular imports: **Track visited, skip duplicates**
13. Diagnostic reporting: **Both** resolved total and direct size
14. Import syntax: **@path + relative markdown inline links** to local `.md` files
15. Markdown link scope: **Inline links** only (not reference-style)
16. resolve-imports default: **Disabled** (opt-in)
