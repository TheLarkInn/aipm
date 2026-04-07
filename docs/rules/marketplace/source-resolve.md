# marketplace/source-resolve

**Severity:** error
**Fixable:** No

Checks that every plugin entry in `.ai/.claude-plugin/marketplace.json` has a `source` field and that the path it points to exists on disk. A missing or unresolvable `source` means `aipm install` and other commands cannot locate the plugin at runtime.

## Examples

### Incorrect — missing `source` field

```json
{
  "plugins": [
    {
      "name": "my-plugin",
      "description": "Does useful things"
    }
  ]
}
```

### Incorrect — `source` path does not exist

```json
{
  "plugins": [
    {
      "name": "my-plugin",
      "source": "./typo-plugin-name",
      "description": "Does useful things"
    }
  ]
}
```

*(where `.ai/typo-plugin-name/` does not exist)*

### Correct

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

*(where `.ai/my-plugin/` exists on disk)*

## How to fix

1. Ensure every entry in the `plugins` array has a `source` field.
2. The value must be a relative path (e.g., `"./my-plugin"`) that resolves to an existing directory under `.ai/`.
3. If the plugin directory was renamed or deleted, either restore it or remove the stale entry from `marketplace.json`.

Run `aipm install` after correcting the manifest to verify the plugin resolves correctly.
