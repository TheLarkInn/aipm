---
date: 2026-03-24 07:27:06 PDT
researcher: Claude
git_commit: 732c72b2ad574f48808a89236e624c5a2650053f
branch: main
repository: aipm
topic: "How and where are aipm.toml plugin manifest files generated during aipm init and aipm migrate commands?"
tags: [research, codebase, aipm-toml, manifest, init, migrate, workspace-init, emitter]
status: complete
last_updated: 2026-03-24
last_updated_by: Claude
---

# Research: `aipm.toml` Generation in Init and Migrate Commands

## Research Question

How and where are `aipm.toml` plugin manifest files generated during `aipm init` and `aipm migrate` commands? Document the full code paths, template logic, and trigger conditions for `aipm.toml` creation so we can design a flag/mechanism to suppress manifest generation when marketplace linking/dependency management is not yet available.

## Summary

There are **five distinct `aipm.toml` generation paths** across two binaries (`aipm` and `aipm-pack`). All five use hardcoded `format!()` string templates — none serialize the `Manifest` struct via serde. The migrate command **always** generates per-plugin `aipm.toml` files with no flag or conditional to suppress them. The init commands have partial control via `--no-starter` (suppresses the starter plugin manifest) but no general "skip manifest" option.

| Path | Binary | Trigger | Output Location | Can Be Suppressed? |
|------|--------|---------|-----------------|-------------------|
| Workspace manifest | `aipm` | `--workspace` flag | `{dir}/aipm.toml` | Yes — don't pass `--workspace` |
| Starter plugin manifest | `aipm` | `--marketplace` (default) | `{dir}/.ai/starter-aipm-plugin/aipm.toml` | Yes — pass `--no-starter` |
| Package init manifest | `aipm-pack` | Always on `init` | `{dir}/aipm.toml` | No |
| Single-artifact migrate | `aipm` | Each detected artifact | `{dir}/.ai/{name}/aipm.toml` | Only via `--dry-run` (no files written) |
| Package-scoped migrate | `aipm` | Multiple artifacts in sub-package | `{dir}/.ai/{name}/aipm.toml` | Only via `--dry-run` (no files written) |

## Detailed Findings

### 1. Consumer CLI: `aipm init`

#### Entry Point

[`crates/aipm/src/main.rs:71`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/aipm/src/main.rs#L71) — `Commands::Init` match arm dispatches to `libaipm::workspace_init::init()`.

CLI flags (declared at lines 22-43):
- `--yes` / `-y` — skip interactive prompts
- `--workspace` — generate workspace `aipm.toml`
- `--marketplace` — generate `.ai/` directory (this is the default when no flags given)
- `--no-starter` — skip starter plugin within marketplace
- `dir` — positional argument, defaults to `"."`

#### Path A: Workspace Manifest (`--workspace`)

**Code path**: `main.rs:71` → wizard resolution → [`workspace_init::init()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/workspace_init/mod.rs#L98) → [`init_workspace()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/workspace_init/mod.rs#L106) → [`generate_workspace_manifest()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/workspace_init/mod.rs#L146)

**Guard**: If `aipm.toml` already exists at the target dir, returns `Error::WorkspaceAlreadyInitialized` (line 129-131).

**Generated content** (lines 147-164):
```toml
# AI Plugin Manager — Workspace Configuration
# Docs: https://github.com/thelarkinn/aipm

[workspace]
members = [".ai/*"]
plugins_dir = ".ai"

# Shared dependency versions for all workspace members.
# Members reference these via: dep = { workspace = "^" }
# [workspace.dependencies]

# Direct registry installs (available project-wide).
# [dependencies]

# Environment requirements for all plugins in this workspace.
# [environment]
# requires = ["git"]
```

**Validation**: Round-trip validated via `crate::manifest::parse_and_validate(&content, None)` at line 137.

**Write**: `fs.write_file(&manifest_path, content.as_bytes())` at line 141.

#### Path B: Starter Plugin Manifest (`--marketplace`, default)

**Code path**: `main.rs:71` → wizard resolution → [`workspace_init::init()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/workspace_init/mod.rs#L98) → [`scaffold_marketplace()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/workspace_init/mod.rs#L171) → [`generate_starter_manifest()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/workspace_init/mod.rs#L243)

**Guard**: If `.ai/` directory already exists, returns `Error::MarketplaceAlreadyExists` (line 172-175).

**Suppression**: If `no_starter == true`, function returns early at line 195-197 after creating `.ai/.gitignore` and `.ai/.claude-plugin/marketplace.json`, **without** creating the starter plugin or its `aipm.toml`.

**Generated content** (lines 244-260):
```toml
[package]
name = "starter-aipm-plugin"
version = "0.1.0"
type = "composite"
edition = "2024"
description = "Default starter plugin — scaffold new plugins, scan your marketplace, and log tool usage"

# [dependencies]
# Add registry dependencies here, e.g.:
# shared-skill = "^1.0"

[components]
skills = ["skills/scaffold-plugin/SKILL.md"]
agents = ["agents/marketplace-scanner.md"]
hooks = ["hooks/hooks.json"]
scripts = ["scripts/scaffold-plugin.ts"]
```

**Validation**: Round-trip validated with component path checking via `crate::manifest::parse_and_validate(&starter_manifest, Some(&starter))` at line 228.

**Write**: `fs.write_file(&starter.join("aipm.toml"), ...)` at line 225. Component files (SKILL.md, marketplace-scanner.md, hooks.json, scaffold-plugin.ts) are written before validation (lines 209-221).

#### Default Behavior (no flags)

When neither `--workspace` nor `--marketplace` is explicitly set, the wizard defaults to `(false, true, false)` — marketplace only ([`wizard.rs:150-157`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/aipm/src/wizard.rs#L150)). This means:
- **No** root `aipm.toml` is created
- **Yes**, starter plugin `aipm.toml` is created at `.ai/starter-aipm-plugin/aipm.toml`

---

### 2. Author CLI: `aipm-pack init`

#### Entry Point

[`crates/aipm-pack/src/main.rs:48`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/aipm-pack/src/main.rs#L48) — `Commands::Init` match arm dispatches to `libaipm::init::init()`.

CLI flags (declared at lines 23-41):
- `--yes` / `-y` — skip interactive prompts
- `--name` — package name (optional)
- `--type` — plugin type: skill, agent, mcp, hook, lsp, composite (optional, defaults to composite)
- `dir` — positional argument, defaults to `"."`

**Code path**: `main.rs:48` → wizard resolution → [`init::init()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/init.rs#L57) → [`generate_manifest()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/init.rs#L187)

**Guard**: If `aipm.toml` already exists, returns `Error::AlreadyInitialized` (line 61-64).

**Name resolution**: Uses `--name` if provided, otherwise extracts from directory name (lines 67-74). Validated via `is_valid_package_name()` (lines 99-128).

**Generated content** (lines 197-203):
```toml
[package]
name = "{name}"
version = "0.1.0"
type = "{type_str}"
edition = "2024"
```

**No round-trip validation**: Unlike the consumer CLI, this path does NOT call `parse_and_validate()`.

**Write**: `fs.write_file(&manifest_path, toml_content.as_bytes())` at line 93.

**Cannot be suppressed**: There is no flag or condition to skip `aipm.toml` generation in `aipm-pack init`.

---

### 3. `aipm migrate`

#### Entry Point

[`crates/aipm/src/main.rs:111`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/aipm/src/main.rs#L111) — `Commands::Migrate` match arm dispatches to `libaipm::migrate::migrate()`.

CLI flags (declared at lines 46-64):
- `--dry-run` — preview without writing files
- `--source` — specific source directory (e.g., `.claude`)
- `--max-depth` — limit recursive directory walk depth
- `dir` — positional argument, defaults to `"."`

#### Prerequisite

The `.ai/` directory **must already exist** ([`migrate/mod.rs:182-185`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/migrate/mod.rs#L182)). If absent, returns `Error::MarketplaceNotFound`. This means `aipm init` (which creates `.ai/`) must run before `aipm migrate`.

#### Two Migration Modes

**Single-source mode** (`--source` provided): [`migrate_single_source()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/migrate/mod.rs#L196) — scans a single `.claude/` directory.

**Recursive mode** (default, no `--source`): [`migrate_recursive()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/migrate/mod.rs#L250) — uses the `ignore` crate to walk the project tree, discovering all `.claude/` directories (including in monorepo sub-packages).

#### Discovery Pipeline

1. [`discovery::discover_claude_dirs()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/migrate/discovery.rs#L33) — gitignore-aware walk, finds `.claude/` dirs, extracts package names from parent dirs
2. Detectors run against each source dir:
   - [`SkillDetector`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/migrate/skill_detector.rs#L11) — scans `skills/` for `SKILL.md` files
   - [`CommandDetector`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/migrate/command_detector.rs#L12) — scans `commands/` for `.md` files
3. Artifacts grouped into `PluginPlan`s — package-scoped (merged) or individual

#### Manifest Generation in Migrate

**Single-artifact plugins**: [`generate_plugin_manifest()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/migrate/emitter.rs#L567) — called from [`emit_plugin()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/migrate/emitter.rs#L28) (line 99) and [`emit_plugin_with_name()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/migrate/emitter.rs#L229) (line 298).

Generated template (lines 600-609):
```toml
[package]
name = "{plugin_name}"
version = "0.1.0"
type = "{type_str}"
edition = "2024"
description = "{description}"

[components]
{components_section}
```

Where:
- `type` is always `"skill"` (both `Skill` and `Command` artifact kinds map to `"skill"` via `ArtifactKind::to_type_string()` at `mod.rs:29`)
- `description` comes from artifact metadata or defaults to `"Migrated from .claude/ configuration"`
- `[components]` always includes `skills = [...]`, conditionally adds `scripts = [...]` and `hooks = [...]`

**Package-scoped plugins** (multiple artifacts from same sub-package): [`generate_package_manifest()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/migrate/emitter.rs#L446) — called from [`emit_package_plugin()`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/migrate/emitter.rs#L317) (line 424).

Same template structure. `type` is `"composite"` when artifacts span both skills and commands, otherwise `"skill"`.

**No validation**: Neither `generate_plugin_manifest()` nor `generate_package_manifest()` calls `parse_and_validate()`.

**Write locations**:
- Single-source: `{ai_dir}/{plugin_name}/aipm.toml` (emitter.rs:99)
- Recursive single-artifact: `{ai_dir}/{final_name}/aipm.toml` (emitter.rs:298)
- Recursive package-scoped: `{ai_dir}/{plugin_name}/aipm.toml` (emitter.rs:424)

#### Suppression in Migrate

The **only** way to prevent `aipm.toml` creation during migrate is `--dry-run`, which writes a markdown report instead of files (checked at `mod.rs:222-227` for single-source, `mod.rs:311-317` for recursive). There is no flag to migrate plugin files without generating manifests.

---

### 4. Manifest Module (Schema & Validation)

The manifest module at [`crates/libaipm/src/manifest/`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/manifest/mod.rs) provides:

- **Deserialization** via `toml::from_str()` into the `Manifest` struct
- **Validation** of name format, semver, dependency versions, plugin types, component paths
- **No serialization** — no `Serialize` derive, no `toml::to_string()`

All five generation paths use `format!()` string templates, independent of the type system. This means generated manifests are structurally guaranteed by the template strings, not by the type definitions.

Key types in [`manifest/types.rs`](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/manifest/types.rs):
- `Manifest` — top-level, all-`Option` fields, `deny_unknown_fields`
- `Package` — `[package]` section with `name`, `version`, `description`, `type`, `edition`, `files`
- `Workspace` — `[workspace]` section with `members`, `plugins_dir`, `dependencies`
- `Components` — nine `Option<Vec<String>>` fields for component declarations
- `PluginType` — enum: Skill, Agent, Mcp, Hook, Lsp, Composite

---

### 5. Cross-Cutting Patterns

#### No Constant for `"aipm.toml"`
The filename `"aipm.toml"` appears as a string literal in every location — there is no shared constant. This affects **18 production/test files** across the codebase.

#### Filesystem Abstraction
All file I/O goes through the [`Fs` trait](https://github.com/TheLarkInn/aipm/blob/732c72b2ad574f48808a89236e624c5a2650053f/crates/libaipm/src/fs.rs#L21) (except `manifest::load()` which uses `std::fs` directly), enabling mock-based testing.

#### Hardcoded String Templates
All five generators use `format!()` with inline TOML strings. Version is always `"0.1.0"`, edition is always `"2024"`.

## Code References

### Init paths
- `crates/aipm/src/main.rs:71` — Consumer CLI init dispatch
- `crates/aipm/src/wizard.rs:150-157` — Default flag resolution (marketplace only)
- `crates/libaipm/src/workspace_init/mod.rs:98-122` — Core init orchestrator
- `crates/libaipm/src/workspace_init/mod.rs:125-142` — `init_workspace()` — workspace manifest
- `crates/libaipm/src/workspace_init/mod.rs:146-165` — `generate_workspace_manifest()`
- `crates/libaipm/src/workspace_init/mod.rs:171-230` — `scaffold_marketplace()` — starter plugin
- `crates/libaipm/src/workspace_init/mod.rs:243-261` — `generate_starter_manifest()`
- `crates/aipm-pack/src/main.rs:48` — Author CLI init dispatch
- `crates/libaipm/src/init.rs:57-96` — Package init core
- `crates/libaipm/src/init.rs:187-204` — `generate_manifest()`

### Migrate paths
- `crates/aipm/src/main.rs:111-151` — Migrate CLI dispatch
- `crates/libaipm/src/migrate/mod.rs:181-193` — Migrate orchestrator
- `crates/libaipm/src/migrate/mod.rs:196-248` — `migrate_single_source()`
- `crates/libaipm/src/migrate/mod.rs:250-371` — `migrate_recursive()`
- `crates/libaipm/src/migrate/discovery.rs:33-96` — `discover_claude_dirs()`
- `crates/libaipm/src/migrate/emitter.rs:28` — `emit_plugin()` (single-source)
- `crates/libaipm/src/migrate/emitter.rs:229` — `emit_plugin_with_name()` (recursive single)
- `crates/libaipm/src/migrate/emitter.rs:317` — `emit_package_plugin()` (recursive package)
- `crates/libaipm/src/migrate/emitter.rs:446-491` — `generate_package_manifest()`
- `crates/libaipm/src/migrate/emitter.rs:567-610` — `generate_plugin_manifest()`

### Manifest module
- `crates/libaipm/src/manifest/mod.rs:21` — `parse()`
- `crates/libaipm/src/manifest/mod.rs:33` — `parse_and_validate()`
- `crates/libaipm/src/manifest/types.rs:10-42` — `Manifest` struct
- `crates/libaipm/src/manifest/types.rs:45-65` — `Package` struct
- `crates/libaipm/src/manifest/validate.rs:76-111` — `validate()` entry point

## Architecture Documentation

### Manifest Generation Decision Tree

```
aipm init
├── --workspace → generate_workspace_manifest() → {dir}/aipm.toml
├── --marketplace (default)
│   ├── --no-starter → NO aipm.toml (only .ai/.gitignore + marketplace.json)
│   └── (default) → generate_starter_manifest() → {dir}/.ai/starter-aipm-plugin/aipm.toml
└── --workspace --marketplace → BOTH of the above

aipm-pack init
└── always → generate_manifest() → {dir}/aipm.toml

aipm migrate
├── --dry-run → NO aipm.toml (markdown report only)
├── --source .claude → emit_plugin() per artifact → {dir}/.ai/{name}/aipm.toml
└── (default recursive)
    ├── single-artifact plans → emit_plugin_with_name() → {dir}/.ai/{name}/aipm.toml
    └── package-scoped plans → emit_package_plugin() → {dir}/.ai/{name}/aipm.toml
```

### Key Observation for the Design Task

The migrate command has **no mechanism** to skip `aipm.toml` generation other than `--dry-run` (which skips ALL file creation). The emit functions (`emit_plugin`, `emit_plugin_with_name`, `emit_package_plugin`) unconditionally generate and write `aipm.toml` as part of plugin directory creation. The manifest generation is tightly coupled to the emission step — there is no intermediate representation that separates "create plugin directory with files" from "write manifest".

Similarly, the starter plugin in `scaffold_marketplace()` couples the `aipm.toml` generation with the rest of the plugin scaffolding. The `--no-starter` flag suppresses the entire starter plugin (directory, components, AND manifest), but there is no option to create the starter plugin directory/components without the manifest.

## Historical Context (from research/)

- `research/docs/2026-03-23-aipm-migrate-command.md` — Research backing the migrate command design. Documents the Scanner-Detector-Emitter architecture and all `.claude/` artifact types.
- `research/docs/2026-03-23-recursive-claude-discovery-parallel-migrate.md` — Research on recursive discovery and parallel emission for monorepos.
- `research/docs/2026-03-16-aipm-init-workspace-marketplace.md` — Research backing the init command. Documents the decision to use `.ai/` as the plugin directory.
- `research/docs/2026-03-20-30-better-default-plugin.md` — Research on the starter plugin design with scaffold, scanner, and logging components.
- `specs/2026-03-23-aipm-migrate-command.md` — Primary spec for the migrate command.
- `specs/2026-03-23-recursive-migrate-discovery.md` — Spec extending migrate with recursive `.claude/` discovery.
- `specs/2026-03-16-aipm-init-workspace-marketplace.md` — Primary spec for `aipm init`.
- `specs/2026-03-20-better-default-plugin.md` — Spec for the starter plugin replacement.

## Related Research

- `research/docs/2026-03-09-manifest-format-comparison.md` — TOML format justification for `aipm.toml`
- `research/docs/2026-03-09-cargo-core-principles.md` — Cargo's workspace/manifest model (informing aipm's design)
- `research/docs/2026-03-09-pnpm-core-principles.md` — pnpm's workspace protocol and catalogs (informing aipm's dependency model)

## Open Questions

1. **Scope of suppression**: Should a `--no-manifest` flag suppress only the per-plugin `aipm.toml`, or also the workspace root `aipm.toml`? The workspace manifest and plugin manifests serve different purposes.

2. **Migrate-specific or global**: Should the flag live only on `aipm migrate`, or also on `aipm init` (for the starter plugin) and `aipm-pack init`?

3. **Partial migration**: If manifests are suppressed during migrate, how will the migrated plugins be registered in `marketplace.json`? The registrar (`registrar.rs:10-47`) currently runs after emission and depends on the plugin directory existing — but it does not read `aipm.toml`.

4. **Future reconciliation**: When marketplace linking/dependency management is implemented, will there be a command to retroactively generate `aipm.toml` files for plugins that were migrated without them? Or will users re-run `migrate`?

5. **Plugin validity without manifest**: Claude Code's plugin discovery (via `marketplace.json` + `extraKnownMarketplaces`) does not require `aipm.toml` — it reads `plugin.json` and component files directly. The `aipm.toml` is an aipm-specific concern for dependency management and publishing. This suggests suppression during migrate is safe for local-only use.
