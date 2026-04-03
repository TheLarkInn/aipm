---
date: 2026-04-02 23:34:16 UTC
researcher: Claude
git_commit: 4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56
branch: main
repository: aipm
topic: "aipm lint command: how it works, how to configure it, severity overrides"
tags: [research, codebase, lint, configuration, aipm-toml]
status: complete
last_updated: 2026-04-02
last_updated_by: Claude
---

# Research: `aipm lint` — Implementation and Configuration

## Research Question

Flesh out the README to explain `aipm lint`, how to configure it. Make these based on the code and not the spec/research. The user is struggling to upgrade a lint from a warning to error via the config:

```toml
[workspace.lints]
"source/misplaced-features" = "error"
```

## Summary

The `aipm lint` command is fully implemented with 13 lint rules across 3 source types, a TOML-based configuration system in `aipm.toml` under `[workspace.lints]`, and both text and JSON output formats. The configuration system supports three actions per rule: suppressing with `"allow"`, changing severity with `"error"`/`"warn"`, and detailed overrides with per-rule ignore paths. The user's config format `"source/misplaced-features" = "error"` **is correct** and should work — see the [Troubleshooting](#troubleshooting) section below for what to check.

## Detailed Findings

### CLI Interface

**Source:** [`crates/aipm/src/main.rs:113-130`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm/src/main.rs#L113-L130)

```
aipm lint [OPTIONS] [DIR]
```

| Flag | Description |
|------|-------------|
| `[DIR]` | Project directory (default: `.`) |
| `--source <SRC>` | Filter to a specific source type: `.claude`, `.github`, `.ai` |
| `--format <FMT>` | Output format: `text` (default) or `json` |
| `--max-depth <N>` | Maximum directory traversal depth for recursive discovery |

**Exit codes:**
- `0` — no errors (warnings are OK)
- `1` — one or more error-severity diagnostics found

### Source Types and Discovery

The linter scans three source types:

| Source | Discovery | What it checks |
|--------|-----------|----------------|
| `.claude` | **Recursive** — walks the entire project tree (gitignore-aware) | Misplaced features (skills/agents/hooks in `.claude/` instead of `.ai/`) |
| `.github` | **Recursive** — same tree walk | Misplaced features in `.github/` |
| `.ai` | **Flat** — only checks root `.ai/` directory | All marketplace plugin quality rules (13 rules) |

Without `--source`, all three are scanned (`.ai` only if it exists).

**Source:** [`crates/libaipm/src/lint/mod.rs:78-128`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/lint/mod.rs#L78-L128)

### All Lint Rules

**Source:** [`crates/libaipm/src/lint/rules/mod.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/lint/rules/mod.rs)

#### `.claude` / `.github` rules

| Rule ID | Default | Description |
|---------|---------|-------------|
| `source/misplaced-features` | warning | Plugin features (skills/, agents/, hooks/, etc.) found in source dir instead of `.ai/` marketplace. Only fires when `.ai/` exists. |

#### `.ai` marketplace rules

| Rule ID | Default | Description |
|---------|---------|-------------|
| `skill/missing-name` | warning | SKILL.md missing `name` in frontmatter |
| `skill/missing-description` | warning | SKILL.md missing `description` in frontmatter |
| `skill/oversized` | warning | SKILL.md exceeds 15,000 characters |
| `skill/name-too-long` | warning | Skill name exceeds 64 characters |
| `skill/name-invalid-chars` | warning | Skill name contains invalid characters |
| `skill/description-too-long` | warning | Skill description exceeds limit |
| `skill/invalid-shell` | **error** | Invalid shell specified (only `bash` allowed) |
| `agent/missing-tools` | warning | Agent definition missing tools section |
| `hook/unknown-event` | **error** | Hook references an unrecognized event name |
| `hook/legacy-event-name` | warning | Hook uses legacy event name (suggests modern equivalent) |
| `plugin/broken-paths` | **error** | Component paths in manifest reference nonexistent files |

### Configuration via `aipm.toml`

**Source:** [`crates/aipm/src/main.rs:552-620`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm/src/main.rs#L552-L620)

Configuration lives in `[workspace.lints]` inside `aipm.toml` at the project root. The config parser uses `toml::Value` (not the `Manifest` struct), so it works independently of whether you have a full workspace manifest.

#### Suppress a rule entirely

```toml
[workspace.lints]
"skill/missing-description" = "allow"
```

#### Change severity (warning to error, or error to warning)

```toml
[workspace.lints]
"source/misplaced-features" = "error"    # upgrade warning -> error
"skill/invalid-shell" = "warn"           # downgrade error -> warning
```

Accepted severity strings (from [`crates/libaipm/src/lint/diagnostic.rs:29-35`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/lint/diagnostic.rs#L29-L35)):
- `"error"` or `"deny"` → Error severity (causes exit code 1)
- `"warn"` or `"warning"` → Warning severity (exit code 0)
- `"allow"` → Suppresses the rule entirely

#### Detailed override with per-rule ignore paths

```toml
[workspace.lints."plugin/broken-paths"]
level = "error"
ignore = ["examples/**", "vendor/**"]
```

#### Global ignore paths

```toml
[workspace.lints.ignore]
paths = ["**/legacy-plugin/**", "vendor/**"]
```

### How Severity Overrides Work

**Source:** [`crates/libaipm/src/lint/mod.rs:44-61`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/lint/mod.rs#L44-L61)

1. Each rule produces diagnostics with its `default_severity()`
2. The engine checks `config.severity_override(rule_id)` for a user-specified level
3. If found, the diagnostic's severity is overwritten: `d.severity = effective_severity`
4. This happens AFTER the rule runs and BEFORE the diagnostics are collected

### Output Formats

**Text output** (default) — rustc/clippy style:

```
warning[source/misplaced-features]: skills/ found in .claude instead of .ai/ marketplace
  --> .claude/skills
  |
warning: 1 warning(s) emitted
```

**JSON output** (`--format json`):

```json
{
  "diagnostics": [
    {
      "rule_id": "source/misplaced-features",
      "severity": "error",
      "message": "skills/ found in .claude instead of .ai/ marketplace",
      "file_path": ".claude/skills",
      "line": null,
      "source_type": ".claude"
    }
  ],
  "summary": {
    "errors": 1,
    "warnings": 0,
    "sources_scanned": [".claude", ".ai"]
  }
}
```

**Source:** [`crates/libaipm/src/lint/reporter.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/lint/reporter.rs)

### Troubleshooting

The user's config:
```toml
[workspace.lints]
"source/misplaced-features" = "error"
```

**This format is correct.** The E2E test at [`crates/aipm/tests/lint_e2e.rs:350-370`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm/tests/lint_e2e.rs#L350-L370) confirms this exact pattern works.

Things to check if it's not working:

1. **`aipm.toml` location** — Must be in the same directory you pass to `aipm lint` (or `.` if omitted). The loader looks for `dir.join("aipm.toml")` ([main.rs:553](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm/src/main.rs#L553)).

2. **Silent fallback on parse errors** — If `aipm.toml` has a TOML syntax error, `load_lint_config` silently returns default config (no overrides). There's no error message. ([main.rs:554-558](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm/src/main.rs#L554-L558))

3. **Rule must fire first** — `source/misplaced-features` only produces diagnostics when:
   - `.ai/` directory exists at project root
   - AND a `.claude/` or `.github/` directory contains `skills/`, `agents/`, `hooks/`, `commands/`, `output-styles/`, or `extensions/`

4. **Severity override ≠ triggering** — The config only changes severity of findings that already exist. It cannot make a rule fire when it wouldn't normally.

5. **The `Workspace` struct doesn't have a `lints` field** — This is fine for `aipm lint` (uses `toml::Value` parser), but if other commands that parse via the `Manifest` struct are called, `lints` is silently ignored (no error, since `Workspace` doesn't use `deny_unknown_fields`).

## Code References

- `crates/aipm/src/main.rs:498-549` — `cmd_lint()` CLI handler
- `crates/aipm/src/main.rs:552-620` — `load_lint_config()` TOML parser
- `crates/libaipm/src/lint/mod.rs` — Lint engine (discovery, rule execution, severity override)
- `crates/libaipm/src/lint/config.rs` — `Config` and `RuleOverride` types
- `crates/libaipm/src/lint/diagnostic.rs` — `Severity` enum and `from_str_config()`
- `crates/libaipm/src/lint/rule.rs` — `Rule` trait
- `crates/libaipm/src/lint/rules/mod.rs` — Rule registry and factory functions
- `crates/libaipm/src/lint/rules/misplaced_features.rs` — The specific rule in question
- `crates/libaipm/src/lint/reporter.rs` — Text and JSON output formatters
- `crates/aipm/tests/lint_e2e.rs:324-396` — E2E tests for config overrides

## Architecture Documentation

### Config Loading Flow
```
aipm lint [DIR]
  → load_lint_config(dir)
    → reads dir/aipm.toml as string
    → parses as toml::Value (NOT Manifest struct)
    → navigates to workspace.lints table
    → for each key/value:
        string "allow" → RuleOverride::Allow
        string "error"/"deny" → RuleOverride::Level(Error)
        string "warn"/"warning" → RuleOverride::Level(Warning)
        table { level, ignore } → RuleOverride::Detailed { level, ignore }
    → special key "ignore" → global ignore paths
  → lint::lint(opts)
    → discovers source dirs recursively (.claude/.github)
    → checks .ai/ marketplace (flat)
    → for each rule:
        skip if suppressed (allow)
        run rule → diagnostics
        apply severity override
        filter by ignore paths
    → sort diagnostics by file path
    → count errors/warnings
  → report (text or JSON)
  → exit 1 if error_count > 0
```

### Key Design Decisions
- Config parser is decoupled from manifest parser (uses `toml::Value`, not `Manifest` struct)
- Parse errors in `aipm.toml` silently fall back to defaults (no error surfaced)
- Rules produce diagnostics with default severity; override happens in the engine, not the rule
- `"allow"` is handled separately from severity levels (returns `None` from `severity_override`)

## Related Research
- `research/tickets/2026-03-28-110-aipm-lint.md` — Original lint feature ticket
- `research/docs/2026-03-31-110-aipm-lint-architecture-research.md` — Architecture research

## Open Questions

1. **Silent config parse failures** — Should `load_lint_config` warn when `aipm.toml` exists but fails to parse? Currently it silently falls back to defaults, which could confuse users who think their config is being applied.

2. **No config validation** — If a user types `"source/misplaced-feature"` (missing `s`), there's no warning that the rule ID doesn't match any known rule. The override is silently ignored.
