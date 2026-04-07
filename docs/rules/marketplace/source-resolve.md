# marketplace/source-resolve

**Severity:** error
**Fixable:** No

Checks that every plugin entry in `.ai/.claude-plugin/marketplace.json` has a
`source` field and that the path it references resolves to an existing directory
on disk.

A missing or broken `source` path means the plugin cannot be located at install
time, making the marketplace entry non-functional.

## Examples

### Incorrect

```json
{
  "plugins": [
    {
      "name": "my-tool",
      "description": "My tool",
      "source": "./my-tool"
    }
  ]
}
```

*(where the directory `.ai/my-tool/` does not exist)*

### Also incorrect — missing source field

```json
{
  "plugins": [
    {
      "name": "my-tool",
      "description": "My tool"
    }
  ]
}
```

### Correct

```json
{
  "plugins": [
    {
      "name": "my-tool",
      "description": "My tool",
      "source": "./my-tool"
    }
  ]
}
```

*(where the directory `.ai/my-tool/` exists on disk)*

## How to fix

- If the plugin directory was deleted or renamed, either restore it or update
  the `source` value in `marketplace.json` to point to its new location.
- If the entry is no longer needed, remove it from the `plugins` array.
- Paths must be relative to the `.ai/` directory (e.g. `"./my-tool"`).

## See also

- [marketplace/plugin-field-mismatch](plugin-field-mismatch.md) — validates
  name/description consistency between `marketplace.json` and `plugin.json`
- [plugin/missing-manifest](../plugin/missing-manifest.md) — validates that every
  plugin directory contains a `plugin.json`
