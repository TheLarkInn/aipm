# plugin/missing-manifest

**Severity:** error
**Fixable:** No

Checks that every plugin directory under `.ai/` contains a `.claude-plugin/plugin.json` manifest file. Without this file the plugin cannot be identified by name, version, or author, and `aipm install` will refuse to install it.

The `.claude-plugin` directory itself is exempt from this check.

## Examples

### Incorrect

Directory tree:

```
.ai/
  .claude-plugin/
    marketplace.json
  my-plugin/
    SKILL.md             # plugin.json is missing
```

### Correct

```
.ai/
  .claude-plugin/
    marketplace.json
  my-plugin/
    .claude-plugin/
      plugin.json        # manifest present
    SKILL.md
```

## How to fix

Create a `.claude-plugin/plugin.json` file inside the plugin directory with at minimum the required fields:

```json
{
  "name": "my-plugin",
  "description": "A short description of this plugin",
  "version": "0.1.0",
  "author": {
    "name": "Your Name",
    "email": "you@example.com"
  }
}
```

See [`plugin/required-fields`](./required-fields.md) for the complete list of required fields and their constraints.
