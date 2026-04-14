# plugin/required-fields

**Severity:** error
**Fixable:** No

Validates that a plugin's `.claude-plugin/plugin.json` contains all required fields. Missing or blank fields prevent the plugin from being installed, listed, or identified correctly.

## Required fields

| Field | Type | Notes |
|-------|------|-------|
| `name` | string | Non-empty; identifies the plugin |
| `description` | string | Non-empty; shown in `aipm list` and marketplace |
| `version` | string | Non-empty; follows [SemVer](https://semver.org/) convention |
| `author.name` | string | Non-empty; name of the plugin author or team |
| `author.email` | string | Non-empty; contact email |

## Examples

### Incorrect — missing `version` and `author`

```json
{
  "name": "my-plugin",
  "description": "Does something useful"
}
```

### Correct

```json
{
  "name": "my-plugin",
  "description": "Does something useful",
  "version": "1.0.0",
  "author": {
    "name": "Jane Doe",
    "email": "jane@example.com"
  }
}
```

## How to fix

Add the missing fields to `.claude-plugin/plugin.json`. All five required fields must be present and non-empty strings.

**For plugins created with `aipm init`:** As of the DRY architecture refactor, `aipm init` generates a complete `plugin.json` for the starter plugin, including placeholder `author.name` and `author.email` values set to `"TODO"`. Replace these placeholders with real values:

```json
{
  "name": "my-plugin",
  "description": "What this plugin does",
  "version": "0.1.0",
  "author": {
    "name": "Your Name",
    "email": "you@example.com"
  }
}
```

**For manually created or migrated plugins:** Create or update `.claude-plugin/plugin.json` with all five required fields.

## See also

- [plugin/missing-manifest](missing-manifest.md) — validates that `plugin.json` exists
- [plugin/missing-registration](missing-registration.md) — validates the plugin is listed in `marketplace.json`
- [Creating a plugin](../../guides/creating-a-plugin.md) — how to create a plugin package
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
- [Configuring lint](../../guides/configuring-lint.md) — override rule severity or suppress rules per path
