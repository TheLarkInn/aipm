# plugin/required-fields

**Severity:** error
**Fixable:** No

Checks that every `.ai/<plugin>/.claude-plugin/plugin.json` file contains all required fields with non-empty string values. Missing or blank fields prevent the plugin from being installed or distributed correctly.

## Required fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Plugin identifier, must be non-empty |
| `description` | string | Short human-readable summary, must be non-empty |
| `version` | string | Release version (e.g., `"1.0.0"`), must be non-empty |
| `author.name` | string | Author's display name, must be non-empty |
| `author.email` | string | Author's contact email, must be non-empty |

## Examples

### Incorrect — missing `author` object

```json
{
  "name": "my-plugin",
  "description": "Does useful things",
  "version": "1.0.0"
}
```

### Incorrect — blank field value

```json
{
  "name": "my-plugin",
  "description": "",
  "version": "1.0.0",
  "author": { "name": "Alice", "email": "alice@example.com" }
}
```

### Correct

```json
{
  "name": "my-plugin",
  "description": "Does useful things",
  "version": "1.0.0",
  "author": {
    "name": "Alice",
    "email": "alice@example.com"
  }
}
```

## How to fix

Add the missing fields to the plugin's `.claude-plugin/plugin.json`. Every field listed in the table above must be present and must contain a non-empty string value.

When creating a new plugin, prefer `aipm init` or `aipm-pack init` — both scaffold a `plugin.json` that includes all required fields automatically.
