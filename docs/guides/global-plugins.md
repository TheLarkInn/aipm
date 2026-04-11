# Global Plugin Installation

Install plugins globally so they are available across all projects.

## Install Globally

```bash
# Install for all engines
aipm install --global local:./my-plugin
aipm install --global github:org/repo:plugins/my-tool@main

# Install for a specific engine only
aipm install --global --engine claude github:org/repo:my-plugin@main
aipm install --global --engine copilot local:./copilot-tool
```

## Uninstall Globally

```bash
# Full uninstall
aipm uninstall --global local:./my-plugin

# Remove from specific engine only
aipm uninstall --global --engine claude local:./my-plugin

# Use folder name shorthand
aipm uninstall --global my-plugin
```

## List Global Plugins

```bash
aipm list --global
```

## Engine Scoping

- **Empty engines** (`[]`): plugin available to all engines
- **Specific engines** (`["claude"]`): plugin only available to listed engines
- **Additive**: re-installing with a new engine adds it to the existing list
- **Reset**: re-installing without `--engine` resets to all engines

### Examples

```bash
# Start with Claude only
aipm install --global --engine claude local:./my-plugin
# installed.json: engines: ["claude"]

# Add Copilot
aipm install --global --engine copilot local:./my-plugin
# installed.json: engines: ["claude", "copilot"]

# Reset to all engines
aipm install --global local:./my-plugin
# installed.json: engines: [] (all)

# Remove just Claude
aipm uninstall --global --engine claude local:./my-plugin
# installed.json: engines: ["copilot"]
```

## Name Conflict Rules

Two different plugin sources with the same folder name cannot be installed for overlapping engines:

```bash
# This works (non-overlapping engines):
aipm install --global --engine claude github:org/repo:my-plugin@main
aipm install --global --engine copilot local:./my-plugin

# This fails (overlapping — both target all engines):
aipm install --global github:org/repo:my-plugin@main
aipm install --global local:./my-plugin
# Error: Plugin name conflict for 'my-plugin'
```

## Registry File

Global plugins are stored in `~/.aipm/installed.json`:

```json
{
  "plugins": [
    {
      "spec": "github:org/repo:my-plugin@main",
      "engines": ["claude"],
      "cache_policy": "no-refresh",
      "cache_ttl_secs": 86400
    }
  ]
}
```

## Per-Plugin Cache Policy

```bash
# Install with custom cache policy
aipm install --global --plugin-cache no-refresh github:org/repo:plugin@main
```

The cache policy is stored per-plugin and applies whenever the plugin is used.

---

See also: [`aipm install`](../../README.md#aipm-install), [`docs/guides/install-git-plugin.md`](./install-git-plugin.md), [`docs/guides/install-marketplace-plugin.md`](./install-marketplace-plugin.md), [`docs/guides/cache-management.md`](./cache-management.md), [`docs/guides/source-security.md`](./source-security.md).
