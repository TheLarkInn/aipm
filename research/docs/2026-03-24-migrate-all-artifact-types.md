---
date: 2026-03-24 07:30:00 PDT
researcher: Claude Opus 4.6
git_commit: 732c72b2ad574f48808a89236e624c5a2650053f
branch: main
repository: aipm
topic: "Extending aipm migrate to support all Claude Code plugin artifact types"
tags: [research, codebase, migrate, agents, mcp, lsp, hooks, output-styles, settings, detector, plugin]
status: complete
last_updated: 2026-03-24
last_updated_by: Claude Opus 4.6
---

# Research: Extending `aipm migrate` to All Artifact Types

## Research Question

Now that we've built out command and skill migrations, what needs to happen to build out all the other types of plugin artifacts (MCPs, LSPs, output styles, agents, hooks, settings, scripts)?

## Summary

The existing `aipm migrate` pipeline uses a Scanner-Detector-Emitter architecture that was designed for extensibility. Currently only `SkillDetector` and `CommandDetector` exist. The `Components` struct in `manifest/types.rs` already declares fields for all 9 component types (skills, commands, agents, hooks, mcp_servers, lsp_servers, scripts, output_styles, settings), and the `PluginType` enum covers skill, agent, mcp, hook, lsp, and composite. Six new detector modules are needed to complete the migration story. Each has a distinct source location, file format, and emission strategy.

This document maps every artifact type to its source location in `.claude/`, its file format and schema, the required `ArtifactKind` variant, emission behavior, and changes needed across the 7 pipeline extension points.

---

## 1. Artifact Type Inventory

### Source Locations in `.claude/` Directory

| Artifact Type   | Source Location                       | File Format       | Single/Multi File | Standalone Plugin Type |
|:----------------|:--------------------------------------|:------------------|:-------------------|:-----------------------|
| **Skills**      | `.claude/skills/<name>/SKILL.md`      | Markdown + YAML   | Directory          | `skill` |
| **Commands**    | `.claude/commands/<name>.md`          | Markdown          | Single file        | `skill` (converted) |
| **Agents**      | `.claude/agents/<name>.md`            | Markdown + YAML   | Single file        | `agent` |
| **MCP Servers** | `.mcp.json` (project root, NOT `.claude/`) | JSON          | Single file, multi-server | `mcp` |
| **Hooks**       | `.claude/settings.json` → `hooks` key | JSON (embedded)   | Embedded in settings | `hook` |
| **Settings**    | `.claude/settings.json`               | JSON              | Single file        | N/A (composite only) |
| **Output Styles**| `.claude/output-styles/<name>.md`    | Markdown + YAML   | Single file        | N/A (composite only) |
| **LSP Servers** | No standalone project-level file      | N/A               | N/A                | `lsp` |
| **Scripts**     | No standalone location                | Various           | Referenced by skills | N/A (bundled with skills) |

### Key Architectural Insight

Not all artifact types are equal. They fall into three categories:

1. **Directory-based artifacts** (skills, agents): Each artifact is a file/directory that maps cleanly to one plugin.
2. **File-based configs** (MCP servers, hooks, settings, output styles): A single config file may contain multiple logical artifacts (e.g., `.mcp.json` has multiple servers), or represent a single shared config.
3. **Plugin-only artifacts** (LSP servers): No standalone project-level source exists — these only live inside plugins already. **Cannot be migrated from `.claude/`.**

---

## 2. Detailed Artifact Type Analysis

### 2.1 Agents (`.claude/agents/*.md`)

**Source**: `.claude/agents/` directory, one `.md` file per agent.

**File Format**: Markdown with YAML frontmatter followed by system prompt body.

**Frontmatter Fields** (from official docs):

| Field             | Required | Type    | Description |
|:------------------|:---------|:--------|:------------|
| `name`            | Yes      | string  | Unique identifier (lowercase + hyphens) |
| `description`     | Yes      | string  | When Claude should delegate to this agent |
| `tools`           | No       | string  | Comma-separated tool list (inherits all if omitted) |
| `disallowedTools` | No       | string  | Tools to deny |
| `model`           | No       | string  | `sonnet`, `opus`, `haiku`, full ID, or `inherit` |
| `permissionMode`  | No       | string  | `default`, `acceptEdits`, `dontAsk`, `bypassPermissions`, `plan` |
| `maxTurns`        | No       | number  | Max agentic turns |
| `skills`          | No       | string  | Skills to preload into context |
| `mcpServers`      | No       | list    | MCP servers (inline or by name reference) |
| `hooks`           | No       | object  | Lifecycle hooks scoped to this agent |
| `memory`          | No       | string  | `user`, `project`, or `local` |
| `background`      | No       | bool    | Always run in background |
| `effort`          | No       | string  | `low`, `medium`, `high`, `max` |
| `isolation`        | No       | string  | `worktree` for isolated git worktree |

**Example Source File** (`.claude/agents/security-reviewer.md`):
```markdown
---
name: security-reviewer
description: Reviews code for security vulnerabilities and best practices
tools: Read, Grep, Glob, Bash
model: sonnet
---

You are a security code reviewer. Analyze code for OWASP top 10 vulnerabilities...
```

**Plugin Restrictions**: Plugin-shipped agents do NOT support `hooks`, `mcpServers`, or `permissionMode` (stripped for security).

**Detection Strategy**: Scan `.claude/agents/` for `.md` files. Parse YAML frontmatter for `name` and `description`. The markdown body becomes the agent's system prompt.

**Emission**: Copy each `.md` file to `.ai/<plugin-name>/agents/<name>.md`. Generate `aipm.toml` with `type = "agent"` and `[components] agents = ["agents/<name>.md"]`.

---

### 2.2 MCP Servers (`.mcp.json` at project root)

**Source**: `.mcp.json` at the **project root** (NOT `.claude/.mcp.json`).

**IMPORTANT**: This file is outside `.claude/`. The detector must scan the project root, not the `.claude/` subdirectory. This is a departure from other detectors which scan within `.claude/`.

**File Format**: JSON with `mcpServers` wrapper object.

**Schema**:
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

**Key Fields Per Server**:

| Field     | Required      | Type     | Description |
|:----------|:------------- |:---------|:------------|
| `type`    | No (inferred) | string   | Transport: `stdio`, `http`, `sse` |
| `command` | stdio         | string   | Executable to run |
| `args`    | No            | string[] | CLI arguments |
| `env`     | No            | object   | Environment variables |
| `cwd`     | No            | string   | Working directory |
| `url`     | http/sse      | string   | Remote server URL |
| `headers` | No            | object   | HTTP headers |

**Detection Strategy**: Two options:

- **Option A (one plugin per server)**: Parse `.mcp.json`, extract each server entry as a separate `Artifact`. Each becomes its own plugin with a single MCP server config.
- **Option B (one plugin for all servers)**: Treat the entire `.mcp.json` as one artifact. Emit a single composite plugin containing all MCP servers.

Option A aligns better with the existing pattern where each artifact becomes a plugin. However, MCP servers often work together (e.g., database + API), so Option B may be more practical. The detector could emit both options and let the user choose (or default to B for simplicity).

**Emission**: Copy `.mcp.json` to `.ai/<plugin-name>/.mcp.json`. For per-server plugins, generate a filtered `.mcp.json` containing only that server's entry. Rewrite any absolute paths to use `${CLAUDE_PLUGIN_ROOT}`. Generate `aipm.toml` with `type = "mcp"` and `[components] mcp_servers = [".mcp.json"]`.

**Special Considerations**:
- Environment variable references (`${VAR}`) in the source should be preserved.
- `${CLAUDE_PLUGIN_ROOT}` and `${CLAUDE_PLUGIN_DATA}` should be used for any relative file paths in the emitted config.
- The source file may reference local executables that need to be included or documented.

---

### 2.3 Hooks (embedded in `.claude/settings.json`)

**Source**: `.claude/settings.json` under the `"hooks"` key. NOT a separate file.

**IMPORTANT**: Hooks at the project level live inside `settings.json`, not in a standalone `hooks/hooks.json`. The `hooks/hooks.json` format only exists inside **plugins**.

**Format** (within settings.json):
```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "./scripts/validate.sh"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "./scripts/format.sh"
          }
        ]
      }
    ]
  }
}
```

**Hook Event Types** (22 total): `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `PermissionRequest`, `Notification`, `SubagentStart`, `SubagentStop`, `Stop`, `StopFailure`, `TeammateIdle`, `TaskCompleted`, `InstructionsLoaded`, `ConfigChange`, `WorktreeCreate`, `WorktreeRemove`, `PreCompact`, `PostCompact`, `Elicitation`, `ElicitationResult`.

**Hook Handler Types**: `command`, `http`, `prompt`, `agent`.

**Handler Fields**:

| Field           | Type    | Description |
|:----------------|:--------|:------------|
| `type`          | string  | Handler type: `command`, `http`, `prompt`, `agent` |
| `command`       | string  | Shell command (for `command` type) |
| `url`           | string  | Endpoint URL (for `http` type) |
| `prompt`        | string  | LLM prompt with `$ARGUMENTS` (for `prompt` type) |
| `timeout`       | number  | Timeout in milliseconds |
| `async`         | bool    | Run asynchronously |
| `statusMessage` | string  | Message shown during execution |
| `once`          | bool    | Run only once per session |

**Detection Strategy**: Read `.claude/settings.json`, parse JSON, extract the `"hooks"` key. If hooks exist, create one `Artifact` representing all hooks. Referenced scripts (in `command` fields) should be tracked as `referenced_scripts`.

**Emission**: Convert the hooks JSON into the plugin `hooks/hooks.json` format (which wraps with `{ "hooks": { ... } }`). Copy any referenced scripts to `scripts/`. Generate `aipm.toml` with `type = "hook"` and `[components] hooks = ["hooks/hooks.json"]`.

**Special Considerations**:
- Script paths in `command` fields may be relative to the project root. These need rewriting to `${CLAUDE_PLUGIN_ROOT}/scripts/...`.
- The `settings.json` file contains more than just hooks. The detector must extract ONLY the hooks section.

---

### 2.4 Settings (`.claude/settings.json`)

**Source**: `.claude/settings.json` (the entire file, minus hooks which are handled separately).

**Format**: JSON with many top-level keys:
```json
{
  "$schema": "https://json.schemastore.org/claude-code-settings.json",
  "permissions": {
    "allow": ["Read", "Glob", "Grep"],
    "deny": ["Agent(Explore)"]
  },
  "env": { "NODE_ENV": "development" },
  "model": "claude-sonnet-4-6",
  "outputStyle": "explanatory",
  "agent": "code-reviewer"
}
```

**Key Settings Fields**: `permissions`, `env`, `hooks` (handled separately), `model`, `outputStyle`, `agent`, `sandbox`, `attribution`, `extraKnownMarketplaces`, etc.

**Detection Strategy**: Read `.claude/settings.json`. If it contains keys beyond just `hooks` (permissions, env, model, etc.), create an `Artifact` for the settings. The plugin `settings.json` currently **only supports the `agent` field** per official docs. This means most settings cannot actually be distributed via plugins today.

**Emission**: Copy relevant fields to `.ai/<plugin-name>/settings.json`. Generate `aipm.toml` with `[components] settings = ["settings.json"]`. Mark as composite if bundled with other artifacts.

**Special Considerations**:
- Plugin `settings.json` is very limited — only `agent` settings are currently supported by Claude Code.
- Permissions, env vars, and other settings from the source file cannot be expressed in a plugin's `settings.json`.
- This artifact type may be best treated as a **documentation-only** migration that generates a README noting which settings need manual configuration.

---

### 2.5 Output Styles (`.claude/output-styles/*.md`)

**Source**: `.claude/output-styles/` directory, one `.md` file per style.

**File Format**: Markdown with YAML frontmatter.

**Frontmatter Fields**:

| Field                     | Required | Type   | Description |
|:--------------------------|:---------|:-------|:------------|
| `name`                    | Yes      | string | Display name for the style |
| `description`             | No       | string | What this style does |
| `keep-coding-instructions`| No       | bool   | Whether to keep default coding instructions (default: false) |

**Example Source File** (`.claude/output-styles/concise.md`):
```markdown
---
name: concise
description: Minimal output with no explanations
keep-coding-instructions: true
---

Be extremely concise. No preamble, no explanations, just code.
```

**Detection Strategy**: Scan `.claude/output-styles/` for `.md` files. Parse YAML frontmatter for `name` and `description`.

**Emission**: Copy `.md` files to `.ai/<plugin-name>/output-styles/<name>.md` (or a custom path). Declare in `plugin.json` via `"outputStyles": "./output-styles/"`. Generate `aipm.toml` with `[components] output_styles = ["output-styles/<name>.md"]`.

**Note**: Output styles as a plugin component use the `outputStyles` field in `plugin.json`. The `PluginType` enum does NOT have an `OutputStyle` variant — output styles are always part of a `composite` plugin or bundled with other components.

---

### 2.6 LSP Servers — NOT Migratable

**LSP servers have no standalone project-level configuration.** They exist exclusively within the plugin system (`.lsp.json` at the plugin root). There is no `.claude/.lsp.json` or project-root `.lsp.json` to migrate from.

**Conclusion**: No `LspDetector` is needed for the `.claude/` migration. LSP plugins are created directly, not migrated from existing configs.

---

### 2.7 Scripts — Not a Standalone Artifact

Scripts are not independently discoverable artifacts. They are referenced by skills (via `${CLAUDE_SKILL_DIR}/scripts/`) and hooks (via `command` fields). The existing pipeline already handles script copying as part of skill and command emission.

**Conclusion**: No `ScriptDetector` is needed. Scripts continue to be handled as referenced files within skill and hook artifacts.

---

## 3. Extension Point Changes Required

### 3.1 `ArtifactKind` Enum (`migrate/mod.rs`)

**Current**:
```rust
pub enum ArtifactKind {
    Skill,
    Command,
}
```

**Proposed**:
```rust
pub enum ArtifactKind {
    Skill,
    Command,
    Agent,
    McpServer,
    Hook,
    Settings,
    OutputStyle,
}
```

**`to_type_string()` mapping**:

| Variant       | `to_type_string()` | Matches `PluginType` |
|:--------------|:--------------------|:---------------------|
| `Skill`       | `"skill"`           | Yes |
| `Command`     | `"skill"`           | Yes (converted) |
| `Agent`       | `"agent"`           | Yes |
| `McpServer`   | `"mcp"`             | Yes |
| `Hook`        | `"hook"`            | Yes |
| `Settings`    | `"composite"`       | Yes (settings alone aren't a type) |
| `OutputStyle`  | `"composite"`       | Yes (output styles alone aren't a type) |

### 3.2 `ArtifactMetadata` Struct (`migrate/mod.rs`)

**Current**:
```rust
pub struct ArtifactMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
    pub hooks: Option<String>,
    pub model_invocation_disabled: bool,
}
```

The existing fields are skill/command-specific. New artifact types need different metadata. Options:

**Option A — Extend with optional fields**:
```rust
pub struct ArtifactMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
    // Skill/command-specific
    pub hooks: Option<String>,
    pub model_invocation_disabled: bool,
    // Agent-specific
    pub model: Option<String>,
    pub tools: Option<String>,
    // MCP-specific
    pub server_config: Option<String>,  // raw JSON of the server entry
    // Generic
    pub raw_content: Option<String>,    // raw file content for pass-through
}
```

**Option B — Use raw_content for non-skill types**: Keep existing fields for skills/commands. For other types, store the original file content in `raw_content` and let the emitter handle formatting. This is simpler and avoids proliferating type-specific fields.

**Recommended**: Option B. Most non-skill artifacts are config files that should be copied with minimal transformation.

### 3.3 Detector Registry (`migrate/detector.rs`)

**Current**:
```rust
pub fn claude_detectors() -> Vec<Box<dyn Detector>> {
    vec![
        Box::new(super::skill_detector::SkillDetector),
        Box::new(super::command_detector::CommandDetector),
    ]
}
```

**Proposed**:
```rust
pub fn claude_detectors() -> Vec<Box<dyn Detector>> {
    vec![
        Box::new(super::skill_detector::SkillDetector),
        Box::new(super::command_detector::CommandDetector),
        Box::new(super::agent_detector::AgentDetector),
        Box::new(super::hook_detector::HookDetector),
        Box::new(super::output_style_detector::OutputStyleDetector),
        Box::new(super::settings_detector::SettingsDetector),
    ]
}
```

**MCP detector is special**: Because `.mcp.json` lives at the project root (not inside `.claude/`), the `McpDetector` cannot use the same `source_dir` pattern. Options:
- Pass the project root to `McpDetector` instead of the `.claude/` dir.
- Have the MCP detector derive the project root from the source dir (go up one level from `.claude/`).
- Add a separate detector list for project-root-level configs.

**New module files**:
- `crates/libaipm/src/migrate/agent_detector.rs`
- `crates/libaipm/src/migrate/mcp_detector.rs`
- `crates/libaipm/src/migrate/hook_detector.rs`
- `crates/libaipm/src/migrate/settings_detector.rs`
- `crates/libaipm/src/migrate/output_style_detector.rs`

### 3.4 Emitter (`migrate/emitter.rs`)

The emitter currently only handles `Skill` and `Command` variants. Each new `ArtifactKind` needs emission logic.

**New emission functions needed**:

| Function | Artifact Kind | Behavior |
|:---------|:-------------|:---------|
| `emit_agent_files()` | `Agent` | Copy `.md` file to `agents/<name>.md` |
| `emit_mcp_config()` | `McpServer` | Copy/filter `.mcp.json` to `.mcp.json` at plugin root |
| `emit_hooks_config()` | `Hook` | Convert settings.json hooks to plugin `hooks/hooks.json` format |
| `emit_settings_file()` | `Settings` | Copy supported settings fields to `settings.json` |
| `emit_output_style()` | `OutputStyle` | Copy `.md` file to output-styles directory |

**Match sites to update** (3 locations in emitter.rs):
- `emit_plugin()` lines 61-68
- `emit_plugin_with_name()` lines 263-270
- `emit_package_plugin()` lines 360-376

**Manifest generation changes**:
- `generate_plugin_manifest()` must produce correct `[components]` entries per artifact kind (e.g., `agents = [...]`, `mcp_servers = [...]`).
- `generate_package_manifest()` must track `has_agent`, `has_mcp`, `has_hook`, etc. for composite type determination.
- The composite type logic (currently `has_skill && has_command`) must be generalized to detect any mix of 2+ different component types.

### 3.5 Dry-Run Report (`migrate/dry_run.rs`)

**New sections needed** in the report:
- `## Agents` — list detected agent definitions
- `## MCP Servers` — list detected MCP server configs
- `## Hooks` — list detected hook configurations
- `## Output Styles` — list detected output styles
- `## Settings` — list detected settings (with warning about limited plugin support)

**Count accumulators**: The recursive report's fold currently counts `(skills, commands)`. Must be extended to count all artifact types.

**Composite type logic**: Must be generalized from `has_skill && has_command` to detect any 2+ different types.

### 3.6 Registrar (`migrate/registrar.rs`)

**No changes needed.** The registrar is entirely artifact-type-agnostic. It takes plugin names and appends them to `marketplace.json`. The description could optionally be made type-aware (e.g., "Migrated MCP server from .mcp.json") but this is cosmetic.

### 3.7 `Components` Struct and `PluginType` Enum (`manifest/types.rs`)

**`Components` struct — No changes needed.** Already declares all 9 component types.

**`PluginType` enum — Minor addition needed.** Currently has: `Skill`, `Agent`, `Mcp`, `Hook`, `Lsp`, `Composite`. Does NOT have `OutputStyle`, `Settings`, or `Script` as standalone types. Since output styles and settings aren't standalone plugin types in Claude Code's model, they correctly map to `Composite` when bundled. No changes strictly required.

---

## 4. Implementation Priority

Based on user value and implementation complexity:

### Tier 1 — High Value, Clean Mapping
1. **AgentDetector** — Clean directory-based pattern identical to skills. Markdown + YAML frontmatter. Each `.md` file → one plugin.
2. **McpDetector** — High user value (MCP is heavily used). Single JSON file, well-defined schema. Architectural question about project-root scanning.

### Tier 2 — Medium Value, Extraction Required
3. **HookDetector** — Medium complexity. Must parse `settings.json` and extract just the `hooks` key. Script path rewriting needed.
4. **OutputStyleDetector** — Simple directory scan like agents. Low complexity but lower user value.

### Tier 3 — Low Priority / Limited Plugin Support
5. **SettingsDetector** — Plugin `settings.json` currently only supports the `agent` field. Most settings cannot be expressed in a plugin. Consider documentation-only migration.

### Not Needed
- **LspDetector** — No standalone source to migrate from.
- **ScriptDetector** — Scripts are bundled with skills/hooks, not standalone.

---

## 5. MCP Detector Architectural Decision

The MCP detector is the most architecturally interesting because `.mcp.json` lives at the project root, not inside `.claude/`. Three approaches:

### Approach A: Modify Detector trait to accept project root
Add an optional `project_root` parameter to the `Detector` trait or `detect()` method. MCP detector uses it; others ignore it.

**Pro**: Clean, explicit. **Con**: Changes the trait signature for all detectors.

### Approach B: MCP detector derives project root from source_dir
Since `source_dir` is `.claude/`, the MCP detector can call `source_dir.parent()` to get the project root and look for `.mcp.json` there.

**Pro**: No trait change. **Con**: Implicit coupling to directory naming.

### Approach C: Separate project-root detector list
Have a second detector list for project-root-level configs. The orchestrator runs both lists.

**Pro**: Clean separation. **Con**: More orchestrator complexity.

**Recommendation**: Approach B. The `.claude/` → project root derivation is simple, and the `detect()` method already receives the full source path. This avoids trait changes while keeping the solution contained to the MCP detector module.

---

## 6. Plugin Directory Structure Per Artifact Type

### Agent Plugin
```
.ai/security-reviewer/
├── .claude-plugin/
│   └── plugin.json
├── agents/
│   └── security-reviewer.md
└── aipm.toml
```

### MCP Server Plugin
```
.ai/my-mcp-servers/
├── .claude-plugin/
│   └── plugin.json
├── .mcp.json
├── scripts/            # if servers reference local scripts
│   └── start-server.sh
└── aipm.toml
```

### Hook Plugin
```
.ai/project-hooks/
├── .claude-plugin/
│   └── plugin.json
├── hooks/
│   └── hooks.json
├── scripts/            # referenced by hook commands
│   └── validate.sh
└── aipm.toml
```

### Output Style Plugin
```
.ai/concise-style/
├── .claude-plugin/
│   └── plugin.json
├── output-styles/
│   └── concise.md
└── aipm.toml
```

### Composite Plugin (package-scoped with mixed types)
```
.ai/my-team-config/
├── .claude-plugin/
│   └── plugin.json
├── agents/
│   └── reviewer.md
├── skills/
│   └── deploy/
│       └── SKILL.md
├── hooks/
│   └── hooks.json
├── .mcp.json
├── output-styles/
│   └── team-style.md
├── scripts/
│   └── format.sh
└── aipm.toml          # type = "composite"
```

---

## Code References

### Current Implementation (to extend)
- `crates/libaipm/src/migrate/mod.rs:17-32` — `ArtifactKind` enum, `to_type_string()`
- `crates/libaipm/src/migrate/mod.rs:35-45` — `ArtifactMetadata` struct
- `crates/libaipm/src/migrate/detector.rs:23-28` — `claude_detectors()` registry
- `crates/libaipm/src/migrate/emitter.rs:61-68` — `emit_plugin()` match on `ArtifactKind`
- `crates/libaipm/src/migrate/emitter.rs:263-270` — `emit_plugin_with_name()` match
- `crates/libaipm/src/migrate/emitter.rs:360-376` — `emit_package_plugin()` match
- `crates/libaipm/src/migrate/emitter.rs:567-609` — `generate_plugin_manifest()`
- `crates/libaipm/src/migrate/emitter.rs:446-491` — `generate_package_manifest()`
- `crates/libaipm/src/migrate/dry_run.rs:24-25` — Artifact filtering by kind
- `crates/libaipm/src/manifest/types.rs:115-143` — `Components` struct (already complete)
- `crates/libaipm/src/manifest/types.rs:207-239` — `PluginType` enum

### Existing Detectors (patterns to follow)
- `crates/libaipm/src/migrate/skill_detector.rs` — directory-based detector with frontmatter parsing
- `crates/libaipm/src/migrate/command_detector.rs` — file-based detector with frontmatter parsing

### New Files to Create
- `crates/libaipm/src/migrate/agent_detector.rs`
- `crates/libaipm/src/migrate/mcp_detector.rs`
- `crates/libaipm/src/migrate/hook_detector.rs`
- `crates/libaipm/src/migrate/output_style_detector.rs`
- `crates/libaipm/src/migrate/settings_detector.rs`

---

## Architecture Documentation

### Claude Code Plugin Component Model

From the official [Plugins Reference](https://code.claude.com/docs/en/plugins-reference):

```
plugin-root/
├── .claude-plugin/           # Metadata (optional)
│   └── plugin.json
├── commands/                 # Legacy skills
├── agents/                   # Subagent .md files
├── skills/                   # Skills with SKILL.md
│   └── <name>/
│       └── SKILL.md
├── hooks/                    # Hook configurations
│   └── hooks.json
├── settings.json             # Plugin settings (only `agent` field supported)
├── .mcp.json                 # MCP server definitions
├── .lsp.json                 # LSP server configs
├── output-styles/            # Custom output styles (undocumented in plugin ref)
├── scripts/                  # Utility scripts
└── LICENSE
```

**`plugin.json` Component Path Fields**:

| Field          | Type                  | Default Location |
|:---------------|:----------------------|:-----------------|
| `commands`     | string\|array         | `commands/` |
| `agents`       | string\|array         | `agents/` |
| `skills`       | string\|array         | `skills/` |
| `hooks`        | string\|array\|object | `hooks/hooks.json` |
| `mcpServers`   | string\|array\|object | `.mcp.json` |
| `lspServers`   | string\|array\|object | `.lsp.json` |
| `outputStyles` | string\|array         | (no default) |

All custom paths **supplement** default directories — they don't replace them.

### Hook Format Difference: Project vs Plugin

| Context | Location | Format |
|:--------|:---------|:-------|
| **Project** | `.claude/settings.json` | `{ "hooks": { "EventName": [...] } }` (embedded in settings) |
| **Plugin** | `hooks/hooks.json` | `{ "hooks": { "EventName": [...] } }` (standalone file with wrapper) |

The hook JSON structure is identical. The migration extracts the `hooks` value from settings.json and wraps it in a standalone file.

---

## Historical Context (from research/)

- `research/docs/2026-03-16-claude-code-defaults.md` — Authoritative reference for Claude Code directory structure and all component types
- `research/docs/2026-03-23-aipm-migrate-command.md` — Original migrate command research, defines future detector targets
- `research/docs/2026-03-23-recursive-claude-discovery-parallel-migrate.md` — Recursive discovery with parallel detection
- `research/docs/2026-03-22-rust-interactive-cli-prompts.md` — Plugin type taxonomy (skill/agent/mcp/hook/lsp/composite)
- `research/docs/2026-03-20-30-better-default-plugin.md` — Composite plugin type research

## Related Research

- `specs/2026-03-23-aipm-migrate-command.md` — Migrate command spec (day 1 design)
- `specs/2026-03-09-aipm-technical-design.md` — Overall AIPM technical design

## External Sources

- [Claude Code Subagents Documentation](https://code.claude.com/docs/en/sub-agents) — Official agent definition format and frontmatter fields
- [Claude Code Plugins Reference](https://code.claude.com/docs/en/plugins-reference) — Complete plugin component schemas
- [Claude Code MCP Documentation](https://code.claude.com/docs/en/mcp) — MCP server configuration format
- [Claude Code Hooks Documentation](https://code.claude.com/docs/en/hooks) — Hook events and handler types
- [Claude Code Settings Documentation](https://code.claude.com/docs/en/settings) — Settings file format
- [Claude Code Output Styles Documentation](https://code.claude.com/docs/en/output-styles) — Output style format

## Open Questions

1. **MCP per-server vs whole-file**: Should each MCP server in `.mcp.json` become its own plugin, or should all servers be bundled into one plugin?
2. **Settings migration scope**: Given that plugin `settings.json` only supports the `agent` field, should settings migration be deferred or produce a documentation-only output?
3. **Hook script path rewriting**: Hook command fields reference scripts relative to the project root. What rewriting strategy should be used? (`${CLAUDE_PLUGIN_ROOT}/scripts/...`?)
4. **Output styles in plugin.json**: The `outputStyles` field exists in `plugin.json` but output styles directory structure is not well-documented for plugins. Is `.claude/output-styles/` a real directory in current Claude Code versions?
5. **Composite bundling strategy**: When multiple artifact types are found in a single `.claude/` directory (skills + agents + hooks + MCP), should they be emitted as one composite plugin or as separate per-type plugins?
