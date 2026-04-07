# marketplace/source-resolve

**Severity:** error
**Fixable:** No

Checks that every plugin entry in `.ai/.claude-plugin/marketplace.json` has a `source` field and that the path it points to exists on disk. A broken source reference means `aipm install` cannot locate or install the plugin.

The rule emits distinct diagnostics for three different problems:

| Problem | Message pattern |
|---------|-----------------|
| `source` field is absent | `… is missing a 'source' field` |
| `source` field is not a string | `… 'source' field must be a string` |
| Path does not exist on disk | `… source path does not resolve: <path>` |

The rule also emits a diagnostic when `marketplace.json`:

- **cannot be parsed** — a single parse-error diagnostic is emitted.
- **is missing the `plugins` array** — a single structural diagnostic is emitted.

The rule is **silent** (no diagnostics) when `marketplace.json` does not exist; `plugin/missing-registration` handles that case.

## Examples

### Incorrect — missing `source`

```json
{
  "plugins": [
    { "name": "my-plugin" }
  ]
}
```

### Incorrect — non-string `source`

```json
{
  "plugins": [
    { "name": "my-plugin", "source": 42 }
  ]
}
```

### Incorrect — path does not exist

```json
{
  "plugins": [
    { "name": "my-plugin", "source": "./typo-plugin" }
  ]
}
```

*(where `typo-plugin/` does not exist under `.ai/`)*

### Correct

```json
{
  "plugins": [
    { "name": "my-plugin", "source": "./my-plugin" }
  ]
}
```

*(where `.ai/my-plugin/` exists on disk)*

## How to fix

Ensure each plugin entry in `marketplace.json` has a `source` field that is a string and points to an existing directory under `.ai/`. Relative paths starting with `./` are resolved relative to the `.ai/` directory. Remove or correct any entries that reference directories that have been deleted or renamed.
