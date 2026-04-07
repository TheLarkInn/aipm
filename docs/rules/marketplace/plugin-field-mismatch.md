# marketplace/plugin-field-mismatch

**Severity:** error
**Fixable:** No

Checks that the `name` and `description` fields in each plugin entry in `.ai/.claude-plugin/marketplace.json` match the corresponding values in the plugin's own `.claude-plugin/plugin.json`. Inconsistent metadata causes confusion for consumers and can silently break tooling that indexes plugins by name.

The rule compares:

| Field | Condition for diagnostic |
|-------|--------------------------|
| `name` | Both files have a non-empty value and they differ |
| `description` | Both files have a value and they differ |

If either file is missing a field, no diagnostic is emitted for that field — `plugin/required-fields` handles missing fields separately.

The rule is **silent** when:

- `marketplace.json` does not exist or cannot be read.
- A plugin entry has no `source` field (handled by `marketplace/source-resolve`).
- The plugin's `plugin.json` does not exist (handled by `plugin/missing-manifest`).

The rule emits a parse-error diagnostic if either file contains invalid JSON.

## Examples

### Incorrect — name mismatch

`marketplace.json`:

```json
{
  "plugins": [
    {
      "name": "my-plugin",
      "source": "./my-plugin"
    }
  ]
}
```

`.ai/my-plugin/.claude-plugin/plugin.json`:

```json
{
  "name": "myplugin",
  "description": "Does something useful",
  "version": "0.1.0",
  "author": { "name": "Alice", "email": "alice@example.com" }
}
```

Diagnostic: `plugin name mismatch: marketplace.json has 'my-plugin' but plugin.json has 'myplugin'`

### Incorrect — description mismatch

`marketplace.json`:

```json
{
  "plugins": [
    {
      "name": "my-plugin",
      "description": "Old description",
      "source": "./my-plugin"
    }
  ]
}
```

`.ai/my-plugin/.claude-plugin/plugin.json`:

```json
{
  "name": "my-plugin",
  "description": "Updated description",
  "version": "0.1.0",
  "author": { "name": "Alice", "email": "alice@example.com" }
}
```

Diagnostic: `plugin 'my-plugin' description mismatch: marketplace.json has 'Old description' but plugin.json has 'Updated description'`

### Correct

Both files use identical `name` and `description` values.

## How to fix

Keep `name` and `description` in sync between `marketplace.json` and each plugin's `plugin.json`. The canonical source of truth for a plugin's identity is its own `plugin.json` — update `marketplace.json` to match whenever the plugin metadata changes.
