# `aipm lint` — Developer Experience Guide

| Document Metadata      | Details                          |
| ---------------------- | -------------------------------- |
| Author(s)              | Sean Larkin                      |
| Status                 | In Review (RFC)                  |
| Team / Owner           | AIPM Core                        |
| Created / Last Updated | 2026-03-31 / 2026-03-31         |
| Related                | [GitHub Issue #110](https://github.com/TheLarkInn/aipm/issues/110), [Technical Spec](./2026-03-31-aipm-lint-command.md) |

---

## What is `aipm lint`?

A built-in linting command that validates AI plugin configurations across your repository. It checks `.claude/`, `.github/`, and `.ai/` directories for common quality issues — missing frontmatter fields, broken file references, invalid hook events, and misplaced plugin features.

Think of it as **clippy/eslint for AI plugins**.

---

## Getting Started

### Zero-config usage

`aipm lint` works out of the box with no configuration. Run it in any aipm-initialized project:

```bash
$ aipm lint
```

```
warning[skill/missing-description]: SKILL.md missing required field: description
  --> .ai/my-plugin/skills/default/SKILL.md:1
  |
  = help: add a "description" field to the YAML frontmatter

error[hook/unknown-event]: unknown hook event: InvalidEvent
  --> .ai/my-plugin/hooks/hooks.json:5
  |
  = help: valid events: PreToolUse, PostToolUse, Notification, Stop, ...

error: 1 error, 1 warning emitted
```

Exit code `1` if any errors are found. Warnings alone exit `0`.

### What gets scanned

By default, `aipm lint` auto-discovers and scans all known source directories in your project:

| Directory | What's checked |
|-----------|---------------|
| `.claude/` | Plugin features that should be in `.ai/` marketplace instead |
| `.github/` | Plugin features that should be in `.ai/` marketplace instead |
| `.ai/` | Plugin quality: frontmatter, broken paths, hook events, skill size |

---

## The Rules

Twelve rules ship out of the box. All are enabled by default. Rule definitions are informed by binary analysis of Claude Code CLI v2.1.87 and Copilot CLI v1.0.12.

### Source rules

These check your tool-specific directories for plugin features that belong in the marketplace.

| Rule | Default | What it catches |
|------|---------|-----------------|
| `source/misplaced-features` | warning | Skills, agents, hooks, or other plugin features sitting in `.claude/` or `.github/` instead of `.ai/` |

```
warning[source/misplaced-features]: skill found in .claude/ instead of .ai/ marketplace
  --> .claude/skills/code-review/SKILL.md
  |
  = help: run "aipm migrate" to move this into the marketplace
```

### Marketplace rules

These validate plugin quality inside your `.ai/` directory.

| Rule | Default | What it catches |
|------|---------|-----------------|
| `skill/missing-name` | warning | SKILL.md has no `name` field in frontmatter |
| `skill/missing-description` | warning | SKILL.md has no `description` field in frontmatter |
| `skill/oversized` | warning | SKILL.md exceeds 15,000 characters |
| `agent/missing-tools` | warning | Agent definition has no `tools` in frontmatter |
| `hook/unknown-event` | error | Hook config references an event not valid for the target tool |
| `plugin/broken-paths` | error | Script or file references inside a plugin point to files that don't exist |

### Cross-tool compatibility rules

These catch issues that work in one AI tool but fail in another. Derived from analyzing both CLI binaries.

| Rule | Default | What it catches |
|------|---------|-----------------|
| `skill/name-too-long` | warning | Skill name exceeds 64 characters (fails in Copilot CLI) |
| `skill/name-invalid-chars` | warning | Skill name uses characters Copilot CLI rejects |
| `skill/description-too-long` | warning | Description exceeds 1024 characters (truncated in Copilot CLI) |
| `skill/invalid-shell` | error | `shell` field isn't `bash` or `powershell` (Claude Code silently falls back) |
| `hook/legacy-event-name` | warning | PascalCase hook event that Copilot normalizes to camelCase |

```
warning[skill/name-too-long]: skill name exceeds 64 characters (Copilot CLI limit)
  --> .ai/my-plugin/skills/my-very-long-named-skill-that-exceeds-the-limit/SKILL.md:2
  |
  = help: shorten the name to 64 characters or fewer for cross-tool compatibility

warning[hook/legacy-event-name]: "Stop" is a legacy event name, use "agentStop" instead
  --> .ai/my-plugin/hooks/hooks.json:3
  |
  = help: Copilot CLI normalizes "Stop" to "agentStop"

error[skill/invalid-shell]: invalid shell value "zsh", must be "bash" or "powershell"
  --> .ai/my-plugin/skills/default/SKILL.md:4
  |
  = help: Claude Code only supports "bash" and "powershell" for the shell field
```

```
error[plugin/broken-paths]: broken script reference: ${CLAUDE_SKILL_DIR}/scripts/deploy.sh
  --> .ai/my-plugin/skills/deploy/SKILL.md:12
  |
  = help: file not found: .ai/my-plugin/skills/deploy/scripts/deploy.sh
```

Hook validation is **tool-aware** — Claude Code has 26 valid events and Copilot CLI has 10. The rule knows which tool's event list to check against:

```
error[hook/unknown-event]: unknown hook event: InvalidEvent
  --> .ai/my-plugin/hooks/hooks.json:5
  |
  = help: valid Claude Code events: PreToolUse, PostToolUse, PostToolUseFailure,
          SessionStart, Stop, StopFailure, SubagentStart, SubagentStop, ...
```

---

## Configuration

Configuration is optional and lives in your workspace `aipm.toml` under `[workspace.lints]`. This follows the same convention as `[workspace.lints]` in Cargo.toml.

### Override rule severity

Set any rule to `"error"`, `"warn"`, or `"allow"`:

```toml
[workspace.lints]
# Promote to error — fail CI if descriptions are missing
"skill/missing-description" = "error"

# Suppress entirely — we don't care about skill size
"skill/oversized" = "allow"
```

### Ignore paths globally

Skip entire directories from all rules:

```toml
[workspace.lints.ignore]
paths = ["vendor/**", ".ai/legacy-*/**"]
```

### Ignore paths per rule

Use the inline table syntax to combine a severity override with rule-specific ignore paths:

```toml
[workspace.lints]
# Warn on broken paths, but skip the examples directory
"plugin/broken-paths" = { level = "warn", ignore = ["examples/**"] }

# Error on unknown hooks, but skip the experimental plugin
"hook/unknown-event" = { level = "error", ignore = [".ai/experimental/**"] }
```

### Full configuration example

```toml
[workspace]
members = [".ai/*"]
plugins_dir = ".ai"

[workspace.lints]
# Stricten
"skill/missing-description" = "error"
"skill/missing-name" = "error"

# Relax
"skill/oversized" = "allow"

# Per-rule ignore
"source/misplaced-features" = { level = "warn", ignore = [".claude/skills/legacy-*/**"] }

[workspace.lints.ignore]
paths = ["vendor/**", "third-party/**"]
```

---

## CLI Reference

```
aipm lint [OPTIONS] [dir]
```

| Flag | Description | Default |
|------|-------------|---------|
| `[dir]` | Directory to lint | `.` (current directory) |
| `--source <type>` | Filter to a specific source: `.claude`, `.github`, or `.ai` | All sources |
| `--format <fmt>` | Output format: `text` or `json` | `text` |
| `--max-depth <n>` | Maximum directory traversal depth | Unlimited |

### Examples

```bash
# Lint everything
aipm lint

# Only check the marketplace plugins
aipm lint --source .ai

# Only check for misplaced features in .claude/
aipm lint --source .claude

# JSON output for CI pipelines
aipm lint --format json

# Lint a different project
aipm lint ../other-project
```

---

## CI Integration

### GitHub Actions

```yaml
- name: Lint AI plugins
  run: aipm lint --format json
```

Since `aipm lint` exits `0` on warnings and `1` on errors, it works as a CI gate out of the box. To also fail on warnings, promote them in your config:

```toml
[workspace.lints]
"skill/missing-description" = "error"
"skill/missing-name" = "error"
```

### JSON output for tooling

`--format json` produces machine-readable output:

```json
{
  "diagnostics": [
    {
      "rule_id": "skill/missing-description",
      "severity": "warning",
      "message": "SKILL.md missing required field: description",
      "file_path": ".ai/my-plugin/skills/default/SKILL.md",
      "line": 1,
      "source_type": ".ai"
    },
    {
      "rule_id": "hook/unknown-event",
      "severity": "error",
      "message": "unknown hook event: InvalidEvent",
      "file_path": ".ai/my-plugin/hooks/hooks.json",
      "line": 5,
      "source_type": ".ai"
    }
  ],
  "summary": {
    "errors": 1,
    "warnings": 1,
    "sources_scanned": [".claude", ".ai"]
  }
}
```

---

## Common Workflows

### "I just ran `aipm init` — what do I do?"

Nothing extra. `aipm lint` will check your starter plugin automatically. If you followed the scaffolding, you'll see:

```
$ aipm lint
no issues found
```

### "I have existing `.claude/skills/` — should I migrate?"

Run lint first to see what's there:

```
$ aipm lint --source .claude
warning[source/misplaced-features]: 3 skills found in .claude/ instead of .ai/ marketplace
```

Then migrate them:

```
$ aipm migrate
```

Then lint again to validate the result:

```
$ aipm lint --source .ai
no issues found
```

### "I want strict linting for my team"

Promote all warnings to errors in `aipm.toml`:

```toml
[workspace.lints]
"skill/missing-name" = "error"
"skill/missing-description" = "error"
"skill/oversized" = "error"
"agent/missing-tools" = "error"
"source/misplaced-features" = "error"
```

### "I have a legacy plugin I can't fix yet"

Ignore it globally or per-rule:

```toml
[workspace.lints.ignore]
paths = [".ai/legacy-plugin/**"]
```

Or suppress just one rule for that plugin:

```toml
[workspace.lints]
"plugin/broken-paths" = { level = "error", ignore = [".ai/legacy-plugin/**"] }
```

---

## What's NOT in v1

These are planned for future versions:

| Feature | Status |
|---------|--------|
| `aipm-pack lint` (author CLI) | Planned for when publish gate is built |
| `--fix` auto-fix mode | v2 — e.g., auto-truncate names, add missing fields |
| Quality score on publish | v2 — computed score based on description, license, readme, examples |
| Publish gate | v2 — `aipm-pack publish` rejects packages failing lint |
| LSP / VS Code integration | Separate effort — real-time diagnostics in editor |
| `.vscode/` source adapter | Architecture supports it, rules not yet written |
| Binary analysis automation | Potential CI job to detect new CLI events/fields per release |

---

## Summary

| Aspect | Design |
|--------|--------|
| Command | `aipm lint` |
| Zero-config | Works out of the box, all 12 rules enabled |
| Configuration | `[workspace.lints]` in `aipm.toml` (Cargo-style) |
| Severity levels | `error` (blocks CI) and `warning` (advisory) |
| Rule IDs | Hierarchical: `skill/missing-description`, `hook/unknown-event` |
| Output | Human-readable (default) or JSON (`--format json`) |
| Exit codes | `0` = no errors, `1` = errors found |
| Ignore patterns | Global (`[workspace.lints.ignore]`) and per-rule (`{ level, ignore }`) |
| Sources | Auto-discovers `.claude/`, `.github/`, `.ai/`; filterable via `--source` |
