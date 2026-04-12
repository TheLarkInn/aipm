---
date: 2026-04-11 21:17:18 UTC
researcher: Claude Code (Opus 4.6)
git_commit: 796ace8014cd381324b954c5ac7b5883b9cdf394
branch: main
repository: aipm
topic: "[dogfood] enable 'aipm lint' in this repo (Issue #426)"
tags: [research, lint, dogfood, aipm-toml, fixtures, ci-cd, vscode]
status: complete
last_updated: 2026-04-11
last_updated_by: Claude Code (Opus 4.6)
---

# Research: Dogfood `aipm lint` in This Repo (Issue #426)

## Research Question

How does `aipm lint` currently work, what does an `aipm.toml` manifest look like, and what changes are needed to dogfood linting in this repo — including fixture exclusion, CI integration, and VSCode plugin support?

Issue checklist from [#426](https://github.com/TheLarkInn/aipm/issues/426):

- [ ] Ensure repo AI integrations are up to standard
- [ ] Protect plugin changes in CI/CD
- [ ] Dogfood the VSCode plugin locally (requires root `aipm.toml` with lint settings only)
- [ ] Lint rules should ignore `./fixtures/` (used for e2e / functional tests)
- [ ] If `aipm lint` can't support ignoring fixtures, it's a bug

## Summary

The `aipm lint` system is fully implemented with 18 rules, 4 output reporters, glob-based ignore patterns (global and per-rule), and config via `[workspace.lints]` in `aipm.toml`. The repo currently has **no root `aipm.toml`** and **`aipm lint` is not in CI**. Running `aipm lint .` today produces **40 diagnostics (18 errors, 22 warnings)** — of which **35 come from `fixtures/`** and only **5 from real code** (1 error, 4 warnings). The existing `[workspace.lints.ignore].paths` mechanism with `**/fixtures/**` glob patterns can exclude fixtures. The VSCode extension activates on the presence of `aipm.toml` and launches `aipm lsp` for inline diagnostics.

## Detailed Findings

### 1. Current Lint Output (Baseline)

Running `aipm lint .` at the repo root at commit `796ace8` produces:

| Source | Errors | Warnings | Total |
|--------|--------|----------|-------|
| `fixtures/` | 17 | 18 | 35 |
| Real code (`.ai/`, root) | 1 | 4 | 5 |
| **Total** | **18** | **22** | **40** |

**Real code diagnostics (5):**

| Severity | Rule | File |
|----------|------|------|
| warning | `skill/oversized` | [`.ai/aipm-atomic-plugin/skills/perf-anti-patterns/SKILL.md`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/.ai/aipm-atomic-plugin/skills/perf-anti-patterns/SKILL.md) |
| error | `plugin/required-fields` | [`.ai/starter-aipm-plugin/.claude-plugin/plugin.json`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/.ai/starter-aipm-plugin/.claude-plugin/plugin.json) |
| warning | `skill/missing-name` | [`.ai/starter-aipm-plugin/skills/scaffold-plugin/SKILL.md`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/.ai/starter-aipm-plugin/skills/scaffold-plugin/SKILL.md) |
| warning | `source/misplaced-features` | [`CLAUDE.md`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/CLAUDE.md) |
| warning | `instructions/oversized` | [`CLAUDE.md`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/CLAUDE.md) |

### 2. `aipm lint` Architecture

The lint pipeline is implemented across three crates:

- **CLI entry**: [`crates/aipm/src/main.rs:666-746`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/crates/aipm/src/main.rs#L666-L746) — `cmd_lint()` orchestrates config loading, lint execution, and reporting.
- **Core pipeline**: [`crates/libaipm/src/lint/mod.rs:115-162`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/crates/libaipm/src/lint/mod.rs#L115-L162) — `lint()` performs discovery, rule dispatch, and diagnostic collection.
- **Feature discovery**: [`crates/libaipm/src/discovery.rs:299-369`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/crates/libaipm/src/discovery.rs#L299-L369) — `discover_features()` uses the `ignore` crate's `WalkBuilder` for a single gitignore-aware recursive walk.

**Data flow:**
1. CLI parses args and validates `--source`, `--reporter`, `--color`
2. `load_lint_config(&dir)` reads `aipm.toml` and parses `[workspace.lints]` ([`main.rs:748-845`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/crates/aipm/src/main.rs#L748-L845))
3. `discover_features()` walks the directory tree, classifying files by name/parent into `FeatureKind` variants (Skill, Agent, Hook, Plugin, Marketplace, PluginJson, Instructions)
4. `run_rules_for_feature()` dispatches kind-appropriate rules from the registry
5. `apply_rule_diagnostics()` filters results through global and per-rule ignore patterns
6. Reporter formats output; exit code is `FAILURE` if `error_count > 0`

**Hardcoded skip directories** ([`discovery.rs:178-179`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/crates/libaipm/src/discovery.rs#L178-L179)):
`node_modules`, `target`, `.git`, `vendor`, `__pycache__`, `dist`, `build` — **`fixtures` is not in this list**.

### 3. The 18 Lint Rules

| Rule ID | Default Severity | Applies To |
|---------|-----------------|------------|
| `skill/missing-name` | warning | Skill |
| `skill/missing-description` | warning | Skill |
| `skill/oversized` | warning | Skill |
| `skill/name-too-long` | warning | Skill |
| `skill/name-invalid-chars` | warning | Skill |
| `skill/description-too-long` | warning | Skill |
| `skill/invalid-shell` | error | Skill |
| `plugin/broken-paths` | error | Skill, Plugin |
| `agent/missing-tools` | warning | Agent |
| `hook/unknown-event` | error | Hook |
| `hook/legacy-event-name` | warning | Hook |
| `marketplace/source-resolve` | error | Marketplace |
| `marketplace/plugin-field-mismatch` | error | Marketplace |
| `plugin/missing-registration` | error | Marketplace |
| `plugin/missing-manifest` | error | Marketplace |
| `plugin/required-fields` | error | PluginJson |
| `instructions/oversized` | warning | Instructions |
| `source/misplaced-features` | warning | (any outside `.ai/`) |

Rule implementations live in [`crates/libaipm/src/lint/rules/`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/crates/libaipm/src/lint/rules/).

### 4. Ignore Pattern Support (Fixture Exclusion)

**The system already supports fixture exclusion.** Two ignore mechanisms exist:

#### Global ignore paths (`[workspace.lints.ignore].paths`)

Defined in [`lint/config.rs:9-14`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/crates/libaipm/src/lint/config.rs#L9-L14). Parsed at [`main.rs:790-798`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/crates/aipm/src/main.rs#L790-L798). Applied to all rules via `is_ignored()` at [`lint/mod.rs:23-41`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/crates/libaipm/src/lint/mod.rs#L23-L41).

```toml
[workspace.lints.ignore]
paths = ["**/fixtures/**"]
```

#### Per-rule ignore paths (`[workspace.lints."rule-id"].ignore`)

```toml
[workspace.lints."plugin/broken-paths"]
ignore = ["**/fixtures/**"]
```

#### How matching works

`is_ignored()` uses `glob::Pattern::matches()` against the **full absolute path** string of the diagnostic's `file_path`. Patterns must use `**/` prefixes to match at any depth. A pattern like `fixtures/**` would **NOT** match `/workspaces/aipm/fixtures/...` — it must be `**/fixtures/**`.

#### Where filtering happens

Ignore patterns are applied **post-discovery, post-rule-execution** in `apply_rule_diagnostics()` ([`lint/mod.rs:45-65`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/crates/libaipm/src/lint/mod.rs#L45-L65)). Rules still run on fixture files — only the resulting diagnostics are suppressed. This is sufficient for correctness but does unnecessary work.

**Verdict: `aipm lint` CAN support ignoring fixtures via `[workspace.lints.ignore].paths`. This is not a bug — it just needs an `aipm.toml` configured with the right glob.**

### 5. `aipm.toml` Manifest Format

The manifest struct is defined at [`crates/libaipm/src/manifest/types.rs:10-42`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/crates/libaipm/src/manifest/types.rs#L10-L42). Key sections:

| Section | Purpose |
|---------|---------|
| `[package]` | Plugin metadata (name, version, type, description, engines) |
| `[workspace]` | Workspace config (members, plugins_dir) |
| `[workspace.lints]` | Lint rule configuration (parsed separately via `toml::Value`) |
| `[workspace.lints.ignore]` | Global ignore paths |
| `[dependencies]` | Direct dependencies |
| `[components]` | Component file declarations |
| `[environment]` | Environment requirements |
| `[install]` | Installation behavior |

For dogfooding, only `[workspace.lints]` (and optionally a minimal `[workspace]`) is needed. The `Workspace` struct does **not** use `#[serde(deny_unknown_fields)]` ([`types.rs:88-99`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/crates/libaipm/src/manifest/types.rs#L88-L99)), which allows the `lints` key to pass through typed deserialization while being parsed separately.

### 6. Minimal `aipm.toml` for Dogfooding

Based on the 5 real diagnostics and the issue requirements, a suitable root `aipm.toml` would contain only lint settings:

```toml
[workspace.lints.ignore]
paths = ["**/fixtures/**"]
```

This would:
- Exclude all 35 fixture diagnostics
- Leave the 5 real-code diagnostics visible for fixing
- Activate the VSCode extension (which triggers on `workspaceContains:**/aipm.toml`)
- Serve as input for `aipm lint --reporter ci-github` in CI

Additional overrides could suppress or adjust specific rules for the real diagnostics, depending on what the team considers acceptable (e.g., allowing `source/misplaced-features` for `CLAUDE.md` since it's a project-level instruction file, not a plugin feature).

### 7. CI/CD Integration

**Current CI** ([`.github/workflows/ci.yml`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/.github/workflows/ci.yml)):
- Job `ci`: `cargo build`, `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`
- Job `coverage`: nightly branch coverage gate at 89%
- **No `aipm lint` step exists.**

The lint guide ([`docs/guides/lint.md:144-149`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/docs/guides/lint.md#L144-L149)) documents the recommended CI pattern:

```yaml
- name: Lint AI plugins
  run: aipm lint --reporter ci-github
```

The `ci-github` reporter emits `::error` / `::warning` workflow commands that create inline PR annotations. The `aipm` binary would need to be built or installed in CI before the lint step runs.

### 8. VSCode Extension

The [`vscode-aipm/`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/vscode-aipm/) extension:

- **Activates on** `workspaceContains:**/aipm.toml` ([`package.json:11`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/vscode-aipm/package.json#L11))
- **Launches** `aipm lsp` as a stdio language server ([`extension.ts:18-21`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/vscode-aipm/src/extension.ts#L18-L21))
- **Settings**: `aipm.lint.enable` (default `true`), `aipm.path` (default `"aipm"`)
- **Linted file types**: `aipm.toml`, `SKILL.md`, agent `.md`, `hooks.json`, `plugin.json`, `marketplace.json`
- **Schema validation**: TOML schema at [`schemas/aipm.toml.schema.json`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/schemas/aipm.toml.schema.json) provides IDE-level validation for `[workspace.lints]` via the Taplo TOML extension

Adding a root `aipm.toml` is the only prerequisite for the VSCode extension to activate and provide inline lint diagnostics.

### 9. Existing Lintable Content in `.ai/`

The repo has a live `.ai/` marketplace ([`.ai/.claude-plugin/marketplace.json`](https://github.com/TheLarkInn/aipm/blob/796ace8014cd381324b954c5ac7b5883b9cdf394/.ai/.claude-plugin/marketplace.json)) with two plugins:

| Plugin | Contents |
|--------|----------|
| `aipm-atomic-plugin` | 7 agents, 3 skills (perf-anti-patterns, prompt-engineer, testing-anti-patterns), 7 commands |
| `starter-aipm-plugin` | 1 agent, 1 skill (scaffold-plugin), 1 hooks.json, 1 MCP config, 1 script |

These are already being linted and produce the 5 real diagnostics listed above.

## Code References

- `crates/aipm/src/main.rs:158-182` — `Lint` CLI subcommand definition (clap derive)
- `crates/aipm/src/main.rs:666-746` — `cmd_lint()` handler
- `crates/aipm/src/main.rs:748-845` — `load_lint_config()` parses `[workspace.lints]`
- `crates/libaipm/src/lint/mod.rs:23-41` — `is_ignored()` glob matching
- `crates/libaipm/src/lint/mod.rs:45-65` — `apply_rule_diagnostics()` with ignore filtering
- `crates/libaipm/src/lint/mod.rs:68-104` — `run_rules_for_feature()` rule dispatch
- `crates/libaipm/src/lint/mod.rs:115-162` — `lint()` main pipeline
- `crates/libaipm/src/lint/config.rs:8-72` — `Config`, `RuleOverride` types
- `crates/libaipm/src/lint/rules/mod.rs:41-87` — `quality_rules_for_kind()` registry
- `crates/libaipm/src/discovery.rs:178-179` — `SKIP_DIRS` constant
- `crates/libaipm/src/discovery.rs:243-297` — `classify_feature_kind()`
- `crates/libaipm/src/discovery.rs:299-369` — `discover_features()` walk
- `crates/libaipm/src/manifest/types.rs:10-42` — `Manifest` struct
- `crates/libaipm/src/manifest/types.rs:88-99` — `Workspace` struct (no `deny_unknown_fields`)
- `.github/workflows/ci.yml` — current CI pipeline (no `aipm lint`)
- `vscode-aipm/package.json:11` — extension activation event
- `vscode-aipm/src/extension.ts:12-55` — extension entry point
- `schemas/aipm.toml.schema.json` — JSON Schema for lint config
- `docs/guides/lint.md` — user-facing lint documentation

## Architecture Documentation

### Lint Pipeline Flow

```
CLI (main.rs)
  |
  ├─ load_lint_config() ─── reads aipm.toml ─── parses [workspace.lints]
  |                                               ├─ ignore.paths → Config.ignore_paths
  |                                               └─ rule overrides → Config.rule_overrides
  |
  └─ lint::lint(opts, fs)
       |
       ├─ discover_features(dir, max_depth)
       |    └─ ignore::WalkBuilder (gitignore-aware, SKIP_DIRS filter)
       |         └─ classify_feature_kind() for each file
       |
       ├─ Optional --source filter
       |
       ├─ For each feature:
       |    └─ run_rules_for_feature()
       |         ├─ quality_rules_for_kind() → applicable rules
       |         ├─ Skip suppressed rules (config.is_suppressed)
       |         ├─ rule.check_file(path, fs) → Vec<Diagnostic>
       |         └─ apply_rule_diagnostics()
       |              ├─ Effective severity (config override or default)
       |              ├─ is_ignored() check (global + per-rule)
       |              └─ Attach help text/URL
       |
       └─ Sort, count, return Outcome
            └─ Reporter formats to stdout
                 └─ Exit code: FAILURE if error_count > 0
```

### Config File Discovery

- **Lint config**: `load_lint_config()` looks for `aipm.toml` in the **given directory only** (no upward walk). Returns `Config::default()` (empty) if file is missing or unparseable.
- **Workspace root**: `find_workspace_root()` walks **upward** from CWD, looking for `aipm.toml` with a `[workspace]` section.
- **VSCode extension**: Activates when any `aipm.toml` exists anywhere in the workspace (`workspaceContains:**/aipm.toml`).

### Ignore Pattern Semantics

- Patterns use `glob::Pattern` (not gitignore-style)
- Matching is against the **full absolute path** string of the diagnostic's `file_path`
- `**/fixtures/**` matches any path containing a `fixtures/` component at any depth
- Patterns like `fixtures/**` (no `**/` prefix) will NOT match absolute paths
- Invalid patterns are silently skipped (logged at `tracing::warn` level)
- Filtering happens post-rule-execution — rules still run, only diagnostics are suppressed

## Historical Context (from research/)

- `research/tickets/2026-03-28-110-aipm-lint.md` — Original `aipm lint` command research (Issue #110)
- `research/docs/2026-03-31-110-aipm-lint-architecture-research.md` — Comprehensive lint architecture research
- `research/docs/2026-04-02-aipm-lint-configuration-research.md` — Lint configuration and rule override research
- `research/docs/2026-04-10-377-vscode-support-aipm-lint.md` — VSCode extension integration research (Issue #377)
- `research/docs/2026-04-07-lint-rules-287-288-289-290.md` — Marketplace/plugin rule implementations
- `research/tickets/2026-04-11-185-prevent-long-instructions-files.md` — Instructions oversized rule research

No prior research exists for "dogfooding" specifically.

## Related Research

- [`research/docs/2026-03-09-manifest-format-comparison.md`](../docs/2026-03-09-manifest-format-comparison.md) — Why TOML was chosen for manifests
- [`research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md`](../docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md) — How `aipm init` generates manifests
- [`research/docs/2026-03-16-rust-cross-platform-release-distribution.md`](../docs/2026-03-16-rust-cross-platform-release-distribution.md) — CI/CD binary distribution
- [`research/docs/2026-04-10-vscode-extension-launch-debug.md`](../docs/2026-04-10-vscode-extension-launch-debug.md) — VSCode extension debugging

## Open Questions

1. **Should `CLAUDE.md` warnings be suppressed?** The `source/misplaced-features` and `instructions/oversized` rules fire on `CLAUDE.md`. This is a project-level instruction file, not a plugin feature — suppressing `source/misplaced-features` for `CLAUDE.md` via ignore pattern or `"allow"` may be appropriate.
2. **Should the `plugin/required-fields` error on `starter-aipm-plugin` be fixed?** The `plugin.json` is missing an `author` field. This is a real issue in the live plugin.
3. **Should the `skill/oversized` warning on `perf-anti-patterns` be addressed?** The skill is 20,036 chars vs the 15,000 limit. It could be allowed, split, or the threshold could be overridden.
4. **CI binary availability**: The CI lint step needs `aipm` built before it can run. The simplest approach is to add the lint step after `cargo build --workspace` and use `cargo run --bin aipm --` or `target/debug/aipm`.
5. **Pre-discovery exclusion**: Currently fixtures are still walked and rules still execute — only diagnostics are suppressed. For large fixture directories this is wasted work, but for this repo's fixture count it's negligible.
