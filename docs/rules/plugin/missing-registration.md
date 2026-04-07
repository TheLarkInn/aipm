# plugin/missing-registration

**Severity:** error
**Fixable:** No

Checks that every plugin directory under `.ai/` is listed in the `plugins`
array of `.ai/.claude-plugin/marketplace.json`. The reserved `.claude-plugin`
directory itself is excluded from this check.

An unregistered plugin cannot be discovered or installed by consumers of the
marketplace. Adding the directory to `.ai/` without a matching entry in
`marketplace.json` is incomplete.

## Examples

### Incorrect

```
.ai/
├── .claude-plugin/
│   └── marketplace.json   ← does not list "my-plugin"
└── my-plugin/
    └── .claude-plugin/
        └── plugin.json
```

`marketplace.json`:
```json
{
  "plugins": []
}
```

### Correct

`marketplace.json` includes an entry for every plugin directory:
```json
{
  "plugins": [
    {
      "name": "my-plugin",
      "description": "What this plugin does",
      "source": "./my-plugin"
    }
  ]
}
```

## How to fix

Add an entry to the `plugins` array in `.ai/.claude-plugin/marketplace.json`
for the unregistered plugin. Each entry needs at minimum a `name`, a
`description`, and a `source` pointing to the plugin directory (relative to
`.ai/`, e.g. `"./my-plugin"`).

## See also

- [plugin/missing-manifest](missing-manifest.md) — validates that the plugin
  directory contains a `plugin.json`
- [marketplace/source-resolve](../../rules/marketplace/source-resolve.md) — validates
  that `source` paths in `marketplace.json` point to existing directories
