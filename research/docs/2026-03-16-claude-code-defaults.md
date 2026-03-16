---
date: 2026-03-16
researcher: Claude Opus 4.6
branch: main
repository: aipm
topic: "Claude Code Plugin/Configuration Directory Structure and Defaults"
tags: [research, claude-code, plugins, marketplace, configuration, directory-structure]
status: complete
last_updated: 2026-03-16
last_updated_by: Claude Opus 4.6
---

# Claude Code Plugin and Configuration Directory Structure

## Summary

Claude Code uses a layered, scope-based configuration system centered on two key directories: `~/.claude/` (user-global) and `.claude/` (project-local). Plugins are self-contained directories with a `.claude-plugin/plugin.json` manifest, containing components like skills, agents, hooks, MCP servers, and LSP servers. The official Anthropic marketplace (`claude-plugins-official`) is automatically available on first launch. Configuration files follow a strict precedence chain: Managed > CLI args > Local > Project > User.

## 1. Configuration Scopes and Settings Files

Claude Code organizes configuration into four scopes:

| Scope       | Location                                                      | Shared? | Precedence |
|:------------|:--------------------------------------------------------------|:--------|:-----------|
| **Managed** | Server-managed, MDM/registry, or system `managed-settings.json` | Yes (IT) | Highest    |
| **User**    | `~/.claude/` directory                                        | No       | Lowest     |
| **Project** | `.claude/` in repository root                                 | Yes (git) | Mid-high  |
| **Local**   | `.claude/settings.local.json`                                 | No (gitignored) | Mid   |

### Feature-to-file mapping

| Feature         | User location             | Project location                    | Local location                 |
|:----------------|:--------------------------|:------------------------------------|:-------------------------------|
| **Settings**    | `~/.claude/settings.json` | `.claude/settings.json`             | `.claude/settings.local.json`  |
| **Subagents**   | `~/.claude/agents/`       | `.claude/agents/`                   | N/A                            |
| **MCP servers** | `~/.claude.json`          | `.mcp.json`                         | `~/.claude.json` (per-project) |
| **Plugins**     | `~/.claude/settings.json` | `.claude/settings.json`             | `.claude/settings.local.json`  |
| **CLAUDE.md**   | `~/.claude/CLAUDE.md`     | `CLAUDE.md` or `.claude/CLAUDE.md`  | N/A                            |
| **Skills**      | `~/.claude/skills/`       | `.claude/skills/`                   | N/A                            |

Source: [Claude Code Settings](https://code.claude.com/docs/en/settings)

## 2. User-Global Directory (`~/.claude/`)

```
~/.claude/
├── CLAUDE.md                  # Personal instructions for all sessions
├── settings.json              # User-scope settings (permissions, hooks, env, plugins)
├── agents/                    # Personal subagent definitions
│   └── security-reviewer.md
├── skills/                    # Personal skills available across all projects
│   └── explain-code/
│       └── SKILL.md
├── plugins/
│   └── cache/                 # Plugin installation cache (managed by Claude Code)
├── plans/                     # Default plan storage directory
└── ...
```

## 3. Project-Level Directory (`.claude/`)

```
project-root/
├── CLAUDE.md                      # Project instructions (alternative: .claude/CLAUDE.md)
├── .mcp.json                      # Project-scoped MCP server definitions
├── .claude/
│   ├── settings.json              # Project-scope settings (shared via git)
│   ├── settings.local.json        # Local overrides (gitignored)
│   ├── agents/                    # Project subagents
│   │   └── security-reviewer.md
│   ├── skills/                    # Project skills
│   │   └── api-conventions/
│   │       └── SKILL.md
│   └── commands/                  # Legacy commands (still supported, skills preferred)
│       └── deploy.md
```

## 4. Plugin Directory Structure

```
my-plugin/
├── .claude-plugin/              # Metadata directory (optional)
│   └── plugin.json              # Plugin manifest (only file in this dir)
├── commands/                    # Legacy skill markdown files
├── agents/                      # Subagent definitions
├── skills/                      # Skills with SKILL.md structure
│   ├── code-reviewer/
│   │   └── SKILL.md
│   └── pdf-processor/
│       ├── SKILL.md
│       └── reference.md
├── hooks/                       # Hook configurations
│   └── hooks.json
├── settings.json                # Default settings
├── .mcp.json                    # MCP server definitions
├── .lsp.json                    # LSP server configurations
├── scripts/                     # Hook and utility scripts
└── LICENSE
```

## 5. Plugin Manifest Schema (`plugin.json`)

```json
{
  "name": "plugin-name",
  "version": "1.2.0",
  "description": "Brief plugin description",
  "author": { "name": "Author Name" },
  "commands": ["./custom/commands/special.md"],
  "agents": "./custom/agents/",
  "skills": "./custom/skills/",
  "hooks": "./config/hooks.json",
  "mcpServers": "./mcp-config.json",
  "outputStyles": "./styles/",
  "lspServers": "./.lsp.json"
}
```

Only `name` is required if manifest is provided. Custom paths supplement default directories.

## 6. Default Marketplace Setup

When Claude Code is launched for the first time:

1. The **official Anthropic marketplace** (`claude-plugins-official`) is automatically available
2. Users browse via `/plugin` > **Discover** tab
3. Plugins installed with: `/plugin install plugin-name@claude-plugins-official`
4. Official marketplace auto-update is enabled by default

### Adding additional marketplaces

```shell
/plugin marketplace add owner/repo              # GitHub
/plugin marketplace add https://gitlab.com/...  # Git URL
/plugin marketplace add ./local-path            # Local directory
```

Team admins can auto-register marketplaces via `.claude/settings.json`:

```json
{
  "extraKnownMarketplaces": {
    "my-team-tools": {
      "source": {
        "source": "github",
        "repo": "your-org/claude-plugins"
      }
    }
  }
}
```

## 7. Key Conventions for AIPM Workspace Init

- **CLAUDE.md**: Personal/project instructions loaded at session start; supports `@path/to/import` syntax
- **Skills**: `SKILL.md` with YAML frontmatter (name, description, argument-hint, allowed-tools, model, etc.)
- **Hooks**: `hooks/hooks.json` with event-based lifecycle hooks (PreToolUse, PostToolUse, etc.)
- **MCP Servers**: `.mcp.json` at project root; supports `${CLAUDE_PLUGIN_ROOT}` variable
- **LSP Servers**: `.lsp.json` with command, args, extensionToLanguage mapping
- **Subagents**: Markdown files in `agents/` with YAML frontmatter (name, description, tools, model)

## Sources

- [Claude Code Settings](https://code.claude.com/docs/en/settings)
- [Plugins Reference](https://code.claude.com/docs/en/plugins-reference)
- [Create Plugins](https://code.claude.com/docs/en/plugins)
- [Discover and Install Plugins](https://code.claude.com/docs/en/discover-plugins)
- [Skills](https://code.claude.com/docs/en/skills)
- [Best Practices](https://code.claude.com/docs/en/best-practices)
