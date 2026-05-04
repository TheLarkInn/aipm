# Engine API Changelog

<!-- Entries are prepended (newest first). -->

## 2026-05-01 — Initial Schema Established

This is the first run of the reverse binary analysis workflow.
Baseline versions recorded; no prior schema existed to diff against.

### claude v2.1.126

| Field | Change |
|-------|--------|
| `settings_paths` | **Initial baseline** — `.claude/settings.json`, `.claude/settings.local.json` |
| `folder_conventions` | **Initial baseline** — `.claude/`, `.claude/skills/`, `~/.claude/skills/` |
| `convention_files` | **Initial baseline** — `CLAUDE.md` (paths: `.`, `.claude`) |
| `tool_calls` | **Initial baseline** — 20 tools: `Task`, `Bash`, `Edit`, `Read`, `Write`, `Glob`, `Grep`, `WebFetch`, `WebSearch`, `TodoWrite`, `mcp`, `list_mcp_resources`, `read_mcp_resource`, `notebook_edit`, `ask_user_question`, `enter_worktree`, `exit_worktree`, `exit_plan_mode`, `task_output`, `task_stop` |
| `size_limits` | **Initial baseline** — `Bash.timeout` max 600000 ms |

### copilot-cli v1.0.40

| Field | Change |
|-------|--------|
| `manifest_search_paths` | **Initial baseline** — `marketplace.json`, `.plugin/marketplace.json`, `.github/plugin/marketplace.json`, `.claude-plugin/marketplace.json` |
| `settings_paths` | **Initial baseline** — `.github/copilot/settings.json`, `.github/copilot/settings.local.json`, `.claude/settings.json`, `.claude/settings.local.json`, `~/.copilot/mcp-config.json` |
| `folder_conventions` | **Initial baseline** — `.github/copilot/`, `.github/extensions/`, `.github/skills/`, `.github/agents/`, `.github/plugin/`, `.github/lsp.json`, `.github/mcp.json`, `.github/copilot-instructions.md`, `.github/instructions/**/*.instructions.md`, `.claude/`, `.claude/skills/`, `.claude-plugin/`, `.agents/`, `.agents/skills/`, `~/.copilot/`, `~/.copilot/extensions/`, `~/.copilot/skills/`, `~/.claude/skills/` |
| `convention_files` | **Initial baseline** — `copilot-instructions.md` (.github), `AGENTS.md` (.), `CLAUDE.md` (., .claude), `GEMINI.md` (.) |
| `manifest_fields` | **Initial baseline** — `name` (max 64, `/^[a-zA-Z0-9-]+$/`), `description` (max 1024), `version`, `author`, `homepage`, `repository`, `license`, `keywords`, `category`, `tags`, `commands`, `agents`, `skills`, `hooks`, `mcpServers`, `lspServers`, `outputStyles`, `logo`, `postInstallMessage` (max 2048), `strict` (default true) |
| `mcp_config.transports` | **Initial baseline** — `stdio`, `sse`, `http` |
| `tool_calls` | **Initial baseline** — 107 tools including `bash`, `glob`, `grep`, `web_fetch`, GitHub API tools (`get_pull_request`, `list_issues`, etc.), browser automation tools (`browser_navigate`, `browser_click`, etc.), Azure/MCP tools (`cosmos`, `keyvault`, `storage`, etc.) |
| `size_limits` | **Initial baseline** — `plugin.name` max 64 chars; `description` max 1024 chars; `postInstallMessage` max 2048 chars; child process `maxBuffer` 1 MB |
| `feature_flags` | **Initial baseline** — `managed-agents-2026-04-01`, `skills-2025-10-02`, `sweagent-capi`, `personal-agents`, `copilot_cli_mcp_allowlist`, `copilot_cli_mcp_enterprise_allowlist`, `copilot_cli_gh_cli_over_mcp`, `copilot_cli_session_based_subagents` |

### Cross-Engine Tool Compatibility (issue #697)

| Classification | Tools |
|----------------|-------|
| **Shared** (both engines) | `bash`, `glob`, `grep`, `web_fetch` |
| **claude-exclusive** | `Task`/`Agent`, `Edit`/`FileEdit`, `Read`/`FileRead`, `Write`/`FileWrite`, `Glob`, `Grep`, `WebFetch`, `WebSearch`, `TodoWrite`, `mcp`, `list_mcp_resources`, `read_mcp_resource`, `notebook_edit`, `ask_user_question`, `enter_worktree`, `exit_worktree`, `exit_plan_mode`, `task_output`, `task_stop` |
| **copilot-cli-exclusive** | `get_file_contents`, `git_apply_patch`, all GitHub API tools, all `browser_*` tools, Azure/cloud tools, `store_memory`, `semantic_issues_search`, `sequentialthinking`, `sql`, `report_intent`, `convert_time`, `get_current_time` |

> **Note:** The `valid-tool-name` lint rule (issue #697) should warn when a plugin with no `engines`
> restriction uses any engine-exclusive tool. See `suggestions` in `engine-api-schema.json` for
> adaptor/detector fixes and concrete test cases.
