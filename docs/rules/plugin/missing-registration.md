# plugin/missing-registration

**Severity:** error
**Fixable:** No

Checks that every plugin directory under `.ai/` is listed in `.ai/.claude-plugin/marketplace.json`. An unregistered plugin directory is never surfaced to consumers, so its agents, commands, skills, and hooks will silently do nothing.

The `.claude-plugin` directory itself is excluded from this check.

The rule also emits a diagnostic when `marketplace.json`:

- **does not exist** — all plugin directories are reported as unregistered.
- **cannot be parsed** — a single parse-error diagnostic is emitted.
- **is missing the `plugins` array** — a single structural diagnostic is emitted.

## Examples

### Incorrect

Directory layout:

```
.ai/
  .claude-plugin/
    marketplace.json   ← plugins array is empty
  my-plugin/           ← not listed in marketplace.json
    ...
```

`marketplace.json`:

```json
{
  "plugins": []
}
```

### Correct

```json
{
  "plugins": [
    {
      "name": "my-plugin",
      "source": "./my-plugin"
    }
  ]
}
```

## How to fix

Add an entry to the `plugins` array in `.ai/.claude-plugin/marketplace.json` for every plugin directory under `.ai/`. Each entry must include at least a `source` field pointing to the plugin directory (e.g. `"./my-plugin"`).

Run `aipm init` in the project root to scaffold a valid `marketplace.json` automatically.
