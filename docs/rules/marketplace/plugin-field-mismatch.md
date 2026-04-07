# marketplace/plugin-field-mismatch

**Severity:** error
**Fixable:** No

Checks that the `name` and `description` fields for each plugin entry in `.ai/.claude-plugin/marketplace.json` match the corresponding values in the plugin's own `.claude-plugin/plugin.json`. Inconsistent fields cause confusing output when listing or searching plugins because the registry and the plugin manifest disagree.

## Examples

### Incorrect — name mismatch

`marketplace.json`:

```json
{
  "plugins": [
    {
      "name": "my-awesome-plugin",
      "source": "./my-plugin",
      "description": "Does useful things"
    }
  ]
}
```

`.ai/my-plugin/.claude-plugin/plugin.json`:

```json
{
  "name": "my-plugin",
  "description": "Does useful things",
  "version": "1.0.0",
  "author": { "name": "Alice", "email": "alice@example.com" }
}
```

*(The `name` in `marketplace.json` is `"my-awesome-plugin"` but `plugin.json` says `"my-plugin"`)*

### Correct

`marketplace.json`:

```json
{
  "plugins": [
    {
      "name": "my-plugin",
      "source": "./my-plugin",
      "description": "Does useful things"
    }
  ]
}
```

`.ai/my-plugin/.claude-plugin/plugin.json`:

```json
{
  "name": "my-plugin",
  "description": "Does useful things",
  "version": "1.0.0",
  "author": { "name": "Alice", "email": "alice@example.com" }
}
```

## How to fix

Decide which file is the authoritative source of truth and update the other to match.

- To update `marketplace.json`: edit the `name` or `description` field for the relevant plugin entry.
- To update `plugin.json`: edit the `name` or `description` field inside the plugin's `.claude-plugin/plugin.json` file.

The `name` and `description` in both files must be identical (same casing and whitespace).
