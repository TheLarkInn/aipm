# plugin/missing-registration

**Severity:** error
**Fixable:** No

Checks that every plugin directory under `.ai/` is listed in `.ai/.claude-plugin/marketplace.json`. An unregistered plugin directory is invisible to `aipm install`, `aipm list`, and other commands that read the marketplace registry.

The `.claude-plugin` directory itself is exempt from this check.

## Examples

### Incorrect

Directory tree:

```
.ai/
  .claude-plugin/
    marketplace.json     # does NOT list "my-new-plugin"
  my-new-plugin/
    .claude-plugin/
      plugin.json
    SKILL.md
```

`marketplace.json`:

```json
{
  "plugins": [
    {
      "name": "other-plugin",
      "source": "./other-plugin",
      "description": "Already registered"
    }
  ]
}
```

### Correct

```json
{
  "plugins": [
    {
      "name": "other-plugin",
      "source": "./other-plugin",
      "description": "Already registered"
    },
    {
      "name": "my-new-plugin",
      "source": "./my-new-plugin",
      "description": "Newly created plugin"
    }
  ]
}
```

## How to fix

Add an entry for the unregistered plugin to the `plugins` array in `.ai/.claude-plugin/marketplace.json`:

```json
{
  "name": "<plugin-directory-name>",
  "source": "./<plugin-directory-name>",
  "description": "<short description>"
}
```

Alternatively, run `aipm migrate` to regenerate the marketplace registry from the current `.ai/` directory layout.
