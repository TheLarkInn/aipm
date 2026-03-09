---
date: 2026-03-09 10:04:50 PDT
researcher: Claude Opus 4.6
git_commit: 9ed90fe83636e78e067b21f37d6fee72492dc0d7
branch: main
repository: aipm
topic: "Agency (Microsoft 1ES) and AI Agent Orchestration"
tags: [research, agency, microsoft, 1es, mcp, authentication]
status: complete
last_updated: 2026-03-09
last_updated_by: Claude Opus 4.6
last_updated_note: "Updated with confirmed Agency details from eng.ms/ES Chat"
---

# Research: Agency (Microsoft 1ES Internal Tool)

## What Agency Is

**Agency** is a Microsoft-internal tool developed by the **1ES (One Engineering System) / StartRight** team. It is **not** an open-source framework or general AI orchestration platform. It is a CLI wrapper that:

1. **Wraps agent CLIs** (Claude Code, GitHub Copilot CLI, VS Code Copilot) so they can use Microsoft-internal services
2. **Provides automatic Azure authentication** using ambient credentials (no manual token management)
3. **Spawns local MCP proxy servers** for each configured MCP server, forwarding JSON-RPC over stdin/stdout

## How It Works

```
Developer Machine
├── Agent CLI (Claude Code / Copilot)
│   └── reads .mcp.json
│       └── MCP server commands invoke Agency
│           └── agency mcp <server> [--flags]
│               ├── Handles Azure auth (ambient credentials)
│               ├── Spawns local MCP proxy
│               └── Forwards JSON-RPC stdin/stdout to remote MCP server
└── Logs: ~/.agency/logs/
```

## MCP Servers Provided via Agency

| Server | Purpose | Example CLI |
|--------|---------|-------------|
| `ado` | Azure DevOps (work items, pipelines, repos) | `agency mcp ado --organization onedrive` |
| `bluebird` | Semantic search for engineering content | `agency mcp bluebird --organization onedrive --project ODSP-Web` |
| `workiq` | Microsoft 365 integration (emails, Teams) | `agency mcp workiq` |
| `es-chat` | Engineering systems knowledge base | `agency mcp es-chat` |
| `msft-learn` | Microsoft Learn documentation | `agency mcp msft-learn` |
| `kusto` | Telemetry and log database queries | `agency mcp kusto --service-uri https://kusto.aria.microsoft.com` |
| `code-companion` | Code Companion remote MCP | `agency mcp remote --url https://codecompanionmcp.azurewebsites.net/mcp` |

## Configuration Format

Agency uses `.mcp.json` files (same format as Claude Code):

```json
{
  "mcpServers": {
    "bluebird": {
      "type": "stdio",
      "command": "dev",
      "args": ["agency", "mcp", "bluebird"]
    },
    "ado": {
      "type": "stdio",
      "command": "dev",
      "args": ["agency", "mcp", "ado", "--organization", "onedrive"]
    },
    "code-companion": {
      "type": "stdio",
      "command": "dev",
      "args": ["agency", "mcp", "remote", "--url", "https://codecompanionmcp.azurewebsites.net/mcp"]
    }
  }
}
```

Key patterns:
- Commands go through `dev agency mcp <server>` (the `dev` CLI is a Microsoft-internal dev tool launcher)
- Each MCP server gets its own proxy process
- Server-specific flags (e.g., `--organization`, `--project`, `--service-uri`) configure the connection
- Configuration files live in plugin directories: `claude-plugins/<plugin-name>/.mcp.json`

## Integration Patterns

- **Claude Code**: Plugin directories with `.mcp.json` referencing `dev agency mcp ...`
- **VS Code Copilot**: Similar MCP configuration adapted for VS Code settings
- **Copilot CLI**: Agency-wrapped MCP servers available to Copilot agents
- **Authentication**: Requires `az login` for initial auth; Agency uses ambient credentials thereafter
- **Logging**: Debug logs at `~/.agency/logs/`

## Implications for AIPM

Agency integration (now P1) means AIPM should:

1. **Generate valid `.mcp.json`** configurations that reference Agency commands
2. **Support Agency-wrapped MCP server declarations** in package manifests
3. **Understand the `dev agency mcp` command pattern** for MCP server proxying
4. **Not duplicate auth** -- delegate authentication entirely to Agency
5. **Support org-specific configuration** (organization, project, service-uri flags)
6. **Allow packages to declare Agency MCP server requirements** without bundling Agency itself

## Sources

- [eng.ms/docs/.../agency](https://eng.ms/docs/coreai/devdiv/one-engineering-system-1es/1es-jacekcz/startrightgitops/agency) (Microsoft internal, requires auth)
- [ODSP-Web Wiki: MCP Servers](https://dev.azure.com/onedrive/ODSP-Web/_wiki/wikis/ODSP-Web.wiki?pagePath=%2FAI+Dev+Tooling%2FMCP+Servers) (Microsoft internal)
- ES Chat query results (2026-03-09)
