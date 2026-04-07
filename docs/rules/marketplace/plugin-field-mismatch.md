# marketplace/plugin-field-mismatch

**Severity:** error
**Fixable:** No

Validates that the `name` and `description` fields in each plugin entry in `marketplace.json` match the corresponding values in that plugin's own `.claude-plugin/plugin.json`. Stale copies in the marketplace registry cause confusing behaviour when users install or list plugins — the registry metadata and the plugin's self-description diverge.

## Examples

### Incorrect

`.ai/.claude-plugin/marketplace.json`:

```json
{
  "name": "local",
  "plugins": [
    {
      "name": "my-plugin",
      "source": "./my-plugin",
      "description": "Old description that was never updated"
    }
  ]
}
```

`.ai/my-plugin/.claude-plugin/plugin.json`:

```json
{
  "name": "my-plugin",
  "description": "New authoritative description",
  "version": "1.2.0",
  "author": { "name": "Jane Doe", "email": "jane@example.com" }
}
```

*(The `description` values differ — the marketplace entry is stale.)*

### Correct

Both files agree on `name` and `description`:

`.ai/.claude-plugin/marketplace.json`:

```json
{
  "name": "local",
  "plugins": [
    {
      "name": "my-plugin",
      "source": "./my-plugin",
      "description": "New authoritative description"
    }
  ]
}
```

`.ai/my-plugin/.claude-plugin/plugin.json`:

```json
{
  "name": "my-plugin",
  "description": "New authoritative description",
  "version": "1.2.0",
  "author": { "name": "Jane Doe", "email": "jane@example.com" }
}
```

## How to fix

Update either `marketplace.json` or `plugin.json` so the `name` and `description` fields are identical in both files. Treat `plugin.json` as the authoritative source of truth and copy its values into the marketplace entry.
