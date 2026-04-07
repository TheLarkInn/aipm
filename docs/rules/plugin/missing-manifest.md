# plugin/missing-manifest

**Severity:** error
**Fixable:** No

Checks that every plugin directory under `.ai/` contains a
`.claude-plugin/plugin.json` manifest file. The reserved `.claude-plugin`
directory itself is excluded from this check.

Without a `plugin.json`, the plugin cannot be identified, installed, or
validated by `aipm`.

## Examples

### Incorrect

```
.ai/
├── .claude-plugin/
│   └── marketplace.json
└── my-plugin/           ← plugin directory
    └── SKILL.md         ← but no .claude-plugin/plugin.json!
```

### Correct

```
.ai/
├── .claude-plugin/
│   └── marketplace.json
└── my-plugin/
    ├── .claude-plugin/
    │   └── plugin.json  ← manifest present
    └── SKILL.md
```

## How to fix

Create a `.claude-plugin/plugin.json` file inside the plugin directory. The
manifest must include the required fields `name`, `description`, `version`,
`author.name`, and `author.email`:

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

You can scaffold a new plugin with all required files using:

```sh
aipm-pack init
```

## See also

- [plugin/required-fields](required-fields.md) — validates that `plugin.json` contains
  all required fields
- [plugin/missing-registration](missing-registration.md) — validates that the plugin
  is registered in `marketplace.json`
