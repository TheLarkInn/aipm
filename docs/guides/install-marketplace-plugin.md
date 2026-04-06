# Installing Plugins from Marketplaces

Install plugins from curated marketplace repositories.

## What is a Marketplace?

A marketplace is a git repository containing a `marketplace.toml` manifest that lists available plugins. Plugins can be sourced from within the marketplace repo or from external git repositories.

## CLI Usage

```bash
# Install from a named marketplace
aipm install market:hello-skills@community

# Install with a specific marketplace ref
aipm install market:hello-skills@community#v2.0

# Install from a GitHub marketplace (owner/repo format)
aipm install market:my-tool@org/marketplace-repo

# Install from a local marketplace (for testing)
aipm install market:my-plugin@./test-fixtures/marketplace
```

## Configuring Marketplaces

Add named marketplaces in `~/.aipm/config.toml`:

```toml
[marketplaces]
community = "github.com/aipm-plugins/marketplace"
internal = "git.company.com/team/plugins"
local-dev = "./my-local-marketplace"
```

## Manifest Usage

In your `aipm.toml`:

```toml
[dependencies]
my-market-plugin = { marketplace = "community", name = "hello-skills", ref = "main" }
```

## Marketplace Manifest Format

Marketplace repositories contain a `marketplace.toml` at engine-specific paths:

- **Claude**: `.claude-plugin/marketplace.toml`
- **Copilot**: `.github/plugin/marketplace.toml`

### Example `marketplace.toml`

```toml
[metadata]
plugin_root = "./plugins"   # optional base directory for relative sources

[[plugins]]
name = "hello-skills"
source = "hello-skills-v1"  # relative path (resolved under plugin_root)
description = "Hello world skill plugin"

[[plugins]]
name = "external-tool"
description = "Tool from an external repo"
[plugins.source]
type = "git"
url = "https://github.com/org/tool-repo.git"
path = "plugins/tool"
ref = "v2.0"
sha = "abc123def456..."
```

### Source Types

| Type | Format | Description |
|------|--------|-------------|
| String | `source = "path"` | Relative path within the marketplace repo |
| Git | `[plugins.source] type = "git"` | External git repository |
| Unsupported | `type = "npm"` / `type = "pip"` | Returns descriptive error |

## Location Formats

| Format | Example | Description |
|--------|---------|-------------|
| GitHub short | `owner/repo` | GitHub repository |
| Full URL | `https://github.com/org/repo` | Any git URL |
| Local path | `./path/to/marketplace` | Filesystem directory |

## Pinning a Marketplace Version

Use `#ref` to pin the marketplace repo to a branch, tag, or commit:

```bash
aipm install market:my-plugin@community#v2.0
aipm install market:my-plugin@org/marketplace#abc123
```

Local marketplace paths do not support `#ref` — `#` is treated as a literal directory character.
