# marketplace/source-resolve

**Severity:** error
**Fixable:** No

Validates that every plugin entry in `marketplace.json` has a `source` field and that the path it specifies resolves to an existing directory under `.ai/`. A missing or broken `source` means the plugin cannot be located at install or lint time.

This rule also flags:

- A missing `plugins` array in `marketplace.json`.
- Any `source` value that is not a string.
- Any `source` path that does not exist on disk relative to the `.ai/` directory.
- Any `source` path containing `..` (parent-dir traversal), an absolute root (`/`), or a Windows drive prefix — these are rejected before any filesystem access to prevent path-traversal attacks.

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

### Incorrect — path traversal rejected

```json
{
  "name": "local",
  "plugins": [
    { "name": "evil", "source": "../../etc/passwd" }
  ]
}
```

Output:

```
error[marketplace/source-resolve]: plugin 'evil' source path '../../etc/passwd' rejected: parent-dir traversal, absolute paths, and Windows prefixes are not allowed
```

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

For path traversal errors, use a simple relative path like `./my-plugin` or `my-plugin` — paths containing `..`, starting with `/`, or using Windows drive letters are never valid plugin source paths.

## See also

- [plugin/missing-registration](../plugin/missing-registration.md) — validates that every plugin directory is listed in `marketplace.json`
- [marketplace/plugin-field-mismatch](plugin-field-mismatch.md) — validates that `name`/`description` match between `marketplace.json` and `plugin.json`
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
