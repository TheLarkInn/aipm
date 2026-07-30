# skill/description-invalid-chars

**Severity:** warning
**Fixable:** No

Checks that a skill `description` contains no `<` or `>` characters.

Copilot CLI injects skill descriptions into an XML-tagged block in the system prompt, so angle brackets in a description corrupt that markup. This is **not** a general YAML restriction and Claude Code does not impose it, so the rule is scoped to skills that Copilot can actually load:

| Skill location | Checked? |
|---|---|
| `.github/` | Always — Copilot's own source root |
| `.claude/` | Never |
| `.ai/<plugin>/` | Unless the nearest `aipm.toml` declares an `engines` list that excludes `copilot` |

The description is evaluated **after** YAML parsing, so folded (`>`) and literal (`|`) block scalars are checked by their resolved value rather than their source text.

## Examples

### Incorrect

```markdown
---
name: my-skill
description: Wraps the <tool> invocation for you
---
```

### Correct

```markdown
---
name: my-skill
description: Wraps the tool invocation for you
---
```

Or opt the plugin out of Copilot in `aipm.toml`:

```toml
[package]
name = "my-plugin"
version = "1.0.0"
engines = ["claude"]
```

## How to fix

Rewrite the description without angle brackets — name the tag or tool in prose instead — or declare `engines` in `aipm.toml` without `copilot` if the plugin is Claude-only.

## See also

- [skill/description-too-long](description-too-long.md) — validates the description length limit
- [skill/missing-description](missing-description.md) — validates that a `description` field is present and non-empty
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
