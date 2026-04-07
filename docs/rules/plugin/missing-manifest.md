# plugin/missing-manifest

**Severity:** error
**Fixable:** No

Checks that every plugin directory under `.ai/` contains a `.claude-plugin/plugin.json` file. This manifest is required for the plugin to declare its identity (name, version, author) and for other lint rules — such as `plugin/required-fields` and `marketplace/plugin-field-mismatch` — to operate correctly.

The `.claude-plugin` directory itself is excluded from this check.

## Examples

### Incorrect

```
.ai/
  my-plugin/
    agents/
      my-agent.md
    # ← no .claude-plugin/plugin.json
```

### Correct

```
.ai/
  my-plugin/
    .claude-plugin/
      plugin.json   ← required
    agents/
      my-agent.md
```

`plugin.json` minimum content:

```json
{
  "name": "my-plugin",
  "description": "Does something useful",
  "version": "0.1.0",
  "author": {
    "name": "Your Name",
    "email": "you@example.com"
  }
}
```

## How to fix

Create a `.claude-plugin/plugin.json` file inside the plugin directory. Run `aipm-pack init` from within the plugin directory to scaffold the file interactively, or create it manually with at least the `name`, `description`, `version`, and `author` fields.
