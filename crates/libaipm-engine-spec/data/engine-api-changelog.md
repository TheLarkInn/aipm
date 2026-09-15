# Engine API Changelog

<!-- Entries are prepended (newest first). -->

## 2026-09-15 — claude v2.1.272 (was v2.1.128)

Source: `claude` binary strings (v2.1.272) and the bundled `sdk-tools.d.ts` SDK type
definitions (json-schema-to-typescript generated), which enumerate `ToolInputSchemas` /
`ToolOutputSchemas` authoritatively.

| Field | Change |
|-------|--------|
| `tool_calls` | **Added** 31 tools discovered in `sdk-tools.d.ts` and binary strings: `Skill`, `TaskOutput`, `TaskCreate`, `TaskGet`, `TaskUpdate`, `TaskList`, `REPL`, `Workflow`, `CronCreate`, `CronDelete`, `CronList`, `ScheduleWakeup`, `RemoteTrigger`, `ShowOnboardingRolePicker`, `ReadNotifications`, `Monitor`, `ProposeSkills`, `ProposeGoal`, `Artifact`, `PushNotification`, `SendFeedback`, `ClaudeDesign`, `Projects`, `EnterPlanMode`, `ReportFindings`, `RefreshMcpTools`, `ReadMcpResourceDir`, `SendUserMessage` (alias `Brief`), `ListAgents` (alias `ListPeers`), plus deprecated aliases `BashOutput`/`AgentOutputTool`/`AgentOutput` (→ `TaskOutput`) and `KillShell`/`KillBash` (→ `TaskStop`). |
| `tool_calls[Task].notes` | **Changed** — `model` now also accepts `fable` in addition to `sonnet \| opus \| haiku` (ignored for `subagent_type: "fork"`, which always inherits the parent model); `isolation` now accepts `remote` in addition to `worktree` (remote always runs in background, gated availability); the previously-added `name`/`team_name`/`mode` fields are now documented as deprecated/ignored — subagents inherit the parent session's permission mode. |
| `folder_conventions` | **Added** — `.claude/agents/`, `.claude/commands/`, `.claude/rules/`, `.claude/workflows/`, `.claude/plugins/`, `.claude/hooks/hooks.json` (all observed as literal path strings in the v2.1.272 binary). |
| `settings_paths` | **Added** — `.claude/scheduled_tasks.json` (created by `CronCreate` with `durable: true`), `.claude/agent-registry.json`. |
| `discovery_notes` | **Added** — note that skills/agents/commands/rules are also discoverable under the new `.claude/agents/`, `.claude/commands/`, `.claude/rules/*.md` conventions. |
| `rule_notes` | **Added** — note on `CronCreate`'s `durable` flag: `true` persists to `.claude/scheduled_tasks.json` (auto-expires after 7 days when `recurring`), `false` is in-memory only and dies with the session. |

## 2026-09-15 — copilot v1.0.83 (was v1.0.40)

Source: bundled `schemas/api.schema.json` (JSON-RPC + type definitions shipped with the
CLI — includes the official `HookType` enum and `Tool`/`BuiltinToolDescriptor` shapes),
the official `changelog.json` shipped with the package, and `app.js` string/table
extraction (tool-name → category and tool-name → display-label lookup tables).

| Field | Change |
|-------|--------|
| `tool_calls` | **Added** 31 tools discovered via `app.js` tool-category/display-label tables and the bundled `schemas/api.schema.json`: `view`, `edit`, `create`, `str_replace_editor` (alias `str_replace`), `apply_patch`, `delete`, `move`, `powershell`, `local_shell`, `shell`, `read_bash`, `write_bash`, `stop_bash`, `task`, `skill`, `manage_schedule`, `session_store_sql`, `read_agent`, `write_agent`, `list_agents`, `ask_user` (alias `ask_user_question`), `web_search`, `github-mcp-server-web_search`, `search_code_subagent`, `update_todo`, `report_progress`, `fetch_copilot_cli_documentation`, `ide_diagnostics`, `ide_selection`, `noop`. |
| `hook_events` | **Added** 6 hook events confirmed via the official `HookType` enum in `schemas/api.schema.json`: `preMcpToolCall`, `userPromptTransformed`, `postResult`, `prePRDescription`, `permissionRequest`, `notification`. |
| `folder_conventions` | **Added** — `~/.copilot/copilot-instructions.md`, `~/.copilot/instructions/**/*.instructions.md`, `.copilot/copilot-instructions.md`, `.copilot/instructions/**/*.instructions.md`, `.github/hooks/*.json`, and (breaking, see below) `com.github.copilot/`, `com.github.copilot/commands/`, `com.github.copilot/agents/`, `com.github.copilot/rules/`, `com.github.copilot/hooks/hooks.json`, `com.github.copilot/lsp.json`, `com.github.copilot/extensions/`. |
| `rule_notes` | **Added, BREAKING (v1.0.80)** — Agent Plugins spec plugins now read `commands/`, `agents/`, `rules/`, `hooks/hooks.json`, `lsp.json`, and `extensions/` only under a `com.github.copilot/` subdirectory of the plugin root — no longer from the plugin root directly. (Extensions were already allowed under `com.github.copilot/extensions/` since v1.0.79.) Confirmed via the package's own `changelog.json`. |
| `rule_notes` | **Added (v1.0.83)** — custom agents can list several models in the `model` manifest field, tried in order until one is available; `model-policy: required` pins model changes to that list. |
| `manifest_fields[agents].notes` | **Changed** — documents that `model` may now be an ordered list of model IDs (fallback chain) gated by `model-policy: required`. |

### Cross-Engine Tool Compatibility Update (issue #697)

`shared_tools` grew from `bash, glob, grep, web_fetch` to also include `edit`, `task`,
`skill`, and `monitor` — these names are now used as literal tool identifiers by **both**
engines. **Caveat:** `monitor` is a false-positive collision — claude's `Monitor` watches a
shell command or WebSocket stream for events, while copilot's `monitor` is the Azure
Monitor MCP tool. Name equality does not imply behavioural equivalence for `monitor`; the
`valid-tool-name` lint rule and any cross-engine tool documentation should treat this pair
as non-interchangeable despite the literal-name match. `edit`, `task`, and `skill` are
closer in intent (file editing, sub-agent dispatch, skill invocation respectively) but
argument shapes still differ per engine and should not be assumed identical.

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
