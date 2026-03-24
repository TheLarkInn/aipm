---
date: 2026-03-24
researcher: Claude Opus 4.6
branch: main
repository: aipm
topic: "Claude Code MCP and LSP Server Configuration Format"
tags: [research, claude-code, mcp, lsp, configuration, json-schema]
status: complete
last_updated: 2026-03-24
last_updated_by: Claude Opus 4.6
---

# Claude Code MCP and LSP Server Configuration

## Summary

MCP servers use `.mcp.json` at the project root with a `{ "mcpServers": { ... } }` wrapper. LSP servers are plugin-only (no standalone project-level file). This means MCP configs can be migrated from existing projects but LSP configs cannot.

## MCP Server Configuration

### File Locations

| Scope | Location | Shared? |
|:------|:---------|:--------|
| Local | `~/.claude.json` (per-project key) | No |
| Project | `.mcp.json` at project root | Yes (git) |
| User | `~/.claude.json` (top-level `mcpServers`) | No |
| Managed | System-level `managed-mcp.json` | Yes (IT) |
| Plugin | `.mcp.json` at plugin root | Via plugin |

**IMPORTANT**: Project MCP config is `.mcp.json` at the project root, NOT `.claude/.mcp.json`.

### JSON Schema

```json
{
  "mcpServers": {
    "<server-name>": {
      "type": "stdio|http|sse",
      "command": "<executable>",
      "args": ["<arg1>"],
      "env": { "<KEY>": "<value>" },
      "cwd": "<working-directory>",
      "url": "<remote-url>",
      "headers": { "<Header>": "<value>" }
    }
  }
}
```

### Field Reference

| Field | Required | Applies To | Description |
|:------|:---------|:-----------|:------------|
| `type` | No (inferred) | all | Transport: `stdio`, `http`, `sse` |
| `command` | stdio | stdio | Executable to run |
| `args` | No | stdio | CLI arguments |
| `env` | No | stdio | Environment variables |
| `cwd` | No | stdio | Working directory |
| `url` | http/sse | http, sse | Remote URL |
| `headers` | No | http, sse | HTTP headers |

### Environment Variable Expansion

`${VAR}` and `${VAR:-default}` supported in `command`, `args`, `env`, `url`, `headers`.

## LSP Server Configuration

### Plugin-Only

LSP servers have NO standalone project-level file. They exist exclusively in plugins:
- `<plugin-root>/.lsp.json`
- Inline in `plugin.json` via `lspServers` field

### JSON Schema

```json
{
  "<language-id>": {
    "command": "<lsp-binary>",
    "args": ["<arg1>"],
    "extensionToLanguage": { ".<ext>": "<language-id>" }
  }
}
```

Note: No wrapper object — language server name is the top-level key directly.

### Required Fields

| Field | Type | Description |
|:------|:-----|:------------|
| `command` | string | LSP binary to execute |
| `extensionToLanguage` | object | Maps file extensions to language IDs |

## Sources

- [Connect Claude Code to tools via MCP](https://code.claude.com/docs/en/mcp)
- [Plugins reference](https://code.claude.com/docs/en/plugins-reference)
