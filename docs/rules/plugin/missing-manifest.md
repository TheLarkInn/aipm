# plugin/missing-manifest

**Severity:** error
**Fixable:** No

Validates that every plugin directory under `.ai/` contains a `.claude-plugin/plugin.json` manifest file. Without this file the plugin cannot be identified, validated, or installed by `aipm`.

The `.claude-plugin` directory itself is excluded from this check.

## Examples

### Incorrect

```
.ai/
  my-plugin/          ← plugin directory
    commands/
    agents/
    # ❌ no .claude-plugin/plugin.json
```

### Correct

```
.ai/
  my-plugin/
    .claude-plugin/
      plugin.json     ← ✅ manifest is present
    commands/
    agents/
```

## How to fix

Create a `.claude-plugin/plugin.json` file inside the plugin directory with at minimum the required fields:

```json
{
  "name": "my-plugin",
  "description": "What this plugin does",
  "version": "0.1.0",
  "author": {
    "name": "Your Name",
    "email": "you@example.com"
  }
}
```

Run `aipm-pack init` to scaffold a new plugin with the correct layout automatically.

## See also

- [plugin/required-fields](required-fields.md) — validates the contents of `plugin.json`
- [plugin/missing-registration](missing-registration.md) — validates the plugin is listed in `marketplace.json`
