# marketplace/source-resolve

**Severity:** error
**Fixable:** No

Validates that every plugin entry in `marketplace.json` has a `source` field and that the path it specifies resolves to an existing directory under `.ai/`. A missing or broken `source` means the plugin cannot be located at install or lint time.

This rule also flags:

- A missing `plugins` array in `marketplace.json`.
- Any `source` value that is not a string.
- Any `source` path that does not exist on disk relative to the `.ai/` directory.

## Examples

### Incorrect — missing `source` field

```json
{
  "name": "local",
  "plugins": [
    { "name": "my-plugin" }
  ]
}
```

### Incorrect — path does not exist

```json
{
  "name": "local",
  "plugins": [
    { "name": "my-plugin", "source": "./nonexistent-plugin" }
  ]
}
```

*(where `.ai/nonexistent-plugin/` does not exist)*

### Correct

```json
{
  "name": "local",
  "plugins": [
    { "name": "my-plugin", "source": "./my-plugin" }
  ]
}
```

*(where `.ai/my-plugin/` exists on disk)*

## How to fix

Ensure the `source` field is present in each plugin entry and points to an existing plugin directory under `.ai/`. Paths are resolved relative to the `.ai/` directory; the leading `./` is optional.
