# plugin/required-fields

**Severity:** error
**Fixable:** No

Checks that every `.claude-plugin/plugin.json` file contains all five required fields. Missing or blank (whitespace-only) values are treated as absent.

| Field | Type | Notes |
|-------|------|-------|
| `name` | string | Non-empty after trimming whitespace |
| `description` | string | Non-empty after trimming whitespace |
| `version` | string | Non-empty after trimming whitespace |
| `author.name` | string | Must be inside an `author` object |
| `author.email` | string | Must be inside an `author` object |

A separate diagnostic is emitted for each missing field, so you will see up to five diagnostics for a completely empty manifest.

The rule also emits a diagnostic when `plugin.json`:

- **cannot be parsed** — a single parse-error diagnostic is emitted.
- **has an `author` field that is not an object** — a structural diagnostic is emitted instead of checking `author.name` / `author.email`.

## Examples

### Incorrect

```json
{
  "name": "my-plugin",
  "version": "0.1.0"
}
```

Diagnostics emitted:

- `plugin.json is missing required field: description`
- `plugin.json is missing required field: author`

### Also incorrect — whitespace-only value

```json
{
  "name": "  ",
  "description": "Useful tool",
  "version": "0.1.0",
  "author": { "name": "Alice", "email": "alice@example.com" }
}
```

Diagnostics emitted:

- `plugin.json is missing required field: name`

### Correct

```json
{
  "name": "my-plugin",
  "description": "Does something useful",
  "version": "0.1.0",
  "author": {
    "name": "Alice Smith",
    "email": "alice@example.com"
  }
}
```

## How to fix

Add the missing fields to your `.claude-plugin/plugin.json`. All five fields are required; none may be absent or blank. Run `aipm-pack init` to generate a valid manifest interactively.
