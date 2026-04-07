# Linting Plugin Configurations

`aipm lint` validates AI plugin configurations for quality issues. It performs a
single unified, gitignore-aware walk of the project tree, applies quality rules to
every discovered feature file, and exits with a non-zero status when any violations
are found — making it safe to drop into CI pipelines.

## Basic Usage

```bash
# Lint the current directory
aipm lint

# Lint a specific project root
aipm lint ./my-project

# Limit to a single source type
aipm lint --source .claude
aipm lint --source .ai
aipm lint --source .github
```

## CLI Flags

| Flag | Description |
|------|-------------|
| `--source <SRC>` | Limit linting to a specific source type (`.claude`, `.github`, `.ai`) |
| `--reporter <FMT>` | Output format: `human` (default), `json`, `ci-github`, `ci-azure` |
| `--color <MODE>` | Color output: `auto` (default), `always`, `never` |
| `--max-depth <N>` | Maximum directory traversal depth |

## Output Formats

### `human` (default)

Rich, colored output modeled after the Rust compiler — shows the rule ID, message,
file path with line number, and a help link when available.

```
warning[skill/missing-description]: SKILL.md missing recommended field: description
  --> .ai/my-plugin/skills/deploy/SKILL.md:1
  |
1 | ---
  |
  = help: add a "description" field to the YAML frontmatter
  = see: https://github.com/TheLarkInn/aipm/blob/main/docs/rules/skill/missing-description.md

warning: 1 warning emitted
```

### `json`

Machine-readable JSON object with a `diagnostics` array and a `summary`, suitable
for IDE extensions or custom tooling:

```bash
aipm lint --reporter json
```

```json
{
  "diagnostics": [
    {
      "rule_id": "skill/missing-description",
      "severity": "warning",
      "severity_code": 2,
      "message": "SKILL.md missing recommended field: description",
      "file_path": ".ai/my-plugin/skills/deploy/SKILL.md",
      "line": 1,
      "col": null,
      "end_line": null,
      "end_col": null,
      "help_url": "https://github.com/TheLarkInn/aipm/blob/main/docs/rules/skill/missing-description.md",
      "help_text": "add a \"description\" field to the YAML frontmatter",
      "source_type": ".ai"
    }
  ],
  "summary": {
    "errors": 0,
    "warnings": 1,
    "sources_scanned": [".ai/my-plugin"]
  }
}
```

Fields:

| Field | Type | Description |
|-------|------|-------------|
| `rule_id` | string | Hierarchical rule identifier (e.g. `"skill/missing-description"`) |
| `severity` | string | `"warning"` or `"error"` |
| `severity_code` | number | `2` for warning, `1` for error |
| `message` | string | Human-readable description of the finding |
| `file_path` | string | Path to the file where the issue was found |
| `line` | number\|null | 1-based line number, or `null` |
| `col` | number\|null | 1-based column number, or `null` |
| `end_line` | number\|null | End line for multi-line spans, or `null` |
| `end_col` | number\|null | End column for multi-line spans, or `null` |
| `help_url` | string\|null | Link to the rule documentation, or `null` |
| `help_text` | string\|null | Fix suggestion, or `null` |
| `source_type` | string | Source directory type that produced this diagnostic (e.g. `".ai"`, `".claude"`) |

### `ci-github`

Emits [GitHub Actions workflow commands](https://docs.github.com/en/actions/writing-workflows/choosing-what-your-workflow-does/workflow-commands-for-github-actions)
(`::error` / `::warning`) so violations appear as file annotations in pull requests:

```bash
aipm lint --reporter ci-github
```

```
::warning file=.ai/my-plugin/skills/deploy/SKILL.md,line=1::skill/missing-description: SKILL.md missing recommended field: description
```

### `ci-azure`

Emits [Azure Pipelines logging commands](https://learn.microsoft.com/en-us/azure/devops/pipelines/scripts/logging-commands)
(`##vso[task.logissue]`) for Azure DevOps:

```bash
aipm lint --reporter ci-azure
```

## CI Integration

### GitHub Actions

```yaml
- name: Lint AI plugins
  run: aipm lint --reporter ci-github
```

Violations appear as inline annotations on the changed files in pull requests.

### Azure Pipelines

```yaml
- script: aipm lint --reporter ci-azure
  displayName: Lint AI plugins
```

### Generic CI (fail on errors only)

```bash
# Exit 0 on warnings, non-zero on errors
aipm lint --reporter json | jq '[.diagnostics[] | select(.severity == "error")] | length' | grep -q '^0$'
```

## Configuring Lint Rules

Lint rules are configured in the `[workspace.lints]` section of `aipm.toml`. All
options are optional — rules run at their default severity when not overridden.

### Suppressing a rule

Set a rule to `"allow"` to silence it entirely:

```toml
[workspace.lints]
"skill/oversized" = "allow"
```

### Changing severity

Promote a warning to an error, or demote an error to a warning:

```toml
[workspace.lints]
"skill/missing-description" = "error"   # promote to error
"plugin/broken-paths" = "warn"          # demote to warning
```

Valid severity values: `"error"` / `"deny"` (treated identically) and `"warn"` /
`"warning"` (treated identically).

### Per-rule ignore paths

Use a table form with `level` and `ignore` to exclude specific files from one rule:

```toml
[workspace.lints."plugin/broken-paths"]
level = "warn"
ignore = ["**/examples/**", "**/fixtures/**"]
```

Ignore patterns are matched against the full file path using `*` as a wildcard
(case-insensitive). Use `**/` prefixes to match at any depth:

```toml
[workspace.lints."skill/oversized"]
level = "warn"
ignore = ["**/.ai/starter-aipm-plugin/**"]
```

### Globally ignoring paths

Ignore entire directories across all rules:

```toml
[workspace.lints.ignore]
paths = ["**/vendor/**", "**/fixtures/**"]
```

### Full example `[workspace.lints]`

```toml
[workspace]
members = ["plugins/*"]

[workspace.lints]
# Suppress noisy rules in generated / vendored content
"skill/oversized" = "allow"

# Escalate critical rules to errors in CI
"skill/missing-name" = "error"

# Detailed rule override with per-rule ignores
[workspace.lints."plugin/broken-paths"]
level = "error"
ignore = ["**/examples/**"]

# Global path ignores (all rules)
[workspace.lints.ignore]
paths = ["**/vendor/**", "**/.ai/starter-aipm-plugin/**"]
```

## Rules Reference

All available rules, grouped by category:

### `agent/`

| Rule | Severity | Description |
|------|----------|-------------|
| [`agent/missing-tools`](../rules/agent/missing-tools.md) | warning | AGENT.md is missing a `tools` field in frontmatter |

### `hook/`

| Rule | Severity | Description |
|------|----------|-------------|
| [`hook/legacy-event-name`](../rules/hook/legacy-event-name.md) | warning | Hook uses a deprecated event name |
| [`hook/unknown-event`](../rules/hook/unknown-event.md) | error | Hook uses an unrecognised event name |

### `plugin/`

| Rule | Severity | Description |
|------|----------|-------------|
| [`plugin/broken-paths`](../rules/plugin/broken-paths.md) | error | Plugin references a file path that does not exist |

### `skill/`

| Rule | Severity | Description |
|------|----------|-------------|
| [`skill/description-too-long`](../rules/skill/description-too-long.md) | warning | `description` frontmatter value exceeds the length limit |
| [`skill/invalid-shell`](../rules/skill/invalid-shell.md) | error | `shell` frontmatter value is not a recognised shell |
| [`skill/missing-description`](../rules/skill/missing-description.md) | warning | SKILL.md is missing a `description` field in frontmatter |
| [`skill/missing-name`](../rules/skill/missing-name.md) | warning | SKILL.md is missing a `name` field in frontmatter |
| [`skill/name-invalid-chars`](../rules/skill/name-invalid-chars.md) | warning | Skill `name` contains characters that are not allowed |
| [`skill/name-too-long`](../rules/skill/name-too-long.md) | warning | Skill `name` exceeds the maximum length |
| [`skill/oversized`](../rules/skill/oversized.md) | warning | SKILL.md file exceeds the recommended size limit |

### `source/`

| Rule | Severity | Description |
|------|----------|-------------|
| [`source/misplaced-features`](../rules/source/misplaced-features.md) | warning | AI plugin feature files are outside the `.ai/` marketplace directory |

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | No errors (warnings do not affect the exit code) |
| `1` | One or more error-severity violations found |
| `2` | Unexpected I/O or internal error |

## See also

- [Configuring the lint system](./configuring-lint.md) — rule severity overrides, per-rule path ignores, and global path excludes
- [Lint rule reference](../rules/) — individual rule pages with severity, rationale, and fix guidance
