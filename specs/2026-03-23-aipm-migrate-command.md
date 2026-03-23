# `aipm migrate` — Scan and Migrate AI Tool Configurations into Marketplace Plugins

| Document Metadata      | Details                                                                                                        |
| ---------------------- | -------------------------------------------------------------------------------------------------------------- |
| Author(s)              | selarkin                                                                                                       |
| Status                 | Draft (WIP)                                                                                                    |
| Team / Owner           | AI Dev Tooling                                                                                                 |
| Created / Last Updated | 2026-03-23                                                                                                     |
| Research               | [research/docs/2026-03-23-aipm-migrate-command.md](../research/docs/2026-03-23-aipm-migrate-command.md)        |

## 1. Executive Summary

This spec introduces `aipm migrate`, a new subcommand for the `aipm` consumer CLI that scans AI tool configuration folders (starting with `.claude/`) for skills, commands, and other artifacts, then converts each into a standalone plugin in the `.ai/` marketplace. On day 1, the command migrates skills from `.claude/skills/` and legacy commands from `.claude/commands/`, copies their files (with script path rewriting) into `.ai/<plugin-name>/`, generates an `aipm.toml` manifest, registers each plugin in `marketplace.json`, and extracts any frontmatter hooks into `hooks.json`. A `--dry-run` flag produces a structured markdown report without writing files. The architecture uses a `Detector` trait so future iterations can add agent, MCP, LSP, hook, and output-style detectors — and scan additional source folders like `.github/` and `.copilot/` — without changing the core orchestration logic.

## 2. Context and Motivation

### 2.1 Current State

Today, developers configure Claude Code by placing skills in `.claude/skills/<name>/SKILL.md` and legacy commands in `.claude/commands/<name>.md`. These live inside the tool-specific `.claude/` directory and are not interoperable with the `aipm` plugin ecosystem. The `aipm init` command creates a `.ai/` marketplace and an optional starter plugin, but there is no bridge from existing tool configurations to marketplace plugins.

```
project/
├── .claude/
│   ├── skills/
│   │   ├── deploy/
│   │   │   ├── SKILL.md
│   │   │   └── scripts/deploy.sh
│   │   └── lint-fix/
│   │       └── SKILL.md
│   ├── commands/
│   │   └── review.md
│   └── settings.json
├── .ai/                          ← marketplace (created by aipm init)
│   ├── .claude-plugin/
│   │   └── marketplace.json
│   └── starter-aipm-plugin/
└── aipm.toml
```

### 2.2 The Problem

| Problem | Impact |
|---------|--------|
| Skills in `.claude/` are invisible to the plugin marketplace | Cannot be shared, versioned, or discovered via `aipm` tooling |
| No automated conversion path exists | Manual migration is error-prone and tedious for repos with many skills |
| Legacy `.claude/commands/` files are a dead-end format | Users must manually convert to the skills format before they can benefit from plugins |
| No extensibility point for scanning other tool folders | Adding `.github/copilot` or `.copilot/` support would require ad-hoc code each time |

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [ ] `aipm migrate` scans `.claude/skills/` and discovers all skill directories containing `SKILL.md`
- [ ] `aipm migrate` scans `.claude/commands/` and discovers all legacy command `.md` files
- [ ] Each discovered skill/command is **copied** (non-destructive) into `.ai/<name>/` as a plugin
- [ ] Each migrated plugin gets a valid `aipm.toml` with `[package]` and `[components]` sections
- [ ] Each migrated plugin gets a `.claude-plugin/plugin.json` descriptor
- [ ] Each migrated plugin is registered in `.ai/.claude-plugin/marketplace.json` (NOT auto-enabled)
- [ ] Legacy commands get `disable-model-invocation: true` added to their SKILL.md frontmatter
- [ ] `${CLAUDE_SKILL_DIR}` references in SKILL.md are rewritten to match the new plugin directory layout
- [ ] Hooks in SKILL.md frontmatter are extracted into a plugin-level `hooks/hooks.json`
- [ ] Name conflicts with existing plugins are resolved by auto-renaming to `<name>-renamed-<id>`
- [ ] `--dry-run` flag produces `aipm-migrate-dryrun-report.md` in the project root without writing any other files
- [ ] The `Fs` trait is extended with `read_dir` for directory scanning
- [ ] A `Detector` trait enables future artifact type detectors without changing orchestration code
- [ ] `.ai/` must already exist (error with helpful message if missing — user must run `aipm init` first)

### 3.2 Non-Goals (Out of Scope)

- [ ] We will NOT implement a wizard/interactive mode on day 1 (flag reserved for future)
- [ ] We will NOT scan `.github/`, `.copilot/`, or other non-`.claude` folders on day 1
- [ ] We will NOT detect agents, MCPs, LSPs, hooks, or output styles as standalone artifacts on day 1
- [ ] We will NOT move/delete original files from `.claude/` — copy only
- [ ] We will NOT auto-enable migrated plugins in `.claude/settings.json`
- [ ] We will NOT modify the `aipm-pack` binary — `migrate` is a consumer (`aipm`) command only

## 4. Proposed Solution (High-Level Design)

### 4.1 Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    aipm migrate CLI                      │
│              (Commands::Migrate in main.rs)              │
├─────────────────────────────────────────────────────────┤
│  Parses flags → builds Options → calls migrate::migrate()│
│  Prints actions to stdout                                │
└──────────────────────┬──────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────┐
│              libaipm::migrate::migrate()                 │
│           (orchestrator — takes Options, Fs)             │
├─────────────────────────────────────────────────────────┤
│  1. Validate prerequisites (.ai/ exists)                 │
│  2. Resolve source → Vec<Source>                         │
│  3. For each source, run detectors → Vec<Artifact>       │
│  4. If --dry-run: generate report, return                │
│  5. For each artifact: emit plugin → .ai/<name>/         │
│  6. Register all in marketplace.json                     │
│  7. Return MigrateResult with actions                    │
└───────┬─────────────┬────────────────┬──────────────────┘
        │             │                │
        ▼             ▼                ▼
┌─────────────┐ ┌──────────────┐ ┌──────────────┐
│   Skill     │ │   Command    │ │   (Future)   │
│  Detector   │ │  Detector    │ │   Agent/MCP  │
│             │ │              │ │   Detector   │
│ .claude/    │ │ .claude/     │ │              │
│  skills/    │ │  commands/   │ │              │
└─────────────┘ └──────────────┘ └──────────────┘
    impl Detector    impl Detector    impl Detector
```

### 4.2 Architectural Pattern

We adopt a **Scanner–Detector–Emitter** pipeline:

1. **Scanner** resolves source directories (`.claude/` on day 1)
2. **Detectors** (trait objects) scan a source directory for artifacts of a specific kind
3. **Emitter** converts each detected artifact into a plugin directory under `.ai/`
4. **Registrar** appends plugin entries to `marketplace.json`

This is analogous to the existing `ToolAdaptor` pattern ([workspace_init/mod.rs:17-30](../crates/libaipm/src/workspace_init/mod.rs)) where trait objects are iterated and each performs its work independently.

### 4.3 Key Components

| Component | Responsibility | Location | Justification |
|-----------|---------------|----------|---------------|
| `Detector` trait | Scan a directory, return discovered artifacts | `libaipm::migrate::detector` | Extensible — add agent/MCP/LSP detectors without touching orchestrator |
| `SkillDetector` | Scan `.claude/skills/` for SKILL.md directories | `libaipm::migrate::skill_detector` | Day 1 primary target |
| `CommandDetector` | Scan `.claude/commands/` for legacy .md files | `libaipm::migrate::command_detector` | Day 1 — converts to skill format |
| `Emitter` functions | Write plugin dirs to `.ai/`, generate manifests | `libaipm::migrate::emitter` | Shared by all detectors |
| `Registrar` functions | Append to `marketplace.json` | `libaipm::migrate::registrar` | Reuses JSON merge pattern from Claude adaptor |
| `DryRunReporter` | Generate `aipm-migrate-dryrun-report.md` | `libaipm::migrate::dry_run` | Structured markdown report |

## 5. Detailed Design

### 5.1 Fs Trait Extension

**`crates/libaipm/src/fs.rs`** — Add `read_dir` method:

```rust
pub trait Fs {
    fn exists(&self, path: &Path) -> bool;
    fn create_dir_all(&self, path: &Path) -> std::io::Result<()>;
    fn write_file(&self, path: &Path, content: &[u8]) -> std::io::Result<()>;
    fn read_to_string(&self, path: &Path) -> std::io::Result<String>;

    /// List entries in a directory. Returns file names (not full paths).
    /// Returns an empty Vec if the directory does not exist.
    fn read_dir(&self, path: &Path) -> std::io::Result<Vec<DirEntry>>;
}

/// A directory entry returned by `Fs::read_dir`.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
}
```

The `Real` implementation delegates to `std::fs::read_dir`, collecting entries into `Vec<DirEntry>`. Test mocks can return predetermined directory listings.

### 5.2 CLI Interface

**`crates/aipm/src/main.rs`** — Add `Commands::Migrate` variant:

```rust
#[derive(Subcommand)]
enum Commands {
    /// Initialize a workspace for AI plugin management
    Init { /* existing fields */ },

    /// Migrate AI tool configurations into marketplace plugins
    Migrate {
        /// Preview migration without writing files (generates report)
        #[arg(long)]
        dry_run: bool,

        /// Source folder to scan (default: .claude)
        #[arg(long, default_value = ".claude")]
        source: String,

        /// Project directory
        #[arg(default_value = ".")]
        dir: std::path::PathBuf,
    },
}
```

> The `--source` flag defaults to `.claude` but is designed for future expansion (e.g., `--source .github`). The `--dry-run` flag uses `dry_run` with underscore per Rust conventions; clap maps it to `--dry-run`.

### 5.3 Core Types

**New module: `crates/libaipm/src/migrate/mod.rs`**

```rust
use std::path::{Path, PathBuf};
use crate::fs::Fs;

/// What kind of artifact was detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactKind {
    Skill,
    Command,
    // Future: Agent, Mcp, Hook, Lsp, OutputStyle, Settings
}

/// Metadata extracted from a skill's YAML frontmatter.
#[derive(Debug, Clone, Default)]
pub struct ArtifactMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
    pub hooks: Option<String>,          // raw YAML/JSON hooks block
    pub model_invocation_disabled: bool, // for commands: always true
}

/// A single detected artifact from a source folder.
#[derive(Debug, Clone)]
pub struct Artifact {
    pub kind: ArtifactKind,
    pub name: String,                     // e.g., "deploy", "lint-fix"
    pub source_path: PathBuf,             // e.g., .claude/skills/deploy/
    pub files: Vec<PathBuf>,              // all files (relative to source_path)
    pub referenced_scripts: Vec<PathBuf>, // scripts found in body
    pub metadata: ArtifactMetadata,
}

/// Options for the migrate command.
pub struct Options<'a> {
    pub dir: &'a Path,           // project root
    pub source: &'a str,         // source folder name (e.g., ".claude")
    pub dry_run: bool,
}

/// A single action taken (or planned) during migration.
#[derive(Debug, Clone)]
pub enum MigrateAction {
    PluginCreated {
        name: String,
        source: PathBuf,
        plugin_type: String,
    },
    MarketplaceRegistered {
        name: String,
    },
    Renamed {
        original_name: String,
        new_name: String,
        reason: String,
    },
    Skipped {
        name: String,
        reason: String,
    },
    DryRunReport {
        path: PathBuf,
    },
}

/// Result of migration.
pub struct MigrateResult {
    pub actions: Vec<MigrateAction>,
}

/// Errors specific to migration.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("marketplace directory does not exist at {0} — run `aipm init --marketplace` first")]
    MarketplaceNotFound(PathBuf),

    #[error("source directory does not exist: {0}")]
    SourceNotFound(PathBuf),

    #[error("failed to parse marketplace.json at {path}: {source}")]
    MarketplaceJsonParse {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("failed to parse SKILL.md frontmatter in {path}: {reason}")]
    FrontmatterParse {
        path: PathBuf,
        reason: String,
    },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Manifest(#[from] crate::manifest::Error),
}
```

### 5.4 Detector Trait

**New file: `crates/libaipm/src/migrate/detector.rs`**

```rust
use std::path::Path;
use crate::fs::Fs;
use super::{Artifact, Error};

/// Trait for scanning a source directory for artifacts of a specific kind.
///
/// Each detector is responsible for one artifact type within one source folder.
/// The orchestrator calls `detect()` and collects all returned artifacts.
pub trait Detector {
    /// Human-readable name for this detector (e.g., "skill", "command").
    fn name(&self) -> &'static str;

    /// Scan `source_dir` and return all discovered artifacts.
    /// `source_dir` is the resolved path (e.g., /project/.claude/).
    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error>;
}

/// Returns the default set of detectors for `.claude/` source.
pub fn claude_detectors() -> Vec<Box<dyn Detector>> {
    vec![
        Box::new(super::skill_detector::SkillDetector),
        Box::new(super::command_detector::CommandDetector),
    ]
}
```

### 5.5 Skill Detector

**New file: `crates/libaipm/src/migrate/skill_detector.rs`**

```rust
/// Scans .claude/skills/ for directories containing SKILL.md.
pub struct SkillDetector;

impl Detector for SkillDetector {
    fn name(&self) -> &'static str { "skill" }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        let skills_dir = source_dir.join("skills");
        if !fs.exists(&skills_dir) {
            return Ok(Vec::new());
        }

        let entries = fs.read_dir(&skills_dir)?;
        let mut artifacts = Vec::new();

        for entry in entries {
            if !entry.is_dir { continue; }

            let skill_dir = skills_dir.join(&entry.name);
            let skill_md = skill_dir.join("SKILL.md");

            if !fs.exists(&skill_md) { continue; }

            let content = fs.read_to_string(&skill_md)?;
            let metadata = parse_skill_frontmatter(&content)?;
            let files = collect_files_recursive(&skill_dir, &skill_dir, fs)?;
            let referenced_scripts = extract_script_references(&content);

            let name = metadata.name.clone()
                .unwrap_or_else(|| entry.name.clone());

            artifacts.push(Artifact {
                kind: ArtifactKind::Skill,
                name,
                source_path: skill_dir,
                files,
                referenced_scripts,
                metadata,
            });
        }

        Ok(artifacts)
    }
}
```

**Frontmatter parsing** (`parse_skill_frontmatter`):
- Splits content on `---` delimiters (first two occurrences)
- Extracts YAML block between delimiters
- Parses `name`, `description`, `hooks` fields
- For hooks: preserves raw YAML text (will be converted to JSON by the emitter)
- Returns `ArtifactMetadata`

**Script reference extraction** (`extract_script_references`):
- Scans the markdown body (after frontmatter) for patterns:
  - `${CLAUDE_SKILL_DIR}/scripts/<path>`
  - `${CLAUDE_SKILL_DIR}/<path>` (any relative path)
- Returns `Vec<PathBuf>` of referenced paths relative to the skill directory

**Recursive file collection** (`collect_files_recursive`):
- Walks the skill directory using `fs.read_dir` recursively
- Returns `Vec<PathBuf>` of all files relative to the skill root

### 5.6 Command Detector

**New file: `crates/libaipm/src/migrate/command_detector.rs`**

```rust
/// Scans .claude/commands/ for .md files (legacy command format).
pub struct CommandDetector;

impl Detector for CommandDetector {
    fn name(&self) -> &'static str { "command" }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        let commands_dir = source_dir.join("commands");
        if !fs.exists(&commands_dir) {
            return Ok(Vec::new());
        }

        let entries = fs.read_dir(&commands_dir)?;
        let mut artifacts = Vec::new();

        for entry in entries {
            if entry.is_dir { continue; } // commands are single files
            if !entry.name.ends_with(".md") { continue; }

            let cmd_path = commands_dir.join(&entry.name);
            let content = fs.read_to_string(&cmd_path)?;

            // Strip .md extension for the name
            let name = entry.name.trim_end_matches(".md").to_string();

            let mut metadata = parse_command_frontmatter(&content);
            // Commands always get disable-model-invocation: true
            metadata.model_invocation_disabled = true;

            let referenced_scripts = extract_script_references(&content);

            artifacts.push(Artifact {
                kind: ArtifactKind::Command,
                name,
                source_path: cmd_path,
                files: vec![PathBuf::from(&entry.name)],
                referenced_scripts,
                metadata,
            });
        }

        Ok(artifacts)
    }
}
```

**Command-to-skill conversion**: When the emitter writes a command as a plugin, it:
1. Creates a `skills/<name>/SKILL.md` inside the plugin directory
2. If the original command had frontmatter, ensures `disable-model-invocation: true` is added
3. If the original command had no frontmatter, wraps the content with frontmatter:
   ```yaml
   ---
   name: <command-name>
   disable-model-invocation: true
   ---
   <original content>
   ```

### 5.7 Emitter — Plugin Directory Creation

**New file: `crates/libaipm/src/migrate/emitter.rs`**

The emitter converts a single `Artifact` into a plugin directory under `.ai/`.

```rust
/// Emit a single artifact as a plugin directory.
///
/// Returns the final plugin name (may differ from artifact name if renamed).
pub fn emit_plugin(
    artifact: &Artifact,
    ai_dir: &Path,
    existing_names: &HashSet<String>,
    rename_counter: &mut u32,
    fs: &dyn Fs,
) -> Result<(String, Vec<MigrateAction>), Error> {
    let mut actions = Vec::new();

    // 1. Resolve name (handle conflicts)
    let plugin_name = resolve_plugin_name(
        &artifact.name, existing_names, rename_counter, &mut actions
    );

    let plugin_dir = ai_dir.join(&plugin_name);

    // 2. Create directory structure
    fs.create_dir_all(&plugin_dir)?;
    fs.create_dir_all(&plugin_dir.join(".claude-plugin"))?;
    fs.create_dir_all(&plugin_dir.join("skills").join(&artifact.name))?;

    // 3. Copy skill files (with path rewriting for SKILL.md)
    for file in &artifact.files {
        let source = artifact.source_path.join(file);
        let dest = plugin_dir.join("skills").join(&artifact.name).join(file);
        if let Some(parent) = dest.parent() {
            fs.create_dir_all(parent)?;
        }
        let content = fs.read_to_string(&source)?;

        // Rewrite ${CLAUDE_SKILL_DIR} paths in SKILL.md
        let final_content = if file_is_skill_md(file) {
            rewrite_skill_dir_paths(&content, &artifact.name)
        } else {
            content
        };

        fs.write_file(&dest, final_content.as_bytes())?;
    }

    // 4. Copy referenced scripts
    if !artifact.referenced_scripts.is_empty() {
        let scripts_dir = plugin_dir.join("scripts");
        fs.create_dir_all(&scripts_dir)?;
        for script in &artifact.referenced_scripts {
            let source = artifact.source_path.join(script);
            if fs.exists(&source) {
                let dest = scripts_dir.join(
                    script.file_name().unwrap_or_default()
                );
                let content = fs.read_to_string(&source)?;
                fs.write_file(&dest, content.as_bytes())?;
            }
        }
    }

    // 5. Extract hooks (if any) into hooks/hooks.json
    if let Some(ref hooks_yaml) = artifact.metadata.hooks {
        let hooks_dir = plugin_dir.join("hooks");
        fs.create_dir_all(&hooks_dir)?;
        let hooks_json = convert_hooks_yaml_to_json(hooks_yaml)?;
        write_file(&hooks_dir.join("hooks.json"), &hooks_json, fs)?;
    }

    // 6. Generate aipm.toml
    let manifest = generate_plugin_manifest(artifact, &plugin_name);
    write_file(&plugin_dir.join("aipm.toml"), &manifest, fs)?;

    // 7. Generate .claude-plugin/plugin.json
    let plugin_json = generate_plugin_json(&plugin_name, &artifact.metadata);
    write_file(
        &plugin_dir.join(".claude-plugin").join("plugin.json"),
        &plugin_json,
        fs,
    )?;

    actions.push(MigrateAction::PluginCreated {
        name: plugin_name.clone(),
        source: artifact.source_path.clone(),
        plugin_type: artifact.kind.to_type_string().to_string(),
    });

    Ok((plugin_name, actions))
}
```

### 5.8 Name Conflict Resolution

```rust
/// Resolve plugin name, auto-renaming on conflict.
fn resolve_plugin_name(
    name: &str,
    existing: &HashSet<String>,
    counter: &mut u32,
    actions: &mut Vec<MigrateAction>,
) -> String {
    if !existing.contains(name) {
        return name.to_string();
    }

    *counter += 1;
    let new_name = format!("{name}-renamed-{counter}");
    actions.push(MigrateAction::Renamed {
        original_name: name.to_string(),
        new_name: new_name.clone(),
        reason: format!("plugin '{name}' already exists in .ai/"),
    });
    new_name
}
```

The rename counter is shared across all artifacts in a single `migrate()` invocation, producing sequential IDs: `deploy-renamed-1`, `lint-renamed-2`, etc.

### 5.9 `${CLAUDE_SKILL_DIR}` Path Rewriting

When a skill is migrated from `.claude/skills/deploy/` to `.ai/deploy/skills/deploy/`, the `SKILL.md` references to `${CLAUDE_SKILL_DIR}/scripts/deploy.sh` need rewriting because the scripts now live at `.ai/deploy/scripts/deploy.sh` (one level up from the skills subdirectory).

```rust
/// Rewrite ${CLAUDE_SKILL_DIR}/scripts/... paths in SKILL.md content.
///
/// In the original layout, scripts are siblings of SKILL.md:
///   .claude/skills/deploy/scripts/deploy.sh
///   ${CLAUDE_SKILL_DIR}/scripts/deploy.sh  ← resolves correctly
///
/// In the plugin layout, scripts are at the plugin root:
///   .ai/deploy/scripts/deploy.sh
///   .ai/deploy/skills/deploy/SKILL.md
///   ${CLAUDE_SKILL_DIR}/scripts/deploy.sh  ← would look inside skills/deploy/scripts/
///
/// We rewrite to: ${CLAUDE_SKILL_DIR}/../../scripts/deploy.sh
/// which resolves from skills/deploy/ up to the plugin root.
fn rewrite_skill_dir_paths(content: &str, _skill_name: &str) -> String {
    content.replace(
        "${CLAUDE_SKILL_DIR}/scripts/",
        "${CLAUDE_SKILL_DIR}/../../scripts/",
    )
}
```

> If scripts are NOT in a `scripts/` subdirectory but directly referenced by relative path, those references are left unchanged — the skill directory structure is preserved 1:1 inside the plugin.

### 5.10 Plugin Manifest Generation

```rust
/// Generate aipm.toml for a migrated plugin.
fn generate_plugin_manifest(artifact: &Artifact, plugin_name: &str) -> String {
    let type_str = match artifact.kind {
        ArtifactKind::Skill | ArtifactKind::Command => "skill",
    };

    let description = artifact.metadata.description.as_deref()
        .unwrap_or("Migrated from .claude/ configuration");

    let mut components = Vec::new();

    // Skills component
    components.push(format!(
        "skills = [\"skills/{}/SKILL.md\"]",
        artifact.name
    ));

    // Scripts component (if any)
    if !artifact.referenced_scripts.is_empty() {
        let scripts: Vec<String> = artifact.referenced_scripts.iter()
            .filter_map(|p| p.file_name())
            .map(|f| format!("\"scripts/{}\"", f.to_string_lossy()))
            .collect();
        components.push(format!("scripts = [{}]", scripts.join(", ")));
    }

    // Hooks component (if extracted)
    if artifact.metadata.hooks.is_some() {
        components.push("hooks = [\"hooks/hooks.json\"]".to_string());
    }

    let components_section = components.join("\n");

    format!(
        "[package]\n\
         name = \"{plugin_name}\"\n\
         version = \"0.1.0\"\n\
         type = \"{type_str}\"\n\
         edition = \"2024\"\n\
         description = \"{description}\"\n\
         \n\
         [components]\n\
         {components_section}\n"
    )
}
```

### 5.11 Plugin JSON Generation

```rust
/// Generate .claude-plugin/plugin.json for a migrated plugin.
fn generate_plugin_json(name: &str, metadata: &ArtifactMetadata) -> String {
    let description = metadata.description.as_deref()
        .unwrap_or("Migrated from .claude/ configuration");

    format!(
        "{{\n  \"name\": \"{name}\",\n  \"version\": \"0.1.0\",\n  \
         \"description\": \"{description}\"\n}}\n"
    )
}
```

### 5.12 Marketplace Registration

**New file: `crates/libaipm/src/migrate/registrar.rs`**

```rust
/// Append migrated plugins to marketplace.json without modifying existing entries.
pub fn register_plugins(
    ai_dir: &Path,
    plugin_names: &[String],
    fs: &dyn Fs,
) -> Result<(), Error> {
    let marketplace_path = ai_dir.join(".claude-plugin").join("marketplace.json");
    let content = fs.read_to_string(&marketplace_path)?;
    let mut json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| Error::MarketplaceJsonParse {
            path: marketplace_path.clone(),
            source: e,
        })?;

    let plugins = json.get_mut("plugins")
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| Error::MarketplaceJsonParse {
            path: marketplace_path.clone(),
            source: serde_json::Error::custom("missing 'plugins' array"),
        })?;

    for name in plugin_names {
        // Skip if already registered
        let already_registered = plugins.iter().any(|p| {
            p.get("name").and_then(|n| n.as_str()) == Some(name.as_str())
        });
        if already_registered { continue; }

        plugins.push(serde_json::json!({
            "name": name,
            "source": format!("./{name}"),
            "description": format!("Migrated from .claude/ configuration")
        }));
    }

    let output = serde_json::to_string_pretty(&json)?;
    fs.write_file(&marketplace_path, format!("{output}\n").as_bytes())?;
    Ok(())
}
```

### 5.13 Dry Run Report

When `--dry-run` is set, the orchestrator runs detection but skips emission and registration. Instead, it generates a structured markdown report.

**New file: `crates/libaipm/src/migrate/dry_run.rs`**

```rust
/// Generate dry-run report as markdown.
pub fn generate_report(
    artifacts: &[Artifact],
    existing_plugins: &HashSet<String>,
    source_name: &str,
) -> String {
    // Report structure:
    // # aipm migrate — Dry Run Report
    // **Source:** .claude/
    // **Date:** <timestamp>
    // **Artifacts found:** N
    //
    // ## Skills
    // ### deploy
    // - **Source:** .claude/skills/deploy/
    // - **Target:** .ai/deploy/
    // - **Files to copy:**
    //   - SKILL.md
    //   - scripts/deploy.sh
    // - **Manifest changes:**
    //   - New aipm.toml with type = "skill"
    //   - New .claude-plugin/plugin.json
    // - **marketplace.json:** append entry "deploy"
    // - **Path rewrites:** ${CLAUDE_SKILL_DIR}/scripts/ → ${CLAUDE_SKILL_DIR}/../../scripts/
    // - **Hooks extracted:** yes/no
    // - **Conflict:** none / renamed to deploy-renamed-1
    //
    // ## Legacy Commands
    // ### review
    // ...
    //
    // ## Summary
    // | Action | Count |
    // |--------|-------|
    // | Plugins to create | N |
    // | Marketplace entries to add | N |
    // | Name conflicts (auto-renamed) | N |
    // | Hooks to extract | N |
}
```

The report file is written to `<project-root>/aipm-migrate-dryrun-report.md`.

### 5.14 Orchestrator Function

**`crates/libaipm/src/migrate/mod.rs`** — main entry point:

```rust
/// Run the migration pipeline.
pub fn migrate(opts: &Options<'_>, fs: &dyn Fs) -> Result<MigrateResult, Error> {
    let ai_dir = opts.dir.join(".ai");
    let source_dir = opts.dir.join(opts.source);

    // 1. Validate prerequisites
    if !fs.exists(&ai_dir) {
        return Err(Error::MarketplaceNotFound(ai_dir));
    }
    if !fs.exists(&source_dir) {
        return Err(Error::SourceNotFound(source_dir));
    }

    // 2. Resolve detectors for this source
    let detectors = match opts.source {
        ".claude" => detector::claude_detectors(),
        _ => return Err(Error::SourceNotFound(source_dir)),
    };

    // 3. Run all detectors
    let mut all_artifacts = Vec::new();
    for detector in &detectors {
        let artifacts = detector.detect(&source_dir, fs)?;
        all_artifacts.extend(artifacts);
    }

    // 4. Collect existing plugin names for conflict detection
    let existing_plugins = collect_existing_plugin_names(&ai_dir, fs)?;

    // 5. Dry run — generate report and return
    if opts.dry_run {
        let report = dry_run::generate_report(
            &all_artifacts, &existing_plugins, opts.source
        );
        let report_path = opts.dir.join("aipm-migrate-dryrun-report.md");
        fs.write_file(&report_path, report.as_bytes())?;
        return Ok(MigrateResult {
            actions: vec![MigrateAction::DryRunReport { path: report_path }],
        });
    }

    // 6. Emit plugins
    let mut actions = Vec::new();
    let mut registered_names = Vec::new();
    let mut known_names = existing_plugins;
    let mut rename_counter = 0u32;

    for artifact in &all_artifacts {
        let (plugin_name, emit_actions) = emitter::emit_plugin(
            artifact, &ai_dir, &known_names, &mut rename_counter, fs
        )?;
        actions.extend(emit_actions);
        known_names.insert(plugin_name.clone());
        registered_names.push(plugin_name);
    }

    // 7. Register all in marketplace.json
    registrar::register_plugins(&ai_dir, &registered_names, fs)?;
    for name in &registered_names {
        actions.push(MigrateAction::MarketplaceRegistered {
            name: name.clone(),
        });
    }

    Ok(MigrateResult { actions })
}

/// Collect names of existing plugins in .ai/ directory.
fn collect_existing_plugin_names(
    ai_dir: &Path,
    fs: &dyn Fs,
) -> Result<HashSet<String>, Error> {
    let entries = fs.read_dir(ai_dir)?;
    Ok(entries.into_iter()
        .filter(|e| e.is_dir)
        .map(|e| e.name)
        .collect())
}
```

### 5.15 CLI Dispatch

**`crates/aipm/src/main.rs`** — Add dispatch arm:

```rust
Some(Commands::Migrate { dry_run, source, dir }) => {
    let dir = if dir == Path::new(".") {
        std::env::current_dir()?
    } else {
        dir
    };

    let opts = libaipm::migrate::Options {
        dir: &dir,
        source: &source,
        dry_run,
    };

    let result = libaipm::migrate::migrate(&opts, &libaipm::fs::Real)?;

    let mut stdout = std::io::stdout();
    for action in &result.actions {
        match action {
            MigrateAction::PluginCreated { name, source, plugin_type } => {
                let _ = writeln!(stdout,
                    "Migrated {plugin_type} '{name}' from {}",
                    source.display()
                );
            }
            MigrateAction::MarketplaceRegistered { name } => {
                let _ = writeln!(stdout,
                    "Registered '{name}' in marketplace.json"
                );
            }
            MigrateAction::Renamed { original_name, new_name, reason } => {
                let _ = writeln!(stdout,
                    "Warning: renamed '{original_name}' → '{new_name}' ({reason})"
                );
            }
            MigrateAction::Skipped { name, reason } => {
                let _ = writeln!(stdout,
                    "Skipped '{name}': {reason}"
                );
            }
            MigrateAction::DryRunReport { path } => {
                let _ = writeln!(stdout,
                    "Dry run report written to {}", path.display()
                );
            }
        }
    }
}
```

### 5.16 Generated Plugin Directory Layout

For a skill `deploy` in `.claude/skills/deploy/` with `SKILL.md` and `scripts/deploy.sh`:

```
.ai/
  deploy/                              ← new plugin directory
    aipm.toml                          ← [package] + [components]
    .claude-plugin/
      plugin.json                      ← name, version, description
    skills/
      deploy/
        SKILL.md                       ← copied + path-rewritten
    scripts/
      deploy.sh                        ← copied from referenced scripts
    hooks/                             ← only if frontmatter had hooks
      hooks.json
```

For a legacy command `review` in `.claude/commands/review.md`:

```
.ai/
  review/                              ← new plugin directory
    aipm.toml
    .claude-plugin/
      plugin.json
    skills/
      review/
        SKILL.md                       ← converted from review.md
                                          with disable-model-invocation: true
```

## 6. Alternatives Considered

| Option | Pros | Cons | Reason for Rejection |
|--------|------|------|---------------------|
| Move files instead of copy | Clean, no duplicates | Irreversible without git; breaks existing `.claude/` references immediately | Copy is safer — user can verify and delete originals manually |
| Auto-create `.ai/` if missing | Fewer steps for user | Conflates responsibilities of `init` and `migrate`; user may not realize a marketplace was created | Keeping commands orthogonal is clearer — `init` creates, `migrate` converts |
| Single monolithic scanner function | Simpler day-1 code | No extensibility path for agents/MCPs/etc. | Detector trait adds minimal complexity but enables all future detectors |
| Skip legacy commands | Smaller scope | Commands are a dead-end format; leaving them behind means users must manually convert | Auto-converting with `disable-model-invocation: true` is a clean migration path |

## 7. Cross-Cutting Concerns

### 7.1 Platform Compatibility

- Path handling uses `std::path::Path` throughout — platform-aware separators
- `Fs::read_dir` on Windows returns entries in filesystem order (not guaranteed alphabetical) — tests should sort results for determinism
- No symlinks or junctions are created by `migrate` (unlike `aipm install`)

### 7.2 Lint Compliance

- No `unwrap()`, `expect()`, `panic!()`, `println!()` — all prohibited by workspace lints
- Error handling via `?` operator and `thiserror` derive
- Output via `writeln!(stdout, ...)` with `let _ =`
- No `#[allow(...)]` attributes — if a lint fires, fix the code

### 7.3 Idempotency

- Running `migrate` twice on the same source is safe: existing plugins cause auto-rename (`deploy-renamed-1`), not overwrite
- Marketplace entries are deduplicated by name before appending
- Dry-run overwrites its previous report file (by design — always shows latest state)

### 7.4 Backward Compatibility

- `aipm migrate` is a new command — no existing behavior changes
- `.claude/` files are never modified or deleted (copy-only)
- `marketplace.json` gains entries but existing entries are untouched
- `.claude/settings.json` is NOT modified (no `enabledPlugins` changes)

## 8. Migration, Rollout, and Testing

### 8.1 Test Plan

#### 8.1.1 Architecture for Testability

All migration logic operates through `&dyn Fs`, enabling mock-based unit tests without touching the real filesystem. The `Detector` trait enables testing each detector in isolation. The emitter and registrar are pure functions that take artifacts and produce file writes through `Fs`.

Test layers:
- **Unit tests** (mock `Fs`): detector scan logic, emitter output, registrar JSON merge, name conflict resolution, path rewriting, frontmatter parsing, dry-run report generation
- **E2E tests** (`assert_cmd` + `tempfile`): full `aipm migrate` command with real filesystem
- **BDD scenarios** (cucumber-rs): human-readable acceptance criteria

#### 8.1.2 Unit Tests — Skill Detector

| Test | Description |
|------|-------------|
| `detect_skill_with_skill_md` | Skill dir with SKILL.md → returns 1 artifact with correct name and files |
| `detect_skill_without_skill_md` | Skill dir missing SKILL.md → skipped, returns 0 artifacts |
| `detect_skill_with_scripts` | Skill referencing `${CLAUDE_SKILL_DIR}/scripts/deploy.sh` → `referenced_scripts` populated |
| `detect_skill_extracts_frontmatter` | SKILL.md with name/description/hooks frontmatter → metadata populated |
| `detect_skill_no_skills_dir` | `.claude/` exists but no `skills/` subdirectory → returns empty Vec |
| `detect_skill_empty_skills_dir` | `skills/` exists but empty → returns empty Vec |
| `detect_multiple_skills` | Two skill directories → returns 2 artifacts |
| `detect_skill_nested_files` | Skill with `examples/` and `reference.md` → all files collected |

#### 8.1.3 Unit Tests — Command Detector

| Test | Description |
|------|-------------|
| `detect_command_md_file` | `commands/review.md` → returns 1 artifact with `ArtifactKind::Command` |
| `detect_command_adds_disable_model_invocation` | Metadata has `model_invocation_disabled = true` |
| `detect_command_with_frontmatter` | Command with existing frontmatter → metadata parsed correctly |
| `detect_command_without_frontmatter` | Plain markdown → metadata defaults with `model_invocation_disabled = true` |
| `detect_command_ignores_non_md` | Non-.md files in commands/ → skipped |
| `detect_command_ignores_directories` | Subdirectories in commands/ → skipped |
| `detect_no_commands_dir` | `.claude/` without `commands/` → returns empty Vec |

#### 8.1.4 Unit Tests — Emitter

| Test | Description |
|------|-------------|
| `emit_creates_plugin_directory_structure` | Correct dirs: `.claude-plugin/`, `skills/<name>/`, optionally `scripts/`, `hooks/` |
| `emit_generates_valid_aipm_toml` | Generated manifest round-trips through `parse_and_validate` |
| `emit_generates_plugin_json` | JSON has name, version, description |
| `emit_copies_skill_files` | All files from artifact.files present in output |
| `emit_copies_referenced_scripts` | Scripts from `referenced_scripts` present in `scripts/` |
| `emit_rewrites_claude_skill_dir` | `${CLAUDE_SKILL_DIR}/scripts/` → `${CLAUDE_SKILL_DIR}/../../scripts/` |
| `emit_extracts_hooks_to_json` | Frontmatter hooks → `hooks/hooks.json` with valid JSON |
| `emit_command_as_skill` | Command artifact → `skills/<name>/SKILL.md` with `disable-model-invocation: true` |
| `emit_command_wraps_with_frontmatter` | Command without frontmatter → SKILL.md gets frontmatter wrapper |

#### 8.1.5 Unit Tests — Name Conflict Resolution

| Test | Description |
|------|-------------|
| `resolve_name_no_conflict` | Name not in existing set → returns unchanged |
| `resolve_name_conflict_renames` | Name exists → returns `<name>-renamed-1` with `Renamed` action |
| `resolve_name_multiple_conflicts` | Two conflicts → sequential IDs: `-renamed-1`, `-renamed-2` |

#### 8.1.6 Unit Tests — Registrar

| Test | Description |
|------|-------------|
| `register_appends_to_empty_plugins_array` | Empty plugins array → adds entries |
| `register_appends_alongside_existing` | Existing entries preserved, new entries appended |
| `register_skips_already_registered` | Plugin with same name already in array → not duplicated |
| `register_preserves_marketplace_metadata` | `name`, `owner`, `metadata` fields unchanged |

#### 8.1.7 Unit Tests — Dry Run Report

| Test | Description |
|------|-------------|
| `dry_run_report_lists_all_artifacts` | Report contains section for each detected artifact |
| `dry_run_report_shows_conflict_renames` | Conflict artifacts show renamed target |
| `dry_run_report_shows_file_list` | Each artifact section lists files to copy |
| `dry_run_report_summary_table` | Report ends with summary counts table |

#### 8.1.8 Unit Tests — Fs Trait Extension

| Test | Description |
|------|-------------|
| `real_read_dir_lists_entries` | Real implementation returns correct entries for a temp directory |
| `real_read_dir_empty_dir` | Empty directory → empty Vec |
| `real_read_dir_nonexistent` | Nonexistent path → io::Error |
| `real_read_dir_distinguishes_files_and_dirs` | Mixed entries → correct `is_dir` flags |

#### 8.1.9 Unit Tests — Orchestrator

| Test | Description |
|------|-------------|
| `migrate_errors_if_no_ai_dir` | `.ai/` missing → `Error::MarketplaceNotFound` |
| `migrate_errors_if_no_source_dir` | `.claude/` missing → `Error::SourceNotFound` |
| `migrate_dry_run_writes_report` | `dry_run: true` → report file written, no plugin dirs created |
| `migrate_empty_source` | No skills or commands → empty result, no errors |
| `migrate_full_flow` | Skills + commands detected → plugins created + registered |

### 8.2 E2E Tests

**New file: `crates/aipm/tests/migrate_e2e.rs`**

| Test | Description |
|------|-------------|
| `migrate_skill_creates_plugin` | Set up `.claude/skills/deploy/SKILL.md`, run `aipm migrate`, verify `.ai/deploy/` exists with valid `aipm.toml` |
| `migrate_command_creates_plugin` | Set up `.claude/commands/review.md`, verify `.ai/review/skills/review/SKILL.md` has `disable-model-invocation: true` |
| `migrate_registers_in_marketplace` | Verify `marketplace.json` contains new plugin entries |
| `migrate_does_not_enable_plugin` | Verify `.claude/settings.json` `enabledPlugins` is NOT modified |
| `migrate_preserves_originals` | After migration, `.claude/skills/deploy/SKILL.md` still exists |
| `migrate_handles_name_conflict` | Pre-create `.ai/deploy/`, run migrate → plugin created as `deploy-renamed-1` |
| `migrate_dry_run_creates_report` | Run with `--dry-run`, verify `aipm-migrate-dryrun-report.md` exists and no plugin dirs created |
| `migrate_dry_run_no_side_effects` | `.ai/` unchanged after `--dry-run` |
| `migrate_no_ai_dir_errors` | No `.ai/` → error message mentions `aipm init` |
| `migrate_no_source_dir_errors` | No `.claude/` → error message |
| `migrate_empty_skills_dir` | `.claude/skills/` exists but empty → success with 0 actions |
| `migrate_multiple_skills` | Two skills → two plugins created and registered |
| `migrate_skill_with_scripts` | Skill referencing scripts → scripts copied to `scripts/` |
| `migrate_help_output` | `aipm migrate --help` shows expected flags |

### 8.3 BDD Feature Scenarios

**New file: `tests/features/manifest/migrate.feature`**

```gherkin
@p0 @manifest @migrate
Feature: Migrate AI tool configurations into marketplace plugins

  Rule: Skills are migrated as plugins

    Scenario: Migrate a single skill from .claude/skills/
      Given a workspace initialized with marketplace
      And a skill "deploy" exists in ".claude/skills/deploy/SKILL.md"
      When I run "aipm migrate"
      Then the command succeeds
      And a plugin directory exists at ".ai/deploy/"
      And ".ai/deploy/aipm.toml" contains 'name = "deploy"'
      And ".ai/deploy/aipm.toml" contains 'type = "skill"'
      And ".ai/deploy/skills/deploy/SKILL.md" exists
      And the marketplace.json contains plugin "deploy"

    Scenario: Original skill files are preserved after migration
      Given a workspace initialized with marketplace
      And a skill "deploy" exists in ".claude/skills/deploy/SKILL.md"
      When I run "aipm migrate"
      Then ".claude/skills/deploy/SKILL.md" still exists

    Scenario: Migrated plugins are not auto-enabled
      Given a workspace initialized with marketplace
      And a skill "deploy" exists in ".claude/skills/deploy/SKILL.md"
      When I run "aipm migrate"
      Then ".claude/settings.json" does not contain "deploy@local-repo-plugins"

  Rule: Legacy commands are converted to skills

    Scenario: Migrate a legacy command with disable-model-invocation
      Given a workspace initialized with marketplace
      And a command "review" exists in ".claude/commands/review.md"
      When I run "aipm migrate"
      Then ".ai/review/skills/review/SKILL.md" contains "disable-model-invocation: true"

  Rule: Name conflicts are resolved by renaming

    Scenario: Plugin name conflict triggers auto-rename
      Given a workspace initialized with marketplace
      And a plugin directory exists at ".ai/deploy/"
      And a skill "deploy" exists in ".claude/skills/deploy/SKILL.md"
      When I run "aipm migrate"
      Then a plugin directory exists at ".ai/deploy-renamed-1/"
      And the output contains "renamed 'deploy'"

  Rule: Dry run produces report without side effects

    Scenario: Dry run generates report file
      Given a workspace initialized with marketplace
      And a skill "deploy" exists in ".claude/skills/deploy/SKILL.md"
      When I run "aipm migrate --dry-run"
      Then "aipm-migrate-dryrun-report.md" exists
      And no plugin directory exists at ".ai/deploy/"

  Rule: Prerequisites are validated

    Scenario: Error when marketplace directory is missing
      Given an empty directory
      And a skill "deploy" exists in ".claude/skills/deploy/SKILL.md"
      When I run "aipm migrate"
      Then the command fails
      And the error contains "aipm init"
```

## 9. Implementation Order

| Step | Files | Description |
|------|-------|-------------|
| 1 | `crates/libaipm/src/fs.rs` | Extend `Fs` trait with `read_dir` + `DirEntry`; implement for `Real`; update existing mock impls |
| 2 | `crates/libaipm/src/migrate/mod.rs` | Create module with `Options`, `MigrateResult`, `MigrateAction`, `Artifact`, `ArtifactKind`, `ArtifactMetadata`, `Error` types |
| 3 | `crates/libaipm/src/migrate/detector.rs` | `Detector` trait + `claude_detectors()` factory |
| 4 | `crates/libaipm/src/migrate/skill_detector.rs` | `SkillDetector` with frontmatter parsing, script reference extraction, recursive file collection |
| 5 | `crates/libaipm/src/migrate/command_detector.rs` | `CommandDetector` with command-to-skill conversion logic |
| 6 | `crates/libaipm/src/migrate/emitter.rs` | Plugin directory creation, file copying, path rewriting, hooks extraction, manifest/JSON generation |
| 7 | `crates/libaipm/src/migrate/registrar.rs` | Marketplace JSON append logic |
| 8 | `crates/libaipm/src/migrate/dry_run.rs` | Dry-run report generator |
| 9 | `crates/libaipm/src/migrate/mod.rs` | `migrate()` orchestrator function wiring detectors → emitter → registrar |
| 10 | `crates/libaipm/src/lib.rs` | Add `pub mod migrate;` |
| 11 | `crates/aipm/src/main.rs` | Add `Commands::Migrate` variant + dispatch arm |
| 12 | `crates/libaipm/src/migrate/*.rs` | Unit tests for all modules (`#[cfg(test)]` blocks) |
| 13 | `crates/aipm/tests/migrate_e2e.rs` | E2E tests |
| 14 | `tests/features/manifest/migrate.feature` | BDD scenarios (wire into `bdd.rs` filter list) |
| 15 | All | `cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check` |
| 16 | All | Coverage gate: `cargo +nightly llvm-cov report --doctests --branch --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)'` ≥ 89% |

## 10. Open Questions / Unresolved Issues

- [x] **Should `aipm migrate` auto-create `.ai/`?** No — require `aipm init` first. Error with helpful message.
- [x] **Copy or move files?** Copy (non-destructive). Originals left intact.
- [x] **Extend `Fs` trait or separate abstraction?** Extend `Fs` with `read_dir`.
- [x] **How to handle `${CLAUDE_SKILL_DIR}` paths?** Rewrite `scripts/` references to `../../scripts/`.
- [x] **Migrate legacy commands?** Yes — convert to skills with `disable-model-invocation: true`.
- [x] **`--dry-run` on day 1?** Yes — full structured markdown report (`aipm-migrate-dryrun-report.md`).
- [x] **Name conflict strategy?** Auto-rename to `<name>-renamed-<id>`, warn, continue.
- [x] **Extract frontmatter hooks?** Yes — to `hooks/hooks.json`.
- [ ] **Should `--dry-run` report include a diff preview of `marketplace.json` changes?** TBD — could be useful but adds complexity.
- [ ] **Should the `--source` flag accept comma-separated values for scanning multiple sources in one invocation?** TBD — deferred until multi-source support is needed.
