# Engine API Changelog

<!-- Entries are prepended (newest first). -->

## 2026-08-25 — claude v2.1.245 (was v2.1.128)

| Field | Change |
|-------|--------|
| `tool_calls` | **Added** 58 tool entries discovered via binary string extraction of the internal tool-name array (`FWo=[...]`): `LS`, `MultiEdit`, `BashOutput`, `KillShell`, `PowerShell`, `Tmux`, `Monitor`, `REPL`, `LSP`, `ReadMcpResourceTool`, `ReadMcpResourceDirTool`, `ListMcpResourcesTool`, `Snip`, `WebBrowser`, `Agent`, `Workflow`, `Skill`, `CronCreate`, `CronDelete`, `CronList`, `ScheduleWakeup`, `RemoteTrigger`, `EnterWorktree`, `ExitWorktree`, `SendMessage`, `SendUserMessage`, `Brief`, `PushNotification`, `SendFeedback`, `SendFile`, `SendUserFile`, `SubscribePR`, `Artifact`, `DesignSync`, `ClaudeDesign`, `Projects`, `ConnectGitHub`, `ReportFindings`, `ObserverReport`, `propose_skills`, `RefreshMcpTools`, `SuggestPluginInstall`, `SuggestConnectors`, `SuggestSkills`, `ListConnectors`, `ListAgents`, `ListPeers`, `SearchMcpRegistry`, `ListPlugins`, `ListSkills`, `SearchPlugins`, `SearchSkills`, `TaskCreate`, `TaskGet`, `TaskList`, `TaskUpdate`, `self_hosted_runner_spawn_local` (enterprise-gated, summarizes 7 sibling `self_hosted_runner_*` tools), `mcp__claude-code-remote` (summarizes a bundled first-party `mcp__github__*` catalog of ~140 GitHub MCP tools) |
| `hook_events` | No changes detected — `PreToolUse`, `PostToolUse`, `Notification`, `Stop`, `SubagentStop`, `SessionStart`, `SessionEnd`, `PreCompact`, `UserPromptSubmit`, etc. all still present verbatim in the binary. |
| `suggestions.claude` | **Added** adaptor-fix, test-case, and behaviour-variant notes covering the new tool constants and the bundled GitHub MCP catalog. |

## 2026-08-25 — copilot-cli v1.0.80 (was v1.0.40)

| Field | Change |
|-------|--------|
| `tool_calls` | **Added** 8 internal editor tool entries found in `app.js`: `read_file`, `str_replace_editor` (aliases `str_replace`, `edit`; sub-commands `view`/`create`/`str_replace`/`edit`), `view`, `create`, `task`, `file_search`, `semantic_search`, `grep_search`. These are distinct from the previously catalogued MCP-exposed `bash`/`glob`/`grep`/`web_fetch` tool names. |
| `suggestions.copilot` | **Added** adaptor-fix and test-case notes covering the internal editor tool constants. |

## 2026-08-25 — Cross-engine tool compatibility recomputed

| Field | Change |
|-------|--------|
| `tool_compatibility.shared_tools` | **Changed** — now empty; the previously recorded `bash`/`glob`/`grep`/`web_fetch` overlap was based on case-insensitive/informal matching, but exact tool-call names differ per engine (e.g. `Bash` vs `bash`, `Glob` vs `glob`). Recomputed by exact case-sensitive name/alias set intersection. |
| `tool_compatibility.engine_exclusive_tools` | **Changed** — regenerated to include all 207 exact tool names now catalogued across both engines (up from the prior smaller list), each flagged with `supported_by`/`unsupported_by`. |

## 2026-05-05 — claude v2.1.128

| Field | Change |
|-------|--------|
| `tool_calls[Task].notes` | **Changed** — `AgentInput` gains four new optional fields: `name` (makes agent addressable via `SendMessage({to: name})`), `team_name` (team context for spawning), `mode` (permission mode: `acceptEdits \| auto \| bypassPermissions \| default \| dontAsk \| plan`), `model` (explicit model override: `sonnet \| opus \| haiku`). |

No other API changes detected for this run.

## 2026-05-05 — copilot-cli v1.0.40

No API changes detected (version unchanged since 2026-05-01).

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
