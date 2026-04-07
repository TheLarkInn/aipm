# marketplace/plugin-field-mismatch

**Severity:** error
**Fixable:** No

Checks that the `name` and `description` fields in each plugin entry of
`.ai/.claude-plugin/marketplace.json` match the corresponding values in the
plugin's own `.claude-plugin/plugin.json`.

A mismatch between the two manifests means users see inconsistent metadata
depending on which file a tool reads — the marketplace index or the installed
plugin itself.

## Examples

### Incorrect

`.ai/.claude-plugin/marketplace.json`:
```json
{
  "plugins": [
    {
      "name": "my-tool",
      "description": "A general-purpose tool",
      "source": "./my-tool"
    }
  ]
}
```

`.ai/my-tool/.claude-plugin/plugin.json`:
```json
{
  "name": "my-tool",
  "description": "A specialized automation tool",
  "version": "1.0.0",
  "author": { "name": "Alice", "email": "alice@example.com" }
}
```

*(The `description` field differs between the two files.)*

### Correct

Both files agree on `name` and `description`:

`.ai/.claude-plugin/marketplace.json`:
```json
{
  "plugins": [
    {
      "name": "my-tool",
      "description": "A specialized automation tool",
      "source": "./my-tool"
    }
  ]
}
```

`.ai/my-tool/.claude-plugin/plugin.json`:
```json
{
  "name": "my-tool",
  "description": "A specialized automation tool",
  "version": "1.0.0",
  "author": { "name": "Alice", "email": "alice@example.com" }
}
```

## How to fix

Update whichever file contains the stale value so both `name` and `description`
fields are identical. Typically `plugin.json` is the source of truth — copy its
values into the corresponding entry in `marketplace.json`.

## Fields checked

| Field | Required to match |
|-------|-------------------|
| `name` | Yes — both files must have the same non-empty name |
| `description` | Yes — when both files declare a description, they must be equal |

If `marketplace.json` does not include a `description` for an entry, or if
`plugin.json` has no `description`, the check is skipped for that field.

## See also

- [marketplace/source-resolve](source-resolve.md) — validates that the `source` path exists
- [plugin/required-fields](../plugin/required-fields.md) — validates required fields in `plugin.json`
