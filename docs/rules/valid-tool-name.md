# valid-tool-name

**Severity:** warning (no declared engines) / error (declared engines incompatible)
**Fixable:** No

Checks that every tool name listed in an agent, skill, or hook `tools` frontmatter field is compatible with the plugin's declared engines. Some tools are exclusive to a single AI engine; referencing them in a plugin that targets a different engine means the tool will be silently unavailable at runtime.

This rule consults the schema-driven tool-compatibility tables generated from `crates/libaipm-engine-spec/data/engine-api-schema.json`, which is updated weekly by the `reverse-binary-analysis` workflow.

## How severity is determined

| Situation | Severity | Message |
|---|---|---|
| `engines` not declared in `aipm.toml` (or no `aipm.toml`) and tool is engine-exclusive | **warning** | Suggests adding `engines = [...]` to declare the supported engine |
| `engines` declared and none of them support the referenced tool | **error** | Tool is not supported by any of the declared engines |
| Tool is shared across all engines (`bash`, `glob`, `grep`, `web_fetch`) | (no diagnostic) | Always allowed regardless of engine declarations |
| Tool name not recognised in the compatibility table | (no diagnostic) | Unknown tools are out of scope for this rule |

## Examples

### Incorrect — using a Claude-exclusive tool without declaring engines

```markdown
---
name: task-runner
tools: Task, bash
---
Runs tasks using the Task tool.
```

Output:

```
warning[valid-tool-name]: Tool 'Task' is exclusive to claude; consider declaring engines = ["claude"] in aipm.toml.
```

### Incorrect — using a Claude-exclusive tool while only `copilot-cli` is declared

```markdown
---
name: reviewer
tools: Task, bash
---
Code reviewer.
```

`aipm.toml`:

```toml
[package]
name = "reviewer"
version = "1.0.0"
engines = ["copilot-cli"]
```

Output:

```
error[valid-tool-name]: Tool 'Task' is not supported by any of the declared engines.
```

### Correct — declaring the matching engine

`aipm.toml`:

```toml
[package]
name = "task-runner"
version = "1.0.0"
engines = ["claude"]
```

```markdown
---
name: task-runner
tools: Task, bash
---
Runs tasks using the Task tool.
```

### Correct — using only shared tools (no declaration needed)

```markdown
---
name: finder
tools: bash, glob, grep
---
Finds files in the project.
```

## Engine-exclusive tools

### Claude Code only

`Task` · `Edit` · `Read` · `Write` · `Grep` · `Glob` · `WebSearch` · `WebFetch` ·
`TodoWrite` · `mcp` · `list_mcp_resources` · `read_mcp_resource` · `notebook_edit` ·
`ask_user_question` · `enter_worktree` · `exit_worktree` · `exit_plan_mode` ·
`task_output` · `task_stop`

> **Note:** These are the tool names as of the last weekly binary analysis run. The
> complete, up-to-date list is generated from
> `crates/libaipm-engine-spec/data/engine-api-schema.json`.

### Copilot CLI only

`get_file_contents` · `git_apply_patch` · `search` · `search_code` · `search_issues` ·
`search_repositories` · `search_users` · `get_me` · `get_commit` · `get_tag` ·
`get_pull_request` · `get_pull_request_comments` · `get_pull_request_files` ·
`get_pull_request_reviews` · `get_pull_request_status` · `list_pull_requests` ·
`list_issues` · `issue_read` · `list_branches` · `list_commits` · `list_tags` ·
`list_workflows` · `get_workflow` · `get_workflow_run` · `get_workflow_run_logs` ·
`list_workflow_runs` · `list_workflow_jobs` · `list_workflow_run_artifacts` ·
`summarize_job_log_failures` · `actions_get` · `actions_list` · `actions_run_trigger` ·
`get_code_scanning_alert` · `list_code_scanning_alerts` · `get_secret_scanning_alert` ·
`list_secret_scanning_alerts` · `get_job_logs` · `get_copilot_space` ·
`list_copilot_spaces` · `store_memory` · `semantic_issues_search` ·
`sequentialthinking` · `convert_time` · `get_current_time` · `browser_navigate` ·
`browser_navigate_back` · `browser_click` · `browser_type` · `browser_fill_form` ·
`browser_snapshot` · `browser_take_screenshot` · `browser_tabs` · `browser_close` ·
`browser_install` · `browser_hover` · `browser_drag` · `browser_press_key` ·
`browser_select_option` · `browser_resize` · `browser_evaluate` · `browser_file_upload` ·
`browser_handle_dialog` · `browser_console_messages` · `browser_network_requests` ·
`browser_wait_for` · `sql` · `report_intent`

### Shared (always allowed)

`bash` · `glob` · `grep` · `web_fetch`

## How to fix

1. **Declare the correct engine** — add `engines = ["claude"]` or `engines = ["copilot-cli"]` (or both) to your `[package]` table in `aipm.toml`:

   ```toml
   [package]
   name = "my-plugin"
   version = "1.0.0"
   engines = ["claude"]
   ```

2. **Or replace the tool** — use a shared tool that works across all engines, such as `bash`, `glob`, or `grep`.

3. **Or remove the tool** — if the tool is not actually used, remove it from the `tools` field.

## See also

- [Engine & Platform Compatibility](../guides/engine-platform-compatibility.md) — how to declare engines in `aipm.toml`
- [agent/missing-tools](agent/missing-tools.md) — warns when an agent declares no tools at all
- [Using `aipm lint`](../guides/lint.md) — CLI reference for running the lint system
- [Configuring lint](../guides/configuring-lint.md) — override rule severity or suppress rules per path
