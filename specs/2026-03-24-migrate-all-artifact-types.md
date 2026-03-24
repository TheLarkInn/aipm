# `aipm migrate` Phase 2 — Extend Migration to All `.claude/` Artifact Types

| Document Metadata      | Details                                                                                          |
| ---------------------- | ------------------------------------------------------------------------------------------------ |
| Author(s)              | selarkin                                                                                         |
| Status                 | Draft (WIP)                                                                                      |
| Team / Owner           | AI Dev Tooling                                                                                   |
| Created / Last Updated | 2026-03-24                                                                                       |
| Research               | [research/docs/2026-03-24-migrate-all-artifact-types.md](../research/docs/2026-03-24-migrate-all-artifact-types.md) |

## 1. Executive Summary

This spec extends `aipm migrate` from skills-and-commands-only to the full Claude Code artifact taxonomy: **agents**, **MCP servers**, **hooks**, and **output styles**. The existing Scanner–Detector–Emitter pipeline was architected for exactly this extension — each new artifact type adds a `Detector` implementation, an `ArtifactKind` variant, and emission logic. Four new detector modules are introduced (`AgentDetector`, `McpDetector`, `HookDetector`, `OutputStyleDetector`). Settings migration is deferred because plugin `settings.json` currently only supports the `agent` field, making it misleadingly incomplete. LSP detection is out of scope because LSP has no standalone project-level configuration — it is a plugin-only concern.

Key design decisions: MCP servers from `.mcp.json` are bundled into **one plugin per project** (not per-server). Hook script paths are rewritten to **absolute paths** for tool-agnostic portability. Root-level `.claude/` emits **separate per-type plugins**; package-scoped `.claude/` directories emit **one composite plugin** per package.

## 2. Context and Motivation

### 2.1 Current State

`aipm migrate` (v0.6.0) scans `.claude/skills/` and `.claude/commands/` directories, converting each detected skill or command into a standalone plugin under `.ai/`. The pipeline uses a `Detector` trait ([`detector.rs:9-19`](../crates/libaipm/src/migrate/detector.rs)) with two implementations: `SkillDetector` and `CommandDetector`. The `claude_detectors()` function returns both in a `Vec<Box<dyn Detector>>` ([`detector.rs:23-28`](../crates/libaipm/src/migrate/detector.rs)).

The `ArtifactKind` enum has two variants (`Skill`, `Command`) and the original spec explicitly notes future variants as a comment: `// Future: Agent, Mcp, Hook, Lsp, OutputStyle, Settings` ([`mod.rs:207`](../crates/libaipm/src/migrate/mod.rs)).

The `Components` struct in [`manifest/types.rs:115-143`](../crates/libaipm/src/manifest/types.rs) already declares all 9 component types (skills, commands, agents, hooks, mcp\_servers, lsp\_servers, scripts, output\_styles, settings).

### 2.2 The Problem

| Problem | Impact |
|---------|--------|
| Agents in `.claude/agents/` are not migrated | Subagent definitions cannot be shared via the marketplace |
| MCP server configs in `.mcp.json` are not migrated | Teams cannot distribute common MCP integrations as plugins |
| Project hooks embedded in `settings.json` are not migrated | Lifecycle hooks tied to skills and workflows remain siloed |
| Output styles in `.claude/output-styles/` are not migrated | Custom formatting preferences cannot be packaged or shared |
| Only skills and commands get the migration path | Users with rich `.claude/` configs must manually reconstruct plugins for everything else |

Research reference: [research/docs/2026-03-24-migrate-all-artifact-types.md](../research/docs/2026-03-24-migrate-all-artifact-types.md) — full analysis of all artifact types, their source locations, schemas, and extension point mapping.

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [ ] `AgentDetector` scans `.claude/agents/` for `.md` files with YAML frontmatter and emits agent-type plugins
- [ ] `McpDetector` reads `.mcp.json` at the **project root** (not inside `.claude/`) and emits a single MCP-type plugin containing all servers
- [ ] `HookDetector` reads `.claude/settings.json`, extracts the `"hooks"` key, and emits a hook-type plugin with `hooks/hooks.json`
- [ ] `OutputStyleDetector` scans `.claude/output-styles/` for `.md` files and emits output-style artifacts
- [ ] `ArtifactKind` enum gains `Agent`, `McpServer`, `Hook`, `OutputStyle` variants
- [ ] `ArtifactMetadata` gains a `raw_content: Option<String>` field for pass-through config artifacts (MCP, hooks)
- [ ] Emitter handles all new `ArtifactKind` variants with appropriate directory layouts and manifest generation
- [ ] Dry-run report includes sections for all new artifact types with accurate component paths
- [ ] Hook script paths (in `command` fields) are rewritten to absolute paths and referenced scripts are copied into the plugin
- [ ] Composite type detection generalizes from `has_skill && has_command` to "2+ distinct artifact kinds present"
- [ ] Root-level `.claude/` emits separate per-type plugins; package-scoped `.claude/` emits one composite plugin per package
- [ ] All new detectors integrate with recursive discovery mode (`migrate_recursive`)
- [ ] All four `cargo` gates pass: `build`, `test`, `clippy`, `fmt`
- [ ] Branch coverage remains ≥ 89%

### 3.2 Non-Goals (Out of Scope)

- [ ] We will NOT implement a `SettingsDetector` — plugin `settings.json` only supports `agent`; deferring until Claude Code expands plugin settings support
- [ ] We will NOT implement an `LspDetector` — LSP servers have no standalone project-level config; they are plugin-only
- [ ] We will NOT implement a `ScriptDetector` — scripts are referenced by skills and hooks, not standalone artifacts
- [ ] We will NOT scan `.github/`, `.copilot/`, or other non-`.claude` folders
- [ ] We will NOT use `${CLAUDE_PLUGIN_ROOT}` for script path rewriting — absolute paths are used for tool-agnostic portability
- [ ] We will NOT split MCP servers into per-server plugins — all servers from one `.mcp.json` become a single plugin
- [ ] We will NOT modify the `aipm-pack` binary — `migrate` is a consumer (`aipm`) command only

## 4. Proposed Solution (High-Level Design)

### 4.1 Architecture

The existing Scanner–Detector–Emitter pipeline remains unchanged. We add 4 new detectors to the `claude_detectors()` registry and extend the emitter to handle 4 new `ArtifactKind` variants.

```
┌─────────────────────────────────────────────────────────────┐
│                    aipm migrate CLI                          │
│              (Commands::Migrate in main.rs)                  │
├─────────────────────────────────────────────────────────────┤
│  Parses flags → builds Options → calls migrate::migrate()   │
│  Prints actions to stdout                                   │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│              libaipm::migrate::migrate()                     │
│           (orchestrator — unchanged)                         │
├─────────────────────────────────────────────────────────────┤
│  1. Validate prerequisites (.ai/ exists)                    │
│  2. Discover .claude/ directories (recursive or single)     │
│  3. For each source, run ALL detectors → Vec<Artifact>      │
│  4. If --dry-run: generate report, return                   │
│  5. For each artifact: emit plugin → .ai/<name>/            │
│  6. Register all in marketplace.json                        │
└───┬──────┬──────┬──────┬──────┬──────┬──────────────────────┘
    │      │      │      │      │      │
    ▼      ▼      ▼      ▼      ▼      ▼
┌──────┐┌──────┐┌──────┐┌──────┐┌──────┐┌───────────┐
│Skill ││Cmd   ││Agent ││ MCP  ││Hook  ││OutputStyle│
│Det.  ││Det.  ││Det.  ││Det.  ││Det.  ││Det.       │
│      ││      ││      ││      ││      ││           │
│.claude││.claude││.claude││.mcp. ││.claude││.claude/   │
│skills/││cmds/ ││agents/││json  ││sett. ││out-styles/│
└──────┘└──────┘└──────┘└──────┘└──────┘└───────────┘
 impl     impl    impl    impl    impl     impl
 Detect.  Detect.  Detect.  Detect.  Detect.  Detect.
```

### 4.2 Architectural Pattern

Same Scanner–Detector–Emitter pipeline as Phase 1 ([`specs/2026-03-23-aipm-migrate-command.md` §4.2](../specs/2026-03-23-aipm-migrate-command.md)). Each detector scans one subdirectory (or config file), returns `Vec<Artifact>`, and the emitter handles the rest. The `Detector` trait is unchanged.

### 4.3 Key Components

| Component | Responsibility | Location | Justification |
|-----------|---------------|----------|---------------|
| `AgentDetector` | Scan `.claude/agents/` for `.md` files | `libaipm::migrate::agent_detector` | Same pattern as `CommandDetector` |
| `McpDetector` | Read `.mcp.json` at project root | `libaipm::migrate::mcp_detector` | `.mcp.json` is outside `.claude/`; detector derives project root from source dir |
| `HookDetector` | Extract `hooks` key from `.claude/settings.json` | `libaipm::migrate::hook_detector` | Hooks are embedded in settings, not standalone |
| `OutputStyleDetector` | Scan `.claude/output-styles/` for `.md` files | `libaipm::migrate::output_style_detector` | Same pattern as `AgentDetector` |
| Extended `ArtifactKind` | 4 new variants | `libaipm::migrate::mod` | Required by all match sites |
| Extended emitter | 4 new emission functions | `libaipm::migrate::emitter` | Each artifact type has distinct directory layout |
| Extended dry-run | New report sections for each type | `libaipm::migrate::dry_run` | Accurate previews for new types |

## 5. Detailed Design

### 5.1 `ArtifactKind` Enum Extension

**File:** `crates/libaipm/src/migrate/mod.rs`

```rust
/// What kind of artifact was detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactKind {
    /// A skill from `.claude/skills/<name>/`.
    Skill,
    /// A legacy command from `.claude/commands/<name>.md`.
    Command,
    /// A subagent from `.claude/agents/<name>.md`.
    Agent,
    /// MCP server configs from `.mcp.json` at the project root.
    McpServer,
    /// Hooks extracted from `.claude/settings.json`.
    Hook,
    /// An output style from `.claude/output-styles/<name>.md`.
    OutputStyle,
}

impl ArtifactKind {
    pub const fn to_type_string(&self) -> &'static str {
        match self {
            Self::Skill | Self::Command => "skill",
            Self::Agent => "agent",
            Self::McpServer => "mcp",
            Self::Hook => "hook",
            // OutputStyle has no standalone PluginType; always composite when mixed
            Self::OutputStyle => "composite",
        }
    }
}
```

### 5.2 `ArtifactMetadata` Extension

Add one field for pass-through content:

```rust
pub struct ArtifactMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
    pub hooks: Option<String>,
    pub model_invocation_disabled: bool,
    /// Raw file content for config-based artifacts (MCP JSON, hooks JSON, etc.).
    /// Used by the emitter for pass-through without re-serialization.
    pub raw_content: Option<String>,
}
```

Skill/command detectors leave `raw_content` as `None`. MCP and hook detectors populate it with the JSON content they should emit.

### 5.3 Agent Detector

**New file:** `crates/libaipm/src/migrate/agent_detector.rs`

```rust
/// Scans `.claude/agents/` for `.md` files (subagent definitions).
pub struct AgentDetector;

impl Detector for AgentDetector {
    fn name(&self) -> &'static str { "agent" }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        let agents_dir = source_dir.join("agents");
        if !fs.exists(&agents_dir) {
            return Ok(Vec::new());
        }

        let entries = fs.read_dir(&agents_dir)?;
        let mut artifacts = Vec::new();

        for entry in entries {
            if entry.is_dir { continue; }
            // Only .md files
            if !Path::new(&entry.name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            { continue; }

            let agent_path = agents_dir.join(&entry.name);
            let content = fs.read_to_string(&agent_path)?;
            let metadata = parse_agent_frontmatter(&content, &agent_path)?;

            let name = metadata.name.clone().unwrap_or_else(|| {
                Path::new(&entry.name)
                    .file_stem()
                    .map_or_else(|| entry.name.clone(), |s| s.to_string_lossy().into_owned())
            });

            artifacts.push(Artifact {
                kind: ArtifactKind::Agent,
                name,
                source_path: agent_path,
                files: vec![PathBuf::from(&entry.name)],
                referenced_scripts: Vec::new(),
                metadata,
            });
        }

        Ok(artifacts)
    }
}
```

**Frontmatter parsing** (`parse_agent_frontmatter`):
- Same `---` delimiter parsing as `parse_skill_frontmatter`
- Extracts `name` and `description` fields only
- Agent-specific fields (`tools`, `model`, `permissionMode`, etc.) are NOT parsed into metadata — they are preserved in the raw `.md` file content which is copied as-is
- Returns `Error::FrontmatterParse` on malformed frontmatter

Agent files are **copied verbatim** (not rewritten). The markdown body is the system prompt and must be preserved exactly. Plugin-shipped agents automatically strip `hooks`, `mcpServers`, and `permissionMode` at load time, so no stripping is needed during migration.

### 5.4 MCP Detector

**New file:** `crates/libaipm/src/migrate/mcp_detector.rs`

```rust
/// Reads `.mcp.json` at the project root and emits all MCP servers as a single artifact.
pub struct McpDetector;

impl Detector for McpDetector {
    fn name(&self) -> &'static str { "mcp" }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        // source_dir is .claude/ — derive project root by going up one level
        let project_root = match source_dir.parent() {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };

        let mcp_path = project_root.join(".mcp.json");
        if !fs.exists(&mcp_path) {
            return Ok(Vec::new());
        }

        let content = fs.read_to_string(&mcp_path)?;

        // Validate it's parseable JSON with mcpServers key
        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| Error::FrontmatterParse {
                path: mcp_path.clone(),
                reason: format!("invalid JSON in .mcp.json: {e}"),
            })?;

        let servers = json.get("mcpServers").and_then(|v| v.as_object());
        if servers.is_none_or(|s| s.is_empty()) {
            return Ok(Vec::new());
        }

        let server_count = servers.map_or(0, |s| s.len());
        let description = format!("{server_count} MCP server(s) from .mcp.json");

        Ok(vec![Artifact {
            kind: ArtifactKind::McpServer,
            name: "project-mcp-servers".to_string(),
            source_path: mcp_path,
            files: vec![PathBuf::from(".mcp.json")],
            referenced_scripts: Vec::new(),
            metadata: ArtifactMetadata {
                name: Some("project-mcp-servers".to_string()),
                description: Some(description),
                raw_content: Some(content),
                ..ArtifactMetadata::default()
            },
        }])
    }
}
```

**Design decisions:**
- One artifact for the entire `.mcp.json` file (per user decision — all servers bundled)
- The plugin name defaults to `"project-mcp-servers"` (rename counter handles conflicts)
- `raw_content` stores the original JSON — emitter writes it verbatim to the plugin's `.mcp.json`
- The detector derives the project root from `source_dir.parent()` (source\_dir is `.claude/`)

### 5.5 Hook Detector

**New file:** `crates/libaipm/src/migrate/hook_detector.rs`

```rust
/// Extracts hooks from `.claude/settings.json` into a standalone hook artifact.
pub struct HookDetector;

impl Detector for HookDetector {
    fn name(&self) -> &'static str { "hook" }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        let settings_path = source_dir.join("settings.json");
        if !fs.exists(&settings_path) {
            return Ok(Vec::new());
        }

        let content = fs.read_to_string(&settings_path)?;
        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| Error::FrontmatterParse {
                path: settings_path.clone(),
                reason: format!("invalid JSON in settings.json: {e}"),
            })?;

        let hooks_value = match json.get("hooks") {
            Some(v) if v.is_object() && !v.as_object().is_some_and(|o| o.is_empty()) => v,
            _ => return Ok(Vec::new()),
        };

        // Build plugin hooks.json format: { "hooks": { ... } }
        // (same structure, just wrapped in a standalone file)
        let hooks_json = serde_json::json!({ "hooks": hooks_value });
        let hooks_content = serde_json::to_string_pretty(&hooks_json)
            .unwrap_or_else(|_| "{}".to_string());

        // Extract script references from command hooks
        let referenced_scripts = extract_hook_script_references(hooks_value, source_dir);

        Ok(vec![Artifact {
            kind: ArtifactKind::Hook,
            name: "project-hooks".to_string(),
            source_path: settings_path,
            files: Vec::new(), // no files to copy — content is in raw_content
            referenced_scripts,
            metadata: ArtifactMetadata {
                name: Some("project-hooks".to_string()),
                description: Some("Hooks extracted from .claude/settings.json".to_string()),
                raw_content: Some(hooks_content),
                ..ArtifactMetadata::default()
            },
        }])
    }
}
```

**`extract_hook_script_references`**: Walks the hooks JSON recursively. For any `"type": "command"` handler, extracts the `"command"` value. If it starts with `./` or is a relative path, it's treated as a script reference. Returns `Vec<PathBuf>`.

**Script path rewriting**: The emitter rewrites `command` field values from relative paths (`./scripts/validate.sh`) to absolute paths (resolved against the project root at migration time). This is tool-agnostic — works with Claude Code, Copilot CLI, opencode, or any future tool.

### 5.6 Output Style Detector

**New file:** `crates/libaipm/src/migrate/output_style_detector.rs`

```rust
/// Scans `.claude/output-styles/` for `.md` files.
pub struct OutputStyleDetector;

impl Detector for OutputStyleDetector {
    fn name(&self) -> &'static str { "output-style" }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        let styles_dir = source_dir.join("output-styles");
        if !fs.exists(&styles_dir) {
            return Ok(Vec::new());
        }

        let entries = fs.read_dir(&styles_dir)?;
        let mut artifacts = Vec::new();

        for entry in entries {
            if entry.is_dir { continue; }
            if !Path::new(&entry.name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            { continue; }

            let style_path = styles_dir.join(&entry.name);
            let content = fs.read_to_string(&style_path)?;
            let metadata = parse_output_style_frontmatter(&content);

            let name = metadata.name.clone().unwrap_or_else(|| {
                Path::new(&entry.name)
                    .file_stem()
                    .map_or_else(|| entry.name.clone(), |s| s.to_string_lossy().into_owned())
            });

            artifacts.push(Artifact {
                kind: ArtifactKind::OutputStyle,
                name,
                source_path: style_path,
                files: vec![PathBuf::from(&entry.name)],
                referenced_scripts: Vec::new(),
                metadata,
            });
        }

        Ok(artifacts)
    }
}
```

**Frontmatter parsing** (`parse_output_style_frontmatter`): Extracts `name` and `description` from YAML frontmatter. The `keep-coding-instructions` field is NOT parsed — the entire `.md` file is copied verbatim to the plugin.

### 5.7 Detector Registry Update

**File:** `crates/libaipm/src/migrate/detector.rs`

```rust
pub fn claude_detectors() -> Vec<Box<dyn Detector>> {
    vec![
        Box::new(super::skill_detector::SkillDetector),
        Box::new(super::command_detector::CommandDetector),
        Box::new(super::agent_detector::AgentDetector),
        Box::new(super::mcp_detector::McpDetector),
        Box::new(super::hook_detector::HookDetector),
        Box::new(super::output_style_detector::OutputStyleDetector),
    ]
}
```

**Module declarations** in `mod.rs`:
```rust
pub mod agent_detector;
pub mod mcp_detector;
pub mod hook_detector;
pub mod output_style_detector;
```

### 5.8 Emitter Changes

**File:** `crates/libaipm/src/migrate/emitter.rs`

#### New emission functions

**`emit_agent_files`**: Copies the agent `.md` file to `agents/<name>.md` inside the plugin directory. No content transformation.

```
.ai/<plugin-name>/
├── .claude-plugin/plugin.json
├── agents/
│   └── <name>.md
└── aipm.toml
```

**`emit_mcp_config`**: Writes `raw_content` (the `.mcp.json` content) to `.mcp.json` at the plugin root. No transformation.

```
.ai/<plugin-name>/
├── .claude-plugin/plugin.json
├── .mcp.json
└── aipm.toml
```

**`emit_hooks_config`**: Writes `raw_content` (the `hooks.json` content) to `hooks/hooks.json`. Copies referenced scripts to `scripts/`. Rewrites `command` field paths to absolute paths.

```
.ai/<plugin-name>/
├── .claude-plugin/plugin.json
├── hooks/
│   └── hooks.json
├── scripts/     (if hook commands reference scripts)
│   └── validate.sh
└── aipm.toml
```

**`emit_output_style`**: Copies the `.md` file to the plugin root (no subdirectory needed — Claude Code's `outputStyles` field in plugin.json points to the file/directory).

```
.ai/<plugin-name>/
├── .claude-plugin/plugin.json
├── <name>.md
└── aipm.toml
```

#### Match site updates

All three `match artifact.kind` blocks in the emitter (`emit_plugin`, `emit_plugin_with_name`, `emit_package_plugin`) need new arms:

```rust
match artifact.kind {
    ArtifactKind::Skill => emit_skill_files(artifact, &plugin_dir, fs)?,
    ArtifactKind::Command => emit_command_as_skill(artifact, &plugin_dir, fs)?,
    ArtifactKind::Agent => emit_agent_files(artifact, &plugin_dir, fs)?,
    ArtifactKind::McpServer => emit_mcp_config(artifact, &plugin_dir, fs)?,
    ArtifactKind::Hook => emit_hooks_config(artifact, &plugin_dir, fs)?,
    ArtifactKind::OutputStyle => emit_output_style(artifact, &plugin_dir, fs)?,
}
```

#### Manifest generation changes

**`generate_plugin_manifest`** must produce the correct `[components]` section per artifact kind:

| ArtifactKind | `[components]` entry |
|:-------------|:---------------------|
| `Skill`/`Command` | `skills = ["skills/<name>/SKILL.md"]` |
| `Agent` | `agents = ["agents/<name>.md"]` |
| `McpServer` | `mcp_servers = [".mcp.json"]` |
| `Hook` | `hooks = ["hooks/hooks.json"]` |
| `OutputStyle` | `output_styles = ["<name>.md"]` |

**`generate_package_manifest`** composite type detection must generalize:

```rust
// Count distinct kinds
let distinct_kinds: HashSet<_> = artifacts.iter().map(|a| &a.kind).collect();
let type_str = if distinct_kinds.len() > 1 { "composite" } else {
    artifacts.first().map_or("composite", |a| a.kind.to_type_string())
};
```

The components section must accumulate entries for each kind present:
- `skills = [...]` if any skills or commands
- `agents = [...]` if any agents
- `mcp_servers = [...]` if any MCP servers
- `hooks = [...]` if any hooks
- `output_styles = [...]` if any output styles

#### `plugin.json` generation

The `generate_plugin_json` function currently emits `name`, `version`, `description`. For new artifact types, it should also emit the component path field:

```json
{
  "name": "security-reviewer",
  "version": "0.1.0",
  "description": "Migrated from .claude/ configuration",
  "agents": "./agents/"
}
```

| ArtifactKind | `plugin.json` field |
|:-------------|:--------------------|
| `Skill`/`Command` | `"skills": "./skills/"` |
| `Agent` | `"agents": "./agents/"` |
| `McpServer` | `"mcpServers": "./.mcp.json"` |
| `Hook` | `"hooks": "./hooks/hooks.json"` |
| `OutputStyle` | `"outputStyles": "./"` |

### 5.9 Dry-Run Report Changes

**File:** `crates/libaipm/src/migrate/dry_run.rs`

**`generate_report`**: Add filter groups and sections for each new type:

```rust
let agents: Vec<_> = artifacts.iter().filter(|a| a.kind == ArtifactKind::Agent).collect();
let mcp: Vec<_> = artifacts.iter().filter(|a| a.kind == ArtifactKind::McpServer).collect();
let hooks: Vec<_> = artifacts.iter().filter(|a| a.kind == ArtifactKind::Hook).collect();
let output_styles: Vec<_> = artifacts.iter().filter(|a| a.kind == ArtifactKind::OutputStyle).collect();
```

New sections: `## Agents`, `## MCP Servers`, `## Hooks`, `## Output Styles`.

**`generate_recursive_report`**: The discovery table header and fold accumulator must track counts for all types:

```
| Location | Package | Skills | Commands | Agents | MCP | Hooks | Styles |
```

Composite type detection in the planned plugins section must generalize from `has_skill && has_command` to counting distinct kinds.

**`write_artifact_section`**: The component path display (`skills/<name>/SKILL.md`) must be artifact-kind-aware:

| ArtifactKind | Display path |
|:-------------|:-------------|
| `Skill`/`Command` | `skills/<name>/SKILL.md` |
| `Agent` | `agents/<name>.md` |
| `McpServer` | `.mcp.json` |
| `Hook` | `hooks/hooks.json` |
| `OutputStyle` | `<name>.md` |

### 5.10 Error Handling

**New error variant** for `Error` enum:

```rust
/// Failed to parse a JSON configuration file.
#[error("failed to parse {path}: {reason}")]
ConfigParse {
    path: PathBuf,
    reason: String,
},
```

This replaces the current practice of using `FrontmatterParse` for JSON parse errors in the MCP and hook detectors. The `FrontmatterParse` variant remains for markdown frontmatter errors.

### 5.11 Bundling Strategy (Root vs Package-Scoped)

This follows the existing pattern from the recursive discovery implementation ([`mod.rs:276-296`](../crates/libaipm/src/migrate/mod.rs)):

**Root-level `.claude/`** (no `package_name`): Each artifact becomes its own plugin. A root `.claude/` with 2 skills, 1 agent, and 1 hook becomes 4 separate plugins.

**Package-scoped `.claude/`** (`package_name` = Some): All artifacts from that `.claude/` directory merge into one composite plugin named after the package. A `packages/auth/.claude/` with 1 skill, 1 agent, and hooks becomes one `auth` composite plugin.

This is the existing behavior for skills/commands — no orchestrator changes needed. The `PluginPlan` struct already handles both modes.

## 6. Alternatives Considered

| Option | Pros | Cons | Decision |
|--------|------|------|----------|
| Per-server MCP plugins | Granular install/uninstall | Most MCP configs have related servers; splitting is awkward | **Rejected** — user prefers bundled |
| `${CLAUDE_PLUGIN_ROOT}` for script paths | Standard Claude Code plugin variable | Ties scripts to Claude Code only; breaks other tools | **Rejected** — absolute paths are tool-agnostic |
| Settings migration now | Complete migration story | Plugin `settings.json` only supports `agent` field | **Deferred** — misleading to users |
| Composite-only bundling (always one plugin) | Simplest implementation | Loses granularity for root-level configs | **Rejected** — hybrid approach matches existing pattern |
| Single monolithic detector | Less code to write | Violates open/closed principle; hard to test | **Rejected** — one detector per type |

## 7. Cross-Cutting Concerns

### 7.1 Security

- **Path traversal**: All artifact names pass through the existing `is_safe_path_segment()` validation before being used as directory names.
- **JSON injection**: MCP and hook JSON content is read from the filesystem and written verbatim. No string interpolation or template expansion that could introduce injection.
- **Plugin-stripped fields**: Agent `.md` files are copied verbatim. Claude Code's plugin loading automatically strips `hooks`, `mcpServers`, and `permissionMode` from plugin-shipped agents — no need to sanitize during migration.

### 7.2 Testing Strategy

Each detector gets a unit test module following the pattern in `skill_detector.rs`:
- Uses `MockFs` with pre-populated `exists`, `dirs`, `files` maps
- Tests: happy path, missing directory, empty directory, malformed content, multiple artifacts
- Agent: tests frontmatter parsing, missing frontmatter, non-`.md` files skipped
- MCP: tests JSON parsing, empty `mcpServers`, no `.mcp.json`, project-root derivation
- Hook: tests `settings.json` parsing, no hooks key, empty hooks object, script reference extraction
- Output style: tests markdown detection, frontmatter parsing, name fallback to filename

The emitter tests extend existing `MockFs`-based tests to cover new match arms.

The dry-run tests add `make_artifact` calls for new kinds and verify section headers and component path formatting.

E2E tests in `crates/aipm/tests/migrate_e2e.rs` should add scenarios for:
- Project with agents directory
- Project with `.mcp.json` at root
- Project with hooks in settings.json
- Project with output styles
- Mixed project (skills + agents + hooks + MCP) producing separate root plugins
- Package-scoped directory producing composite plugin

## 8. Migration, Rollout, and Testing

### 8.1 Implementation Order

Each step is independently testable and the pipeline continues to work after each step (new variants are additive):

1. **Extend `ArtifactKind` and `ArtifactMetadata`** — add new enum variants and `raw_content` field. Update all match sites with temporary `todo!()` arms (will be replaced in subsequent steps). Update `to_type_string()`.

2. **`AgentDetector`** — simplest new detector (same pattern as `CommandDetector`). Implement detector + emitter arm + manifest generation + dry-run section + tests.

3. **`OutputStyleDetector`** — second simplest (same pattern as agents). Implement detector + emitter arm + manifest generation + dry-run section + tests.

4. **`McpDetector`** — medium complexity (project-root derivation, JSON parsing, `raw_content` pass-through). Implement detector + emitter arm + manifest generation + dry-run section + tests.

5. **`HookDetector`** — highest complexity (embedded JSON extraction, script reference extraction, absolute path rewriting). Implement detector + emitter arm + manifest generation + dry-run section + tests.

6. **Generalize composite type detection** — update `emit_package_plugin`, `generate_package_manifest`, and recursive dry-run report to use `distinct_kinds.len() > 1` instead of `has_skill && has_command`.

7. **E2E tests and coverage gate** — add integration tests, verify 89% branch coverage.

### 8.2 Test Plan

- **Unit Tests:** Each detector module, each emitter function, each dry-run section
- **Integration Tests:** E2E tests via `assert_cmd` with real filesystem
- **Coverage Gate:** `cargo +nightly llvm-cov` must show ≥ 89% branch coverage

## 9. Open Questions / Unresolved Issues

- [ ] Should the MCP plugin name be `"project-mcp-servers"` or derive from the repo name (e.g., `"aipm-mcp-servers"`)?
- [ ] Should the hook detector also check `.claude/settings.local.json` for hooks, or only `.claude/settings.json` (the committed, shared version)?
- [ ] Should output styles be emitted into their own subdirectory (`output-styles/`) within the plugin, or at the plugin root?
