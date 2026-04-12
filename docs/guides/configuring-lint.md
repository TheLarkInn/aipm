# Configuring the Lint System

`aipm lint` ships with sensible defaults. For project-specific needs you can tune rule severity, suppress noise, and exclude directories — all from your workspace `aipm.toml`.

## How it works

`aipm lint` reads the optional `[workspace.lints]` table from `aipm.toml` in the target directory before it runs. If the file is absent or the section is missing, every rule runs at its default severity.

## Override rule severity

Set any rule to `"error"`, `"warn"`, or `"allow"` — or the equivalent aliases `"deny"` and `"warning"`:

```toml
[workspace.lints]
# Promote to error — fail CI if descriptions are missing
"skill/missing-description" = "error"

# Demote to warning — still visible but won't block CI
"hook/unknown-event" = "warn"

# Suppress entirely — skip the rule for this project
"skill/oversized" = "allow"
```

Valid values: `"error"` · `"warn"` · `"allow"` — plus the aliases `"deny"` (same as `"error"`) and `"warning"` (same as `"warn"`)

## Ignore paths globally

Skip entire directories from **all** rules:

```toml
[workspace.lints.ignore]
paths = ["**/vendor/**", "**/.ai/legacy-*/**", "**/third-party/**"]
```

Paths are [glob patterns](https://docs.rs/glob/latest/glob/struct.Pattern.html) matched against the full file path. Use a `**/` prefix so the pattern matches at any depth — for example, `**/vendor/**` matches `vendor/` at the project root and any subdirectory.

## Ignore paths per rule

Combine a severity override with rule-specific ignore paths using the inline table syntax:

```toml
[workspace.lints]
# Warn on broken paths, but skip the examples directory
"plugin/broken-paths" = { level = "warn", ignore = ["**/examples/**"] }

# Error on unknown hooks, but skip experimental plugins
"hook/unknown-event" = { level = "error", ignore = ["**/.ai/experimental/**"] }
```

Fields: `level` (optional) and `ignore` (optional list of glob patterns).

Rules that support additional options (such as `instructions/oversized`) accept extra keys in
the same inline table or as a separate TOML section:

```toml
[workspace.lints]
# Raise the line/character limits for large monorepo instruction files
"instructions/oversized" = { lines = 200, characters = 20000, resolve-imports = true }
```

Or using a section header (useful when there are many options):

```toml
[workspace.lints."instructions/oversized"]
level = "error"
lines = 200
characters = 20000
resolve-imports = true
ignore = ["**/vendor/**"]
```

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
"source/misplaced-features" = { level = "warn", ignore = ["**/.claude/skills/legacy-*/**"] }

[workspace.lints.ignore]
paths = ["**/vendor/**", "**/third-party/**"]

[workspace.lints."instructions/oversized"]
level = "error"
lines = 200
characters = 20000
resolve-imports = true
ignore = ["**/vendor/**"]
```

## Severity levels

| Level | Aliases | Effect |
|-------|---------|--------|
| `"error"` | `"deny"` | Counts toward the error total; `aipm lint` exits non-zero when errors exist |
| `"warn"` | `"warning"` | Reported but does not fail the command |
| `"allow"` | — | Silenced; rule does not run |

## Rule IDs

All built-in rule IDs follow the `category/rule-name` hierarchy. See the individual reference pages in [`docs/rules/`](../rules/) for each rule's default severity and fixability.

| Category | Rules |
|----------|-------|
| `skill/` | `missing-name`, `missing-description`, `name-invalid-chars`, `name-too-long`, `description-too-long`, `invalid-shell`, `oversized` |
| `hook/` | `unknown-event`, `legacy-event-name` |
| `agent/` | `missing-tools` |
| `plugin/` | `broken-paths`, `missing-manifest`, `missing-registration`, `required-fields` |
| `marketplace/` | `plugin-field-mismatch`, `source-resolve` |
| `instructions/` | `oversized` |
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

## Editor schema support

A JSON Schema for `aipm.toml` provides autocomplete and validation for `[workspace.lints]` in any editor that supports Taplo/Tombi or JSON Schema associations. The schema intentionally covers only `[workspace.lints]` — other sections remain unconstrained.

**Schema URL:**

```
https://raw.githubusercontent.com/TheLarkInn/aipm/main/schemas/aipm.toml.schema.json
```

### VS Code

Install the [vscode-aipm](./vscode-extension.md) extension. It registers the schema via the `tomlValidation` contribution point automatically. Requires the **Even Better TOML** or **Taplo** VS Code extension for validation and autocomplete.

### Taplo (all editors)

[Taplo](https://taplo.tamasfe.dev/) is a TOML language server that works in Neovim, Helix, Emacs, and other editors via LSP. Add a `.taplo.toml` at your project root to associate the schema:

```toml
# .taplo.toml
[[rule]]
include = ["**/aipm.toml"]
schema = "https://raw.githubusercontent.com/TheLarkInn/aipm/main/schemas/aipm.toml.schema.json"
```

Once configured, you get:

- **Validation** — unknown rule IDs and type mismatches highlighted inline
- **Autocomplete** — all 18 rule IDs and severity values suggested on demand; rules with additional options (like `instructions/oversized`) also complete their per-rule fields such as `lines`, `characters`, and `resolve-imports`

### SchemaStore

A catalog entry is prepared at `schemas/schemastore-submission/catalog-entry.json` for submission to [SchemaStore.org](https://www.schemastore.org/). Once the SchemaStore PR is merged, Taplo and Tombi users will get zero-install coverage — no `.taplo.toml` needed.

---

See also: [`aipm lint` usage guide](./lint.md), [`aipm lint` README reference](../../README.md#aipm-lint), [lint rule reference](../rules/), [VS Code extension guide](./vscode-extension.md).
