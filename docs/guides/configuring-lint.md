# Configuring the Lint System

`aipm lint` ships with sensible defaults. For project-specific needs you can tune rule severity, suppress noise, and exclude directories — all from your workspace `aipm.toml`.

## How it works

`aipm lint` reads the optional `[workspace.lints]` table from `aipm.toml` in the target directory before it runs. If the file is absent or the section is missing, every rule runs at its default severity.

## Override rule severity

Set any rule to `"error"`, `"warn"`, or `"allow"`:

```toml
[workspace.lints]
# Promote to error — fail CI if descriptions are missing
"skill/missing-description" = "error"

# Demote to warning — still visible but won't block CI
"hook/unknown-event" = "warn"

# Suppress entirely — skip the rule for this project
"skill/oversized" = "allow"
```

Valid values: `"error"` · `"warn"` · `"allow"`

## Ignore paths globally

Skip entire directories from **all** rules:

```toml
[workspace.lints.ignore]
paths = ["vendor/**", ".ai/legacy-*/**", "third-party/**"]
```

Paths are [glob patterns](https://docs.rs/glob/latest/glob/struct.Pattern.html) matched against the relative path of each file.

## Ignore paths per rule

Combine a severity override with rule-specific ignore paths using the inline table syntax:

```toml
[workspace.lints]
# Warn on broken paths, but skip the examples directory
"plugin/broken-paths" = { level = "warn", ignore = ["examples/**"] }

# Error on unknown hooks, but skip experimental plugins
"hook/unknown-event" = { level = "error", ignore = [".ai/experimental/**"] }
```

Fields: `level` (required) and `ignore` (optional list of glob patterns).

## Full configuration example

```toml
[workspace]
members = [".ai/*"]
plugins_dir = ".ai"

[workspace.lints]
# Tighten
"skill/missing-description" = "error"
"skill/missing-name"        = "error"

# Relax
"skill/oversized" = "allow"

# Per-rule ignore
"source/misplaced-features" = { level = "warn", ignore = [".claude/skills/legacy-*/**"] }

[workspace.lints.ignore]
paths = ["vendor/**", "third-party/**"]
```

## Severity levels

| Level | Effect |
|-------|--------|
| `"error"` | Counts toward the error total; `aipm lint` exits non-zero when errors exist |
| `"warn"` | Reported but does not fail the command |
| `"allow"` | Silenced; rule does not run |

## Rule IDs

All built-in rule IDs follow the `category/rule-name` hierarchy. See the individual reference pages in [`docs/rules/`](../rules/) for each rule's default severity and fixability.

| Category | Rules |
|----------|-------|
| `skill/` | `missing-name`, `missing-description`, `name-invalid-chars`, `name-too-long`, `description-too-long`, `invalid-shell`, `oversized` |
| `hook/` | `unknown-event`, `legacy-event-name` |
| `agent/` | `missing-tools` |
| `plugin/` | `broken-paths` |
| `source/` | `misplaced-features` |

## CI usage

In a CI environment, treat missing descriptions as hard errors:

```toml
[workspace.lints]
"skill/missing-description" = "error"
"skill/missing-name"        = "error"
"agent/missing-tools"       = "error"
```

Then run:

```bash
aipm lint --reporter ci-github   # GitHub Actions annotations
aipm lint --reporter ci-azure    # Azure Pipelines annotations
aipm lint --reporter json        # JSON for custom tooling
```

See also: [`aipm lint` usage guide](./lint.md), [`aipm lint` README reference](../../README.md#aipm-lint), [lint rule reference](../rules/).
