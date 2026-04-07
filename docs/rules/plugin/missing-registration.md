# plugin/missing-registration

**Severity:** error
**Fixable:** No

Validates that every plugin directory under `.ai/` is registered in the local marketplace file at `.ai/.claude-plugin/marketplace.json`. Unregistered plugins are invisible to `aipm install`, `aipm list`, and the lint system.

The `.claude-plugin` directory itself is excluded from this check.

## Examples

### Incorrect

```
.ai/
  .claude-plugin/
    marketplace.json    ← {"name":"local","plugins":[]}
  my-plugin/
    .claude-plugin/
      plugin.json
# ❌ my-plugin is not listed in marketplace.json
```

### Correct

```json
{
  "name": "local",
  "plugins": [
    {
      "name": "my-plugin",
      "source": "./my-plugin",
      "description": "What this plugin does"
    }
  ]
}
```

## How to fix

Add an entry for the plugin to the `plugins` array in `.ai/.claude-plugin/marketplace.json`. Each entry requires:

| Field | Required | Description |
|-------|----------|-------------|
| `name` | ✅ | Matches the plugin's `name` in `plugin.json` |
| `source` | ✅ | Relative path from `.ai/` to the plugin directory (e.g. `"./my-plugin"`) |
| `description` | optional | Brief description shown in `aipm list` |

If `.ai/.claude-plugin/marketplace.json` does not exist yet, create it:

```json
{
  "name": "local",
  "plugins": [
    {
      "name": "my-plugin",
      "source": "./my-plugin",
      "description": "What this plugin does"
    }
  ]
}
```

## See also

- [marketplace/source-resolve](../marketplace/source-resolve.md) — validates that each `source` path resolves
- [plugin/missing-manifest](missing-manifest.md) — validates that each plugin has a `plugin.json`
