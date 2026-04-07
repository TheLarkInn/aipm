# plugin/required-fields

**Severity:** error
**Fixable:** No

Checks that `plugin.json` contains all five required top-level fields:
`name`, `description`, `version`, `author.name`, and `author.email`.

Missing fields prevent `aipm` and compatible tools from reliably identifying
the plugin, attributing authorship, or performing version-based dependency
resolution.

## Required fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Unique identifier for the plugin |
| `description` | string | Short summary of what the plugin does |
| `version` | string | Semantic version (e.g. `"1.0.0"`) |
| `author.name` | string | Display name of the plugin author |
| `author.email` | string | Contact email for the plugin author |

## Examples

### Incorrect — missing `description` and `author`

```json
{
  "name": "my-plugin",
  "version": "0.1.0"
}
```

### Correct

```json
{
  "name": "my-plugin",
  "description": "Automates common project tasks",
  "version": "0.1.0",
  "author": {
    "name": "Alice",
    "email": "alice@example.com"
  }
}
```

## How to fix

Add the missing fields to `.claude-plugin/plugin.json`. One diagnostic is
emitted per missing field, so resolve them all before re-running `aipm lint`.

If you are creating a new plugin from scratch, use `aipm-pack init` to scaffold
a `plugin.json` with all required fields pre-populated.

## See also

- [plugin/missing-manifest](missing-manifest.md) — validates that `plugin.json`
  exists in the plugin directory
- [marketplace/plugin-field-mismatch](../marketplace/plugin-field-mismatch.md) —
  validates that `name` and `description` match the `marketplace.json` entry
