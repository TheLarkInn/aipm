---
date: 2026-04-03 18:30:00 UTC
researcher: Claude Code
git_commit: a664b021936c7e4e2fb18a751c5e3bcb666c26b1
branch: main
repository: aipm
topic: "[lint] Lint error, warning, display, UX, DX, and grouping (Issue #198)"
tags: [research, lint, display, ux, colorization, grouping, reporter, lsp, snippets, rule-links]
status: complete
last_updated: 2026-04-03
last_updated_by: Claude Code
---

# Research: Lint Display UX — Issue #198

## Research Question

Issue #198 asks: how can `aipm lint` produce richer, more user-friendly terminal output?
The issue identifies six areas: (1) colorization, (2) diagnostic grouping, (3) rule documentation
links, (4) precise source locations (line:col ranges), (5) inline source code snippets, and
(6) a reporter/format system serving multiple consumption mediums (CI, AI agents, human, web, LSP).

**Issue**: https://github.com/TheLarkInn/aipm/issues/198

---

## Summary

`aipm lint` v0.16.1 produces plain-text output modeled loosely on clippy/rustc but missing
color, grouping, help text, column ranges, and source snippets. The `Reporter` trait is already
present and extensible, and the `FoundSkill.content` string is available inside rules — so
snippet display is mechanically possible today without changing the `Diagnostic` struct,
provided the struct gains a `column` field and rules track field-level offsets.

The six areas in Issue #198 map cleanly to the patterns used by rustc, ESLint, and Biome.
Each area has a clear, precedented solution.

---

## Detailed Findings

### 1. Current State

#### 1.1 `Diagnostic` Struct

Defined at [`crates/libaipm/src/lint/diagnostic.rs:40-53`](https://github.com/TheLarkInn/aipm/blob/a664b021936c7e4e2fb18a751c5e3bcb666c26b1/crates/libaipm/src/lint/diagnostic.rs#L40-L53):

```rust
pub struct Diagnostic {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub file_path: PathBuf,
    pub line: Option<usize>,        // 1-based line number, or None for directory-level
    pub source_type: String,
}
```

**Missing fields for Issue #198**: no `column`, no `end_line`, no `end_col`, no `source_snippet`, no `help_url`, no `help_text`.

#### 1.2 `Rule` Trait

Defined at [`crates/libaipm/src/lint/rule.rs:16-31`](https://github.com/TheLarkInn/aipm/blob/a664b021936c7e4e2fb18a751c5e3bcb666c26b1/crates/libaipm/src/lint/rule.rs#L16-L31):

```rust
pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn default_severity(&self) -> Severity;
    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error>;
}
```

**Missing for Issue #198**: no `is_rangeable()`, no `help_url()`, no `help_text()`.

#### 1.3 `Reporter` Trait and Implementations

Defined at [`crates/libaipm/src/lint/reporter.rs:8-15`](https://github.com/TheLarkInn/aipm/blob/a664b021936c7e4e2fb18a751c5e3bcb666c26b1/crates/libaipm/src/lint/reporter.rs#L8-L15):

```rust
pub trait Reporter {
    fn report(&self, outcome: &Outcome, writer: &mut dyn Write) -> std::io::Result<()>;
}
```

Two implementations exist:
- `Text` — human-readable, clippy-style, **no color**, **no grouping**, **no snippets**
- `Json` — machine-readable JSON with diagnostics array + summary, **no column**, **no help_url**

CLI selection at [`crates/aipm/src/main.rs:549-556`](https://github.com/TheLarkInn/aipm/blob/a664b021936c7e4e2fb18a751c5e3bcb666c26b1/crates/aipm/src/main.rs#L549-L556):

```rust
match format {
    "json" => libaipm::lint::reporter::Json.report(&outcome, &mut stdout)?,
    _      => libaipm::lint::reporter::Text.report(&outcome, &mut stdout)?,
}
```

#### 1.4 Current `Text` Reporter Output (per `write_diagnostic`)

[`crates/libaipm/src/lint/reporter.rs:46-52`](https://github.com/TheLarkInn/aipm/blob/a664b021936c7e4e2fb18a751c5e3bcb666c26b1/crates/libaipm/src/lint/reporter.rs#L46-L52):

```
warning[skill/oversized]: SKILL.md exceeds 15000 character limit (19497 chars)
  --> .ai/<plugin>/skills/perf-anti-patterns/SKILL.md:1
  |
```

No color, no grouping, no help text, no rule links, no source snippet.

#### 1.5 What Each Rule Currently Provides

| Rule | `line` value | Has raw content? | Rangeable? |
|------|-------------|-----------------|------------|
| `skill/missing-name` | `Some(1)` hardcoded | Yes (`FoundSkill.content`) | Partial — field is *absent*, no range to point to |
| `skill/missing-description` | `Some(1)` hardcoded | Yes | Partial — field is *absent* |
| `skill/oversized` | `Some(1)` hardcoded | Yes | Whole-file finding, not rangeable |
| `skill/name-too-long` | `Some(1)` hardcoded | Yes | Could point to `name:` field line if tracked |
| `skill/name-invalid-chars` | `Some(1)` hardcoded | Yes | Could point to `name:` field line |
| `skill/description-too-long` | `Some(1)` hardcoded | Yes | Could point to `description:` field line |
| `skill/invalid-shell` | `Some(1)` hardcoded | Yes | Could point to `shell:` field line |
| `agent/missing-tools` | `Some(1)` hardcoded | No (`FoundAgent` has no `content`) | Partial — field is *absent* |
| `hook/unknown-event` | `Some(n)` (JSON parse offset) | Yes (raw JSON string) | Yes, if JSON key position tracked |
| `hook/legacy-event-name` | `None` | Yes (raw JSON string) | Yes, if JSON key position tracked |
| `plugin/broken-paths` | `Some(n)` | Yes | Yes, if path reference line tracked |
| `source/misplaced-features` | `None` | No (directory-level) | Not rangeable — no file, no line |

**Key finding**: `Frontmatter` (at `crates/libaipm/src/frontmatter.rs`) tracks `start_line` and
`end_line` of the frontmatter block but does **not** track per-field line numbers. To enable
per-field ranges, `Frontmatter` would need to store field positions, or rules would need to
do a line-by-line search of `skill.content`.

---

### 2. Colorization — What Other Tools Do

#### 2.1 Rustc / Clippy

Rustc uses its own internal `Emitter` + the [`annotate-snippets`](https://crates.io/crates/annotate-snippets) crate
(an extracted public crate from the rustc source). Color assignment:

| Output element | Color |
|---------------|-------|
| `error:` label | Bold red |
| `warning:` label | Bold yellow |
| `note:` label | Bold cyan |
| `help:` label | Bold cyan |
| Rule ID `[E0425]` | Bold |
| `-->` arrow | Bold blue |
| Line numbers | Bold blue |
| `|` gutter separator | Bold blue |
| Underline carets `^^^` | Color of severity |
| Highlighted source text | Bold |

Clippy colors **only the label word** (`error`, `warning`) and structural elements, not the
entire line. This is the "partial coloring" model that keeps output readable in terminals
and when piped.

Color is auto-detected via:
- `TERM=dumb` → disable
- `NO_COLOR` env var (no-color.org standard) → disable
- stdout not a TTY (`isatty()`) → disable
- `--color=never|auto|always` flag

#### 2.2 ESLint

ESLint's default `stylish` formatter:
- **File header**: bold underline
- **Rule ID**: dim/faint (after message)
- **Error/warning**: `error` in bold red, `warning` in bold yellow
- **Summary line**: bold

ESLint colors partial — severity label and rule ID only.

ESLint also supports `--color`/`--no-color` and respects `NO_COLOR`.

#### 2.3 OxLint

OxLint uses `termcolor` internally. Its output is similar to rustc-style with colored severity
labels and underlined source locations.

#### 2.4 Biome

Biome uses its own Rust-based formatter with rich color support. It colors severity labels,
diagnostic titles, and file paths differently. It respects `NO_COLOR`.

#### 2.5 `aipm.toml` Color Configuration

The issue asks for color configuration in `aipm.toml`. ESLint does not expose per-user color
configuration in its config file (color is a CLI flag). Clippy/rustc also use CLI flags only.
**No major lint tool exposes color config in the project config file.** The convention is:
- CLI flag `--color=never|auto|always`
- Environment variable `NO_COLOR`
- Environment variable `CLICOLOR=0`

---

### 3. Grouping — What Other Tools Do

#### 3.1 ESLint's File-Grouped Model

ESLint's `stylish` formatter groups diagnostics under a file header:

```
/path/to/file.js
  3:5   error    'x' is defined but never used  no-unused-vars
  8:12  warning  Unexpected console statement    no-console

/path/to/other.js
  1:1   warning  File has too many lines         max-lines

✖ 3 problems (1 error, 2 warnings)
```

Key aspects:
- File path printed once as a header (bold/underlined)
- All diagnostics for that file indented under it
- Rule ID appears at the end of each line (dim)
- Summary at bottom: `✖ N problems (E errors, W warnings)`

#### 3.2 Rustc/Clippy's Per-Diagnostic Model

Rustc prints each diagnostic as a self-contained block:

```
error[E0425]: cannot find value `undefined_var` in this scope
 --> src/main.rs:3:5
  |
3 |     println!("{}", undefined_var);
  |                    ^^^^^^^^^^^^^ not found in this scope
  |
  = note: ...
  = help: for further information visit https://doc.rust-lang.org/...
```

Not grouped by file — each diagnostic is independent. This is appropriate when diagnostics
include full source context so the file+line is already obvious.

#### 3.3 Repo-Wide vs File-Specific Grouping

The issue asks specifically about this distinction. In the current output:

- **File-specific** rules: `skill/missing-name`, `skill/oversized`, `hook/unknown-event`, etc.
  — each diagnostic points to a specific file.
- **Repo-wide** (directory-level) rules: `source/misplaced-features` — points to a directory,
  not a file. `line: None`.

**No mainstream lint tool has a formal "repo-wide" category**. The closest analogy is:
- ESLint has "file-ignored" and "file-not-found" meta-diagnostics
- Clippy has `#![warn(...)]` workspace-level lint attributes
- Biome has workspace-level rules that apply without a specific file context

The `source/misplaced-features` rule is unique in aipm — it finds directories, not files.
It could be grouped under a special section header like `workspace checks:` or displayed
before file-specific rules.

#### 3.4 Sorting and Grouping Strategy

Current: sorted by `file_path` lexicographically
([`crates/libaipm/src/lint/mod.rs:131`](https://github.com/TheLarkInn/aipm/blob/a664b021936c7e4e2fb18a751c5e3bcb666c26b1/crates/libaipm/src/lint/mod.rs#L131)).

Grouping options:
1. **By file** (ESLint model): group diagnostics under file header — good when many diagnostics per file
2. **By severity then file** (errors first, then warnings): good for CI where errors need attention
3. **By source type** (`.ai`, `.claude`, `.github`): natural for aipm given its multi-source model
4. **Repo-wide first, file-specific second**: separate `source/misplaced-features` from others

---

### 4. Rule Documentation Links

#### 4.1 How Rustc/Clippy Links Rules

Rustc shows:
```
  = help: for further information visit https://rust-lang.github.io/rust-clippy/...
```

Clippy has a dedicated docs page per rule at `https://rust-lang.github.io/rust-clippy/master/index.html#rule_name`.

The link is added as a `note`/`help` line in the diagnostic block. Each rule knows its URL
via a `const` — there is no central URL registry at runtime.

#### 4.2 How ESLint Links Rules

ESLint's `stylish` formatter shows the rule ID at the end of each line. The rule ID is a
direct link to `https://eslint.org/docs/latest/rules/<rule-id>`.

In ESLint's JSON output, each diagnostic includes a `ruleId` field. The consumer can construct
the URL.

#### 4.3 How LSP Handles Rule Links

The LSP `Diagnostic` interface has:
```typescript
codeDescription?: { href: URI }
```

This field is specifically for "a URI to open with more information about the diagnostic error."
Editors render this as a clickable link next to the diagnostic. This is the natural place for
aipm's rule documentation link in an LSP context.

#### 4.4 Implications for `aipm lint`

For the human `Text` reporter:
```
warning[skill/missing-description]: SKILL.md missing required field: description
  --> .ai/my-plugin/skills/default/SKILL.md:1
  |
  = help: add a "description" field to the YAML frontmatter
  = help: for further information visit https://aipm.dev/rules/skill/missing-description
```

For the `Json` reporter, add `"help_url"` field to each diagnostic object.

For LSP, populate `codeDescription.href`.

The `Rule` trait would expose `fn help_url(&self) -> Option<&'static str>` and/or
`fn help_text(&self) -> Option<&'static str>`.

---

### 5. Source Locations (Line:Col Ranges)

#### 5.1 Current State

All skill/agent rules hardcode `line: Some(1)`. Hook rules use `None` or a rough line.
No column information exists in the `Diagnostic` struct.

#### 5.2 What Rustc Does

```
error[E0425]: cannot find value `x`
 --> src/main.rs:10:5
  |
10 |     println!("{}", x);
  |                    ^ not found in this scope
```

Format: `file:line:col` in the `-->` arrow line, then the source line, then a caret `^` at
the precise column.

#### 5.3 What ESLint Does

```
/path/file.js
  3:5   error   ...
```

Format: `line:col` in the first column of each diagnostic row.

#### 5.4 The "Rangeable" Trait Concept (from Issue #198)

Issue #198 proposes a trait method `is_rangeable()` on `Rule` that declares whether the rule
can provide line:col ranges. This would:

1. Gate the snippet display — only rangeable rules show the source excerpt
2. Force each rule implementor to consciously declare and implement range reporting

Precedent: no mainstream lint tool formally uses a `Rangeable` interface — instead, the
`Diagnostic` struct itself has optional range fields and rules populate what they can.
The LSP approach (`range` is required but can be set to `{ start: {line:0, col:0}, end: {line:0, col:0} }`)
is similar — "declare what you have."

#### 5.5 What Data Is Currently Available per Rule

Rules using `scan_skills()` have `FoundSkill.content` (full file text) and
`Frontmatter.start_line`/`end_line`. To enable field-level ranges:

- **Option A**: Add `line: Option<usize>` per field to `Frontmatter` — requires parser change
- **Option B**: Rule does a `content.lines().enumerate().find(|(_, l)| l.contains("name:"))` search — fragile
- **Option C**: Rules report the frontmatter block range (`start_line..end_line`) instead of field range — simpler, less precise

For the `source/misplaced-features` rule: directory-level, `line: None`, `col: None`. Not rangeable.

For hook rules: JSON parsing could use `serde_json::from_str::<serde_json::Value>()` with a
custom deserializer that tracks byte offsets, but this is complex. Alternatively, the rule
could search the raw JSON string for the key.

---

### 6. Source Code Snippets (3-Line Context Window)

#### 6.1 How Rustc Renders Snippets

```
error[E0425]: ...
 --> src/main.rs:10:5
  |
 9 |   let y = 2;
10 |     println!("{}", undefined_var);
  |                    ^^^^^^^^^^^^^ not found in this scope
11 | }
  |
```

Structure:
- Line before the error (context)
- The error line itself
- Underline carets `^` pointing to the precise span
- Line after the error (context)
- Optionally: `note:` or `help:` annotations below

This is rendered by the `annotate-snippets` crate.

#### 6.2 How ESLint Renders Snippets

ESLint's `stylish` formatter does **not** show source code snippets. ESLint's `html` formatter
and plugins like `eslint-plugin-prettier` do show them. Biome and OxLint do show snippets.

#### 6.3 Availability of Source Text in Current Rules

`FoundSkill.content` (raw file text string) is available in all skill rules. This makes
snippet display mechanically straightforward for file-based rules. The snippet renderer would:

1. Split `content` into lines
2. Extract `line - 1`, `line`, `line + 1` (0-indexed)
3. Format with gutter (line numbers, `|` separator)
4. If column range available: add underline carets

For `source/misplaced-features` (directory-level, no file): no snippet possible.
For `hook/legacy-event-name` (JSON file, no line): could show the JSON file but without a
specific line to highlight, it is not useful.

#### 6.4 The `annotate-snippets` Crate

`annotate-snippets` (`crates.io/crates/annotate-snippets`) is the library used by rustc itself
to render diagnostic snippets. It supports:
- Multi-span annotations
- Primary and secondary annotations
- Notes and help messages
- ANSI color output via `termcolor`
- No-color fallback

It takes a `Snippet` struct with slices of source text and annotation ranges, and produces
formatted output.

**Alternative**: `ariadne` (`crates.io/crates/ariadne`) is a higher-level diagnostic rendering
crate that also supports beautiful multi-span output with colors.

**Alternative**: `codespan-reporting` is similar but lighter.

---

### 7. Reporter / Format System

#### 7.1 Current State

Two reporters exist: `Text` and `Json`. CLI selection is a simple `match` on the `--format` string.

#### 7.2 What Issue #198 Proposes

Four consumption mediums:
1. **Human** (default) — rich colored output with snippets, grouping, help text
2. **CI** — possibly a subset of human (no color), or GitHub Actions annotations format
3. **AI Agents/LLMs** — structured but human-readable, with rule IDs and documentation links
4. **Web** (v2 optional) — HTML output
5. **LSP** — JSON-RPC compliant LSP `publishDiagnostics` format

#### 7.3 How ESLint's Reporter System Works

ESLint separates "formatters" from the core. Each formatter is a function:
```typescript
function(results: LintResult[]): string
```

Built-in formatters: `stylish` (default), `compact`, `json`, `json-with-metadata`, `checkstyle`,
`html`, `junit`, `sarif`, `tap`, `unix`, `visualstudio`.

Custom formatters are npm packages loaded via `--format=@scope/package` or a file path.

**SARIF** (Static Analysis Results Interchange Format) is a JSON schema standardized by
Microsoft/OASIS for exchanging static analysis results. It is used by GitHub Advanced Security
to display code scanning results in the GitHub UI. ESLint's `sarif` formatter produces SARIF.

**GitHub Actions annotation format** is a special `::error file=...,line=...,col=...::message`
syntax that GitHub Actions renders as inline PR comments. ESLint's `compact` formatter can
be adapted for this.

#### 7.4 How Biome's Reporter System Works

Biome has a `--reporter` flag with values: `summary` (default), `json`, `json-pretty`,
`github` (GitHub Actions annotations), `junit`, `tap`. The `github` reporter produces
`::error` and `::warning` annotation commands.

#### 7.5 LSP Integration Model

There are two integration models for lint tools and LSP:

1. **Full LSP server**: The lint tool implements the LSP protocol and runs as a persistent daemon
   that editors communicate with. Example: biome-lsp, typescript-language-server.

2. **Wrapper/subprocess model**: A general-purpose language server (like
   `efm-langserver`, `null-ls`, or `none-ls` for Neovim) invokes the CLI tool as a subprocess
   and translates its output into LSP diagnostics. Requires a machine-readable output format.

For `aipm lint`, a dedicated LSP server is out of scope (confirmed by the existing spec).
The practical path is a machine-readable output that adapters can consume. The JSON reporter
already provides this, but adding `column`, `end_line`, `end_col`, and `help_url` fields
would make it fully LSP-compatible without any extra translation.

LSP `Diagnostic` structure that a `Json` reporter should match:
```json
{
  "range": {
    "start": { "line": 0, "character": 0 },
    "end": { "line": 0, "character": 0 }
  },
  "severity": 1,
  "code": "skill/missing-description",
  "codeDescription": { "href": "https://aipm.dev/rules/skill/missing-description" },
  "source": "aipm",
  "message": "SKILL.md missing required field: description"
}
```

Note: LSP line/character numbers are **0-based**. The current `Diagnostic.line` is **1-based**.
Any LSP adapter must subtract 1.

#### 7.6 CI Reporter Patterns

| Tool | CI-specific output | How triggered |
|------|-------------------|---------------|
| ESLint | GitHub Actions annotations (`::error`) | `--format github` or `@microsoft/eslint-formatter-sarif` |
| Biome | GitHub Actions annotations | `--reporter github` |
| Clippy | GitHub Actions annotations | `clippy-action` GitHub Action wraps JSON output |
| OxLint | GitHub Actions annotations | `--format github` |

**GitHub Actions annotation format** (emitted to stdout):
```
::error file=.ai/plugin/skills/SKILL.md,line=1,col=1::skill/missing-description: SKILL.md missing required field: description
::warning file=.ai/plugin/skills/SKILL.md,line=1,col=1::skill/oversized: SKILL.md exceeds 15000 chars
```

This causes GitHub to display inline annotations on PR diffs. It costs nothing to add as a
reporter since it is plain text with no extra dependencies.

---

### 8. Rust Crate Landscape for Terminal Diagnostics

| Crate | Approach | Color detection | Notes |
|-------|----------|----------------|-------|
| `termcolor` | Low-level ANSI + Windows console API | `ColorChoice` enum | Used by rustc, codespan-reporting |
| `owo-colors` | Zero-alloc ANSI extensions on `Display` | `supports-color` crate | No Windows Console API |
| `anstream` | Auto-detected stream wrapping | `NO_COLOR`, `CLICOLOR`, TTY | Used by clap |
| `annotate-snippets` | Rustc-style snippet rendering | Via `termcolor` | Exact rustc output format |
| `ariadne` | Rich colorful diagnostic rendering | Built-in | Prettier than rustc, less standard |
| `codespan-reporting` | Similar to annotate-snippets | Via `termcolor` | More ergonomic API |
| `miette` | High-level derive macro approach | Built-in | Best for application errors, not lint |

**Recommendation from research**: `annotate-snippets` + `termcolor` most closely matches the
clippy output the spec is modeled after and is already used in the Rust ecosystem widely.
`anstream` is what `clap` (already a workspace dependency) uses, so it may already be
transitively available.

**`NO_COLOR` standard** (https://no-color.org/): when the `NO_COLOR` env var is present
(any value), tools must not add ANSI color codes. This is the de-facto standard adopted by
rustc, ESLint, Biome, OxLint, and most modern CLI tools.

---

## Code References

- [`crates/libaipm/src/lint/diagnostic.rs:40-53`](https://github.com/TheLarkInn/aipm/blob/a664b021936c7e4e2fb18a751c5e3bcb666c26b1/crates/libaipm/src/lint/diagnostic.rs#L40-L53) — `Diagnostic` struct (no column field)
- [`crates/libaipm/src/lint/rule.rs:16-31`](https://github.com/TheLarkInn/aipm/blob/a664b021936c7e4e2fb18a751c5e3bcb666c26b1/crates/libaipm/src/lint/rule.rs#L16-L31) — `Rule` trait (no rangeable/help_url methods)
- [`crates/libaipm/src/lint/reporter.rs:8-52`](https://github.com/TheLarkInn/aipm/blob/a664b021936c7e4e2fb18a751c5e3bcb666c26b1/crates/libaipm/src/lint/reporter.rs#L8-L52) — `Reporter` trait, `Text` and `Json` implementations
- [`crates/libaipm/src/lint/rules/scan.rs:12-19`](https://github.com/TheLarkInn/aipm/blob/a664b021936c7e4e2fb18a751c5e3bcb666c26b1/crates/libaipm/src/lint/rules/scan.rs#L12-L19) — `FoundSkill` struct with `content: String` field
- [`crates/libaipm/src/frontmatter.rs`](https://github.com/TheLarkInn/aipm/blob/a664b021936c7e4e2fb18a751c5e3bcb666c26b1/crates/libaipm/src/frontmatter.rs) — `Frontmatter` with `start_line`/`end_line` but no per-field positions
- [`crates/libaipm/src/lint/rules/misplaced_features.rs:57-58`](https://github.com/TheLarkInn/aipm/blob/a664b021936c7e4e2fb18a751c5e3bcb666c26b1/crates/libaipm/src/lint/rules/misplaced_features.rs#L57-L58) — `line: None` (directory-level, not rangeable)
- [`crates/libaipm/src/lint/rules/hook_legacy_event.rs:58`](https://github.com/TheLarkInn/aipm/blob/a664b021936c7e4e2fb18a751c5e3bcb666c26b1/crates/libaipm/src/lint/rules/hook_legacy_event.rs#L58) — `line: None` (hook key, no JSON position tracking)
- [`crates/aipm/src/main.rs:549-556`](https://github.com/TheLarkInn/aipm/blob/a664b021936c7e4e2fb18a751c5e3bcb666c26b1/crates/aipm/src/main.rs#L549-L556) — CLI reporter dispatch (simple `match` on `--format` string)

---

## Architecture Documentation

### Current Reporter Architecture

```
Reporter trait
  ├── Text  (human-readable, no color, no snippets)
  └── Json  (machine-readable, minimal fields)

CLI: --format text|json → match → hardcoded impl
```

### What Issue #198 Requires

```
Reporter trait (unchanged interface, extensible)
  ├── Human  (colored, grouped, snippets, help text, rule links)
  ├── CI     (no color, GitHub Actions annotations ::error/::warning)
  ├── LLM    (structured JSON with rule_id, help_url, message, severity — agentic-first)
  ├── Json   (current, extended with column/help_url for LSP adapter compatibility)
  └── Web    (v2 — HTML output)

CLI: --format human|ci|llm|json|web → match → reporter factory
```

### Color Decision Tree (following ecosystem conventions)

```
is_atty(stdout)?  NO → no color
NO_COLOR set?     YES → no color
CLICOLOR=0?       YES → no color
--color=never?    YES → no color
--color=always?   YES → force color
otherwise         → color enabled
```

### Rangeable Rule Decision Tree

```
Rule emits line: None?
  YES → "repo-wide" / directory diagnostic → no snippet
Rule emits line: Some(n)?
  YES → file diagnostic → can show snippet
  Rule emits col: Some(c)?
    YES → can show precise underline
    NO  → show line context only (no underline)
```

---

## Historical Context (from research/)

- [`specs/2026-03-31-aipm-lint-developer-experience.md`](../specs/2026-03-31-aipm-lint-developer-experience.md) — Already shows `= help:` lines in the example output (Section 2) but these are not implemented in the current `Text` reporter. The spec anticipates rule links and help text.
- [`research/docs/2026-04-02-189-verbosity-levels-research.md`](../docs/2026-04-02-189-verbosity-levels-research.md) — Notes that the `lint/reporter.rs` `Reporter` trait is "a potential model for broader structured output." Recommends `tracing` for infrastructure logs, separate from lint diagnostic output.
- [`specs/2026-03-31-aipm-lint-command.md`](../../specs/2026-03-31-aipm-lint-command.md) — Section 3.2 lists "structured error guidance (machine-readable error codes ship, but documentation links and fix suggestions deferred)" as a non-goal for v1. Issue #198 promotes this to active work.
- [`research/tickets/2026-03-28-110-aipm-lint.md`](./2026-03-28-110-aipm-lint.md) — Original lint research. Mentions clippy-style output as the model.

---

## Related Research

- [`research/docs/2026-03-31-110-aipm-lint-architecture-research.md`](../docs/2026-03-31-110-aipm-lint-architecture-research.md) — Comprehensive architecture research for lint system
- [`research/docs/2026-04-02-aipm-lint-configuration-research.md`](../docs/2026-04-02-aipm-lint-configuration-research.md) — Configuration research (aipm.toml, rule overrides)
- [`research/docs/2026-04-02-189-verbosity-levels-research.md`](../docs/2026-04-02-189-verbosity-levels-research.md) — Verbosity levels and structured logging

---

## Open Questions

1. **Should color configuration go in `aipm.toml` or only in CLI flags / env vars?**
   No major lint tool puts color config in the project config file. Convention is `--color` flag + `NO_COLOR`.

2. **Should grouping be by file (ESLint style) or per-diagnostic (rustc style)?**
   ESLint-style grouping works well when many diagnostics share a file. Rustc-style works better
   when each diagnostic is self-contained with its own snippet. Given aipm's relatively low
   diagnostic density per file, either works — but rustc-style is simpler to implement.

3. **How should `source/misplaced-features` be displayed in a grouped model?**
   It produces directory-level diagnostics (no file, no line). A special section header like
   `workspace-wide checks:` or a separate block before file-specific diagnostics would handle this.

4. **What is the right `Diagnostic` struct evolution for column + range?**
   Adding `col: Option<usize>` and optionally `end_line: Option<usize>`, `end_col: Option<usize>`
   preserves backward compatibility. The `Json` reporter would need to include these fields.

5. **Should there be a new `--format ci` (GitHub Actions annotations) reporter in addition to the existing ones?**
   Biome, OxLint, and ESLint all do this. It is zero-dependency (plain text), small to implement,
   and immediately useful for monorepo GitHub Actions workflows.

6. **For the LLM/agentic reporter format, what structure does an AI agent need?**
   The verbosity-levels research (issue #189) discusses "agentic-first" logging. For lint,
   the agent needs: rule_id, severity, message, file_path, line, help_url (to look up docs),
   and a machine-readable summary. The current `Json` reporter provides most of this; adding
   `help_url` would make it fully useful to AI consumers.

7. **Does `Frontmatter` need per-field line tracking for snippet display to be useful?**
   For `skill/missing-name` and `skill/missing-description` (the field is *absent*), there is
   no line to point to — the snippet would show the frontmatter block with a note that the
   field is missing. For `skill/name-too-long` and `skill/invalid-shell` (field is *present*),
   pointing to the actual field line would be ideal but requires `Frontmatter` to track field
   line numbers.

---

## Appendix A: Detailed Per-Rule Rangeable Analysis

From deep analysis of all 12 rule implementations:

| Rule | `line` | Source text? | Column data? | Notes |
|------|--------|-------------|-------------|-------|
| `skill/missing-name` | `Some(1)` hardcoded | Yes (`content`) | None — field is absent | Frontmatter `start_line`/`end_line` usable for block range |
| `skill/missing-description` | `Some(1)` hardcoded | Yes (`content`) | None — field is absent | Same as above |
| `skill/oversized` | `Some(1)` hardcoded | Yes (`content`) | Not applicable | Whole-file issue |
| `skill/name-too-long` | `Some(1)` hardcoded | Yes (`content`) | Could search content for `name:` line | Not currently computed |
| `skill/name-invalid-chars` | `Some(1)` hardcoded | Yes (`content`) | Invalid char position computable | `is_valid_copilot_name()` returns bool, not position |
| `skill/description-too-long` | `Some(1)` hardcoded | Yes (`content`) | Could search content for `description:` line | Not currently computed |
| `skill/invalid-shell` | `Some(1)` hardcoded | Yes (`content`) | Could search content for `shell:` line | Not currently computed |
| `agent/missing-tools` | `Some(1)` hardcoded | **No** (`FoundAgent` has no `content`) | None — field absent | Unique: no source text available |
| `hook/unknown-event` | `Some(1)` on JSON error; `None` on unknown key | Yes (raw JSON) | `serde_json::Error` has `.line()`/`.column()` unused | JSON Value doesn't track key positions |
| `hook/legacy-event-name` | `None` always | Yes (raw JSON) | Key positions not tracked | Would need raw string search |
| `plugin/broken-paths` | **Computed** `Some(line_num + 1)` | Yes (iterates lines) | **`pos` and `end` computed** but NOT stored in Diagnostic | **Most advanced**: already has column data, just not exposed |
| `source/misplaced-features` | `None` always | **No** (directory check) | Not applicable | Not rangeable — directory-level finding |

**Key insight**: `broken_paths` is the only rule that (a) computes a real line number from
content iteration and (b) computes the byte offset `pos` of the match within the line and
the path endpoint `end`. This data is computed in local variables but never stored in
`Diagnostic`. Adding a `col` field to `Diagnostic` would immediately expose this for
`broken_paths` without any rule logic changes.

---

## Appendix B: Exact Rustc Color Assignments

From `rustc_errors/src/lib.rs` `Level::color()` method, using `anstyle`:

| Diagnostic Level | Color | ANSI Approximation |
|-----------------|-------|-------------------|
| `Error`, `Fatal`, `Bug` | Bright Red (bold) | `\x1b[1;91m` |
| `Warning` | Yellow bold (non-Windows) / Bright Yellow (Windows) | `\x1b[1;33m` |
| `Note` | Bright Green (bold) | `\x1b[1;92m` |
| `Help` | Bright Cyan (bold) | `\x1b[1;96m` |

| Structural Element | Color |
|-------------------|-------|
| Line numbers, `|` gutter | Bright Blue bold (or Bright Cyan on Windows) |
| Primary carets `^^^` | Same as diagnostic severity |
| Secondary dashes `---` | Bright Blue bold |
| Addition `+` suggestions | Bright Green |
| Removal `-` suggestions | Bright Red |

Coloring is **partial** — only labels (`error:`, `warning:`), codes, carets, and structural
elements are colored. Source code text and file paths use default terminal color.

Rustc uses `anstyle` (not termcolor directly) for color definitions. The `annotate-snippets`
crate (a **2024 H2 Rust Project Goal** to replace rustc's internal renderer) uses `anstyle`
and is available at `crates.io/crates/annotate-snippets`.

---

## Appendix C: LSP Integration Patterns

Three patterns exist for lint tool + LSP integration (no special reporter format required):

**Pattern A — Lint tool IS an LSP server** (e.g., Biome):
- Tool implements JSON-RPC LSP protocol directly
- Produces `Diagnostic` objects natively
- Not applicable for aipm (out of scope per existing spec)

**Pattern B — Wrapper/subprocess model** (e.g., rust-analyzer + clippy):
- A language server invokes `aipm lint --format json` as a subprocess
- Translates JSON output into LSP `Diagnostic` objects
- Requires: `column`, `end_line`, `end_col`, `help_url` in the JSON output

**Pattern C — Generic diagnostic adapter** (e.g., `diagnostic-languageserver`):
- A generic LSP server wraps any CLI tool via regex pattern matching on text output
- Zero changes to `aipm lint` required — just configure the adapter's regex
- The `::` LSP diagnostic format (from the `Text` reporter) is easy to regex-parse

LSP `Diagnostic` fields mapping:
- `range.start.line` ← `Diagnostic.line - 1` (0-based conversion needed)
- `range.start.character` ← `Diagnostic.col` (to be added)
- `severity` ← 1 (Error) or 2 (Warning)
- `code` ← `Diagnostic.rule_id`
- `codeDescription.href` ← `Rule.help_url()` (to be added to Rule trait)
- `source` ← `"aipm"`
- `message` ← `Diagnostic.message`

---

## Appendix D: ESLint / OxLint / Biome Comparison

### Code Snippet Rendering Comparison

**OxLint** (miette-based, box-drawing characters):
```
x eslint(no-debugger): `debugger` statement is not allowed
  ╭─[test.js:5:1]
4 │
5 │ debugger;
  · ─────────
6 │
  ╰────
help: Remove the debugger statement
```

**Biome** (Rust-native, rustc-style carets):
```
test.js:6:1 lint/suspicious/noDebugger ━━━━━━━━━━━━━━━━━━━━
  x This is an unexpected use of the debugger statement.

  5 │
  6 │ debugger;
    │ ^^^^^^^^^
  7 │

  i Suggested fix: Remove the debugger statement.
```

**ESLint default (stylish)**: No code snippets at all — only `line:col  severity  message  rule-id`.

### Grouping Model Comparison

| Tool | Grouping | Snippet | GitHub CI format |
|------|----------|---------|-----------------|
| ESLint (stylish) | By file | No | Via SARIF upload only |
| OxLint (default) | Per-diagnostic | Yes | `--format github` or auto-detected via `GITHUB_ACTIONS` |
| Biome | Per-diagnostic | Yes | `--reporter github` or auto-detected on CI |

**Key CI finding**: Both OxLint and Biome **auto-detect** the `GITHUB_ACTIONS` environment variable
and switch to the GitHub Actions annotation format automatically. This means `aipm lint` could
also auto-switch to `::error` format when `GITHUB_ACTIONS=true` without requiring `--format ci`.

### JSON Output Severity Field

| Tool | JSON severity field type | Example |
|------|--------------------------|---------|
| ESLint | Numeric: `1` = warning, `2` = error | `"severity": 2` |
| OxLint | String | `"severity": "error"` |
| Biome | String-based in category | `"lint/suspicious/noDebugger"` |
| aipm (current) | String (`"error"`, `"warning"`) | `"severity": "error"` |

aipm's current string format matches OxLint's approach. LSP uses numeric (1 for Error, 2 for Warning).

### Rule Documentation URL in JSON

OxLint's JSON output includes a `url` field per diagnostic pointing to rule documentation. This
is the pattern to follow for aipm's JSON reporter `help_url` field:
```json
{
  "message": "`debugger` statement is not allowed",
  "code": "eslint(no-debugger)",
  "severity": "error",
  "url": "https://oxc.rs/docs/guide/usage/linter/rules/eslint/no-debugger.html"
}
```
