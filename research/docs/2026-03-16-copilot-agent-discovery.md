---
date: 2026-03-16
researcher: Claude Opus 4.6
branch: main
repository: aipm
topic: "GitHub Copilot agent/plugin discovery and configuration model"
tags: [research, copilot, agents, plugins, marketplace, configuration]
status: complete
last_updated: 2026-03-16
last_updated_by: Claude Opus 4.6
---

# GitHub Copilot Agent/Plugin Discovery and Configuration

## Summary

GitHub Copilot's agent/extension discovery model is fundamentally different from Claude Code's. Copilot relies on **file-based convention** (`.github/agents/` with `.agent.md` files) for custom agents, and a separate **plugin system** (preview) with Git-repository-based marketplaces for distributable packages.

## 1. Agent Discovery Locations

**VS Code (in priority order):**
- `.github/agents/` in the workspace root (primary)
- `.claude/agents/` (cross-tool compatibility)
- `~/.copilot/agents/` user home
- `agents/` in VS Code profile directory
- Additional paths via `chat.agentFilesLocations` setting

**Copilot CLI:**
- `.github/agents/` in the current repository
- `~/.copilot/agents/` user home

**GitHub.com (Coding Agent):**
- `.github/agents/` in the current repository
- `agents/` in the org's `.github` or `.github-private` repository

## 2. Configuration Keys

### VS Code `settings.json`

| Key | Purpose |
|-----|---------|
| `chat.agentFilesLocations` | Array of additional directory paths to scan for `.agent.md` files |
| `chat.plugins.marketplaces` | Array of Git repo references serving as plugin marketplaces |

### Copilot CLI

| File | Purpose |
|------|---------|
| `~/.copilot/config.json` | User-level CLI config (trusted folders, permissions) |
| `.copilot/mcp-config.json` | Repository-level MCP server configuration |
| `COPILOT_HOME` env var | Overrides default `~/.copilot` config directory |

## 3. Agent Manifest Format (`.agent.md`)

```markdown
---
description: "Security-focused code reviewer"
tools:
  - read
  - edit
  - search
model: gpt-4o
user-invocable: true
---

You are a security-focused code review agent...
```

Frontmatter keys: `name`, `description`, `tools`, `model`, `target`, `user-invocable`, `disable-model-invocation`, `mcp-servers`, `agents`, `handoffs`, `metadata`.

## 4. Plugin Manifest (`plugin.json`)

For distributable plugins (preview), at `.github/plugin.json`:

```json
{
  "name": "code-reviewer",
  "description": "Security-focused code review plugin",
  "version": "1.0.0",
  "author": "org-name"
}
```

## 5. Marketplace Registry (`marketplace.json`)

At `.github/plugin/marketplace.json` in a Git repository:

```json
{
  "name": "team-marketplace",
  "plugins": [
    {
      "name": "code-reviewer",
      "version": "1.0.0",
      "source": "./plugins/code-reviewer"
    }
  ]
}
```

## 6. MCP Server Configuration

`.copilot/mcp-config.json`:

```json
{
  "mcpServers": {
    "server-name": {
      "type": "local",
      "command": "uvx",
      "args": ["--from", "package-name", "server-command"],
      "tools": ["*"]
    }
  }
}
```

## 7. Key Differences from Claude Code

| Concept | Claude Code | GitHub Copilot |
|---------|------------|----------------|
| Project config dir | `.claude/` | `.github/agents/` + `.copilot/` |
| Agent format | Markdown | `.agent.md` (Markdown + YAML) |
| Marketplace setting | `extraKnownMarketplaces` | `chat.plugins.marketplaces` |
| Enabled plugins | `enabledPlugins` | Auto-discover or install via CLI |
| MCP config | `.mcp.json` | `.copilot/mcp-config.json` |
| Additional scan dirs | N/A | `chat.agentFilesLocations` |

## Sources

- [Custom agents in VS Code](https://code.visualstudio.com/docs/copilot/customization/custom-agents)
- [Custom agents configuration - GitHub Docs](https://docs.github.com/en/copilot/reference/custom-agents-configuration)
- [Creating custom agents for Copilot CLI](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/create-custom-agents-for-cli)
- [Configure GitHub Copilot CLI](https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/configure-copilot-cli)
- [Creating Agent Plugins - Ken Muse](https://www.kenmuse.com/blog/creating-agent-plugins-for-vs-code-and-copilot-cli/)
