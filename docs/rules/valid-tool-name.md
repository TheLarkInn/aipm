# valid-tool-name

**Severity:** warning (escalates to **error** when engines are declared)
**Fixable:** No
**Applies to:** agents, skills, hooks

Checks that every tool name listed in a feature file's frontmatter `tools` field is
supported by the engine(s) the plugin targets. Tool compatibility is derived from the
engine API schema — a machine-generated source of truth updated weekly by the
[reverse-binary-analysis workflow](../../.github/workflows/reverse-binary-analysis.md).

## Severity rules

| Situation | Severity |
|-----------|----------|
| No `engines` field in `aipm.toml` (or no `aipm.toml`) | **Warning** — suggests declaring the engines that support the tool |
| `engines` declared but none support the tool | **Error** — the tool will not be available at runtime |

## Shared tools (always valid)

The following tools are available on both `claude` and `copilot-cli` and never trigger
this rule regardless of engine declarations:

- `bash`
- `glob`
- `grep`
- `web_fetch`

## Examples

### Incorrect — engine-exclusive tool, no `engines` declared

```markdown
---
name: pr-reviewer
tools: get_pull_request, bash
---
Prompt text…
```

```
# aipm.toml (no engines field)
[package]
name = "my-plugin"
version = "1.0.0"
```

`get_pull_request` is exclusive to `copilot-cli`. Without an `engines` declaration,
this emits a **warning** suggesting you add `engines = ["copilot-cli"]`.

### Incorrect — wrong engine declared

```markdown
---
name: pr-reviewer
tools: get_pull_request, bash
---
Prompt text…
```

```toml
# aipm.toml
[package]
name = "my-plugin"
version = "1.0.0"
engines = ["claude"]
```

`get_pull_request` is not supported by `claude`. With an explicit engine declaration
that doesn't intersect the tool's support set, this is an **error**.

### Correct — engine-exclusive tool with matching declaration

```markdown
---
name: pr-reviewer
tools: get_pull_request, bash
---
Prompt text…
```

```toml
# aipm.toml
[package]
name = "my-plugin"
version = "1.0.0"
engines = ["copilot-cli"]
```

`get_pull_request` is supported by `copilot-cli`. The declaration matches — no
diagnostic.

### Correct — claude-exclusive tool with matching declaration

```markdown
---
name: worktree-agent
tools: Task, bash, glob
---
Prompt text…
```

```toml
# aipm.toml
[package]
name = "my-plugin"
version = "1.0.0"
engines = ["claude"]
```

`Task` is claude-exclusive; the declaration matches — no diagnostic.

### Correct — shared tools only (no restriction needed)

```markdown
---
name: file-analyzer
tools: bash, grep, glob
---
Prompt text…
```

Shared tools work with any engine — no engine declaration required.

## How to fix

**When you receive a warning** (no engines declared):

Add an `engines` field to your `aipm.toml` listing the engines that support the tool:

```toml
[package]
name = "my-plugin"
version = "1.0.0"
engines = ["copilot-cli"]   # or ["claude"], or ["claude", "copilot-cli"]
```

**When you receive an error** (wrong engine declared):

Either:
- Change the `engines` declaration to include an engine that supports the tool, or
- Replace the tool with a shared alternative (e.g., use `bash` instead of an
  engine-exclusive shell tool), or
- Remove the tool from the feature file if it is not actually needed.

## Suppressing the rule

To demote the rule globally to a warning or suppress it entirely:

```toml
[workspace.lints]
"valid-tool-name" = "warn"    # keep as warning (already the default)
"valid-tool-name" = "allow"   # suppress entirely
```

To suppress for a specific directory only:

```toml
[workspace.lints]
"valid-tool-name" = { ignore = ["**/.ai/experimental/**"] }
```

## Engine-exclusive tool reference

For the canonical list of which tools belong to which engine, see the
[engine API changelog](../../crates/libaipm-engine-spec/data/engine-api-changelog.md)
or the raw [engine API schema](../../crates/libaipm-engine-spec/data/engine-api-schema.json)
(`tool_compatibility.engine_exclusive_tools`).

## See also

- [Engine and platform compatibility](../guides/engine-platform-compatibility.md) — how to declare `engines` in `aipm.toml`
- [Configuring lint](../guides/configuring-lint.md) — override rule severity or suppress rules per path
- [Using `aipm lint`](../guides/lint.md) — CLI reference for running the lint system
- [Creating a plugin](../guides/creating-a-plugin.md) — how to scaffold a new plugin with correctly structured feature files
