# AIPM — AI Plugin Manager

[![CI](https://github.com/TheLarkInn/aipm/actions/workflows/ci.yml/badge.svg)](https://github.com/TheLarkInn/aipm/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/TheLarkInn/aipm/graph/badge.svg)](https://codecov.io/gh/TheLarkInn/aipm)
[![NuGet](https://img.shields.io/nuget/v/aipm)](https://www.nuget.org/packages/aipm)

A production-grade package manager for AI plugin primitives (skills, agents, MCP servers, hooks). Think npm/Cargo, but purpose-built for the AI plugin ecosystem.

AIPM ships as a **single Rust binary** with **zero runtime dependencies**:

| Binary | Role | Commands |
|--------|------|----------|
| **`aipm`** | CLI | `init`, `install`, `update`, `uninstall`, `link`, `unlink`, `list`, `lint`, `migrate`, `make`, `pack`, `lsp` |

It works across .NET, Python, Node.js, and Rust projects with no runtime dependency.

## Install

### Shell (Linux / macOS)

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/thelarkinn/aipm/releases/latest/download/aipm-installer.sh | sh
```

### PowerShell (Windows)

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/thelarkinn/aipm/releases/latest/download/aipm-installer.ps1 | iex"
```

> Installers are provided by [cargo-dist](https://opensource.axo.dev/cargo-dist/). Run `aipm-update` to self-update.

### Azure DevOps Pipeline (NuGet)

For Azure DevOps pipelines that prefer NuGet over `curl | sh`, restore `aipm` from [nuget.org](https://www.nuget.org/packages/aipm) into the global packages folder and prepend the correct per-RID binary directory to `PATH`:

```yaml
variables:
  AIPM_VERSION: '0.22.3'
  NUGET_PACKAGES: $(Pipeline.Workspace)/.nuget/packages

steps:
  - task: UseDotNet@2
    inputs: { packageType: sdk, version: 8.x }

  - pwsh: |
      New-Item -ItemType Directory -Force -Path "$(Agent.TempDirectory)/aipm-fetch" | Out-Null
      @'
      <Project Sdk="Microsoft.Build.NoTargets/3.7.0">
        <PropertyGroup>
          <TargetFramework>net8.0</TargetFramework>
          <DisableImplicitNuGetFallbackFolder>true</DisableImplicitNuGetFallbackFolder>
        </PropertyGroup>
        <ItemGroup>
          <PackageDownload Include="aipm" Version="[$(env:AIPM_VERSION)]" />
        </ItemGroup>
      </Project>
      '@ | Set-Content "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj"
    displayName: 'Generate aipm download-only project'

  - script: dotnet restore "$(Agent.TempDirectory)/aipm-fetch/fetch.csproj"

  - pwsh: |
      switch ("$(Agent.OS)") { 'Windows_NT'{$o='win';$x='aipm.exe'} 'Linux'{$o='linux';$x='aipm'} 'Darwin'{$o='osx';$x='aipm'} }
      $a = if ("$(Agent.OSArchitecture)".ToLower() -eq 'arm64') { 'arm64' } else { 'x64' }
      $bin = "$env:NUGET_PACKAGES/aipm/$env:AIPM_VERSION/runtimes/$o-$a/native"
      if ("$(Agent.OS)" -ne 'Windows_NT') { chmod +x "$bin/$x" }
      Write-Host "##vso[task.prependpath]$bin"

  - script: aipm --version
```

Public nuget.org needs no service connection or `NuGetAuthenticate@1`. The package ships binaries for `win-x64`, `linux-x64`, `osx-x64`, `osx-arm64`. See [`docs/guides/install-aipm-nuget-ado.md`](docs/guides/install-aipm-nuget-ado.md) for a step-by-step guide including version pinning, RID resolution, caching, and Artifacts proxy usage.

### Build from Source

```bash
cargo build --workspace          # build all crates
cargo test --workspace           # run all tests
```

---

## `aipm` — Consumer CLI

Manages AI plugin workspaces: scaffolding, installing plugins from multiple sources, migrating existing configurations, and linting for quality issues.

### Global Flags

| Flag | Description |
|------|-------------|
| `-v, --verbose` | Increase verbosity (`-v` info, `-vv` debug, `-vvv` trace); default level is warn |
| `-q, --quiet` | Decrease verbosity (`-q` error only, `-qq` silent) |
| `--log-format <FMT>` | Tracing output format on stderr: `text` (default) or `json` |

**Environment variable:** `AIPM_LOG` overrides CLI verbosity flags and accepts [tracing `EnvFilter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html) directives (e.g., `AIPM_LOG=debug` or `AIPM_LOG=libaipm::installer=trace`).

**Log file:** All runs append `DEBUG`-level diagnostics to `<system-temp>/aipm-YYYY-MM-DD.log` (daily rotation, 7-day retention) regardless of stderr verbosity.

See also: [`docs/guides/verbosity-and-logging.md`](docs/guides/verbosity-and-logging.md) for CI integration, filtering examples, and log file management.

### `aipm init`

Scaffold a workspace with a `.ai/` local marketplace and tool-specific settings.

```
aipm init [OPTIONS] [DIR]
```

| Flag | Description |
|------|-------------|
| `-y, --yes` | Skip interactive prompts, use defaults |
| `--workspace` | Generate a workspace-level `aipm.toml` with `[workspace]` section |
| `--marketplace` | Create `.ai/` marketplace directory with tool settings |
| `--no-starter` | Skip the starter plugin (bare `.ai/` directory only) |
| `--manifest` | Generate `aipm.toml` manifests for each plugin (opt-in) |
| `--name <NAME>` | Custom marketplace name (default: `local-repo-plugins`) |

When run on a TTY without `--yes`, launches an interactive wizard.

**What it creates:**
- `.ai/` — local marketplace directory
- `.ai/starter-aipm-plugin/` — starter plugin (unless `--no-starter`)
- `.claude/settings.json` — Claude Code marketplace registration (created/merged at project root)
- `aipm.toml` — workspace manifest (with `--workspace`)

See also: [`docs/guides/init.md`](docs/guides/init.md) for a full walkthrough of initialization modes, tool settings integration, and next steps.

### `aipm migrate`

Migrate existing AI tool configurations into marketplace plugins. Supports two source ecosystems: Claude Code (`.claude/`) and Copilot CLI (`.github/`). Detects skills, agents, MCP servers, hooks, commands, output styles, extensions, and LSP servers.

```
aipm migrate [OPTIONS] [DIR]
```

| Flag | Description |
|------|-------------|
| `--dry-run` | Preview migration without writing files (generates report) |
| `--destructive` | Remove migrated source files after successful migration (interactive prompt if omitted on TTY) |
| `--source <SRC>` | Source folder to scan (e.g., `.claude`). Omit to discover recursively |
| `--max-depth <N>` | Maximum depth for recursive discovery |
| `--manifest` | Generate `aipm.toml` manifests for migrated plugins |

**Claude Code (`.claude/`) artifact types:** skills (`SKILL.md`), agents (`agents/*.md`), MCP servers (`.mcp.json`), hooks (`hooks.json`), commands (`commands/*.md`), output styles.

**Copilot CLI (`.github/`) artifact types:** skills (`.github/skills/<name>/`), agents (`.github/agents/<name>.md` or `<name>.agent.md`), MCP servers (`.copilot/mcp-config.json`), hooks (`.github/hooks.json`), GitHub extensions (`.github/extensions/<name>/`), LSP servers (`.github/lsp.json`).

See also: [`docs/guides/migrate.md`](docs/guides/migrate.md) for a comprehensive reference, or [`docs/guides/migrating-existing-configs.md`](docs/guides/migrating-existing-configs.md) for a step-by-step walkthrough.

### `aipm install`

Install a plugin from the registry, a git repository, a GitHub shorthand, a local path, or a marketplace.

```
aipm install [OPTIONS] [PACKAGE]
```

| Flag | Description |
|------|-------------|
| `--locked` | CI mode: fail if lockfile doesn't match manifest |
| `--registry <REG>` | Use a specific registry |
| `--global` | Install globally (available to all projects) |
| `--engine <ENGINE>` | Restrict a global install to a specific engine (e.g., `claude`, `copilot`) |
| `--plugin-cache <POLICY>` | Download cache policy: `auto` (default), `cache-only`, `skip`, `force-refresh`, `no-refresh` |
| `--dir <DIR>` | Project directory (default: `.`) |

**Package spec formats:**

| Format | Example | Description |
|--------|---------|-------------|
| Registry name | `code-review@^1.0` | Semver range from the default registry |
| `github:` | `github:org/repo:plugin@main` | GitHub repo shorthand |
| `git:` | `git:https://example.com/repo.git:plugin@v1` | Arbitrary git URL |
| `local:` | `local:./path/to/plugin` | Local filesystem path |
| `market:` / `marketplace:` | `market:plugin-name@org/marketplace-repo` | Named marketplace (`mp:` short alias also accepted) |

Omit `PACKAGE` to install all dependencies from `aipm.toml`.

> **⚠️ Registry status:** The bare registry-name format (`code-review@^1.0`) is fully parsed and stored in the manifest, but the registry download step is not yet connected to the CLI binary. Using this format will produce an error today. The `local:`, `git:`, `github:`, and `marketplace:` formats all work end-to-end. Track progress at [#5](https://github.com/TheLarkInn/aipm/issues/5).

**Global installs** write to `~/.aipm/registry/` and are available across all projects. Use `--engine` to scope a plugin to a specific AI tool.

See also: [`docs/guides/install-marketplace-plugin.md`](docs/guides/install-marketplace-plugin.md), [`docs/guides/install-local-plugin.md`](docs/guides/install-local-plugin.md), [`docs/guides/install-git-plugin.md`](docs/guides/install-git-plugin.md), [`docs/guides/global-plugins.md`](docs/guides/global-plugins.md), [`docs/guides/cache-management.md`](docs/guides/cache-management.md).

### `aipm update`

Update packages to their latest compatible versions.

```
aipm update [OPTIONS] [PACKAGE]
```

| Flag | Description |
|------|-------------|
| `--dir <DIR>` | Project directory (default: `.`) |

Omit `PACKAGE` to update all dependencies. Unlike `install`, `update` resolves the latest version within the declared version range and rewrites the lockfile.

See also: [`docs/guides/update.md`](docs/guides/update.md) for the Cargo-model lockfile semantics, CI mode, and version-range upgrade patterns.

### `aipm uninstall`

Remove an installed plugin from the project or the global registry.

```
aipm uninstall [OPTIONS] <PACKAGE>
```

| Flag | Description |
|------|-------------|
| `--global` | Remove from the global registry |
| `--engine <ENGINE>` | Remove from a specific engine only (global installs) |
| `--dir <DIR>` | Project directory (default: `.`; ignored with `--global`) |

See also: [`docs/guides/uninstall.md`](docs/guides/uninstall.md) for project-level and global uninstall examples.

### `aipm link`

Link a local plugin directory for development, overriding the registry version.

```
aipm link [OPTIONS] <PATH>
```

| Flag | Description |
|------|-------------|
| `--dir <DIR>` | Project directory (default: `.`) |

The plugin at `PATH` shadows the installed version until unlinked. Changes to the local directory are reflected immediately without reinstalling.

See also: [`docs/guides/local-development.md`](docs/guides/local-development.md) for a full local development workflow.

### `aipm unlink`

Remove a development link override and restore the registry version.

```
aipm unlink [OPTIONS] <PACKAGE>
```

| Flag | Description |
|------|-------------|
| `--dir <DIR>` | Project directory (default: `.`) |

See also: [`docs/guides/local-development.md`](docs/guides/local-development.md) for the full link/unlink workflow and how to restore the registry version.

### `aipm list`

Show installed plugins or active development link overrides.

```
aipm list [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `--linked` | Show only active dev link overrides |
| `--global` | Show globally installed plugins |
| `--dir <DIR>` | Project directory (default: `.`) |

See also: [`docs/guides/local-development.md`](docs/guides/local-development.md) for using `aipm list` during the local development workflow.

### `aipm lint`

Check AI plugin configurations and instruction files for quality issues across all detected
source directories. `aipm lint` covers `skill/`, `agent/`, `hook/`, `plugin/`,
`marketplace/`, `instructions/`, and `source/` categories — including checks on
`CLAUDE.md`, `AGENTS.md`, `COPILOT.md`, `GEMINI.md`, `INSTRUCTIONS.md`, and
`*.instructions.md` files anywhere in the project tree.

```
aipm lint [OPTIONS] [DIR]
```

| Flag | Description |
|------|-------------|
| `--source <SRC>` | Limit to a specific source type (`.claude`, `.github`, `.ai`) |
| `--reporter <FMT>` | Output format: `human` (default), `json`, `ci-github`, `ci-azure` |
| `--color <MODE>` | Color output: `auto` (default), `always`, `never` |
| `--max-depth <N>` | Maximum directory traversal depth |

Exits with a non-zero status code when violations are found, making it safe to use in CI pipelines. Use `--reporter ci-github` for GitHub Actions annotations or `--reporter ci-azure` for Azure Pipelines.

See also: [`docs/guides/lint.md`](docs/guides/lint.md) for full CLI usage, output formats, and CI integration; [`docs/guides/configuring-lint.md`](docs/guides/configuring-lint.md) for rule severity overrides, path ignores, and per-rule configuration.

### `aipm make`

Scaffold new plugins directly inside an existing workspace marketplace.

#### `aipm make plugin`

Create a new plugin in the `.ai/` marketplace directory discovered from the current (or specified) project directory.

```
aipm make plugin [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `--name <NAME>` | Plugin name (required in non-interactive mode) |
| `--engine <ENGINE>` | Target engine: `claude` (default), `copilot`, or `both` |
| `--feature <FEATURE>` | Feature type to include — repeatable (required in non-interactive mode) |
| `-y, --yes` | Skip interactive prompts, accept defaults |
| `--dir <DIR>` | Project directory (default: `.`) |

**Available feature types:**

| `--feature` value | Description | Claude | Copilot |
|-------------------|-------------|:------:|:-------:|
| `skill` | Prompt templates (`SKILL.md`) | ✓ | ✓ |
| `agent` | Autonomous sub-agents | ✓ | ✓ |
| `mcp` | MCP server configuration | ✓ | ✓ |
| `hook` | Lifecycle event hooks | ✓ | ✓ |
| `output-style` | Response formatting rules | ✓ | — |
| `lsp` | Language Server integration | — | ✓ |
| `extension` | Copilot extensions | — | ✓ |

The command runs an **idempotent** action pipeline — re-running it on an existing plugin directory is safe and makes no changes.

When run on a TTY without `--yes`, launches a two-phase interactive wizard (name + engine, then feature multi-select).

```bash
# Interactive wizard
aipm make plugin

# Non-interactive examples
aipm make plugin --name code-review --feature skill
aipm make plugin --name dev-tools --engine claude --feature skill --feature agent
aipm make plugin --name ide-helper --engine copilot --feature skill --feature lsp
aipm make plugin --name shared-kit --engine both --feature skill --feature agent --feature mcp
```

See also: [`docs/guides/make-plugin.md`](docs/guides/make-plugin.md) for a full walkthrough, feature matrix, and what gets created.

### `aipm lsp`

Start the `aipm` Language Server Protocol (LSP) server on stdio.

```
aipm lsp
```

The LSP server is launched automatically by the [`vscode-aipm`](docs/guides/vscode-extension.md) extension and is not typically invoked directly. It communicates over stdin/stdout using the Language Server Protocol.

**Capabilities:**

| Capability | Description |
|---|---|
| `textDocument/publishDiagnostics` | Publishes `aipm lint` violations as inline diagnostics on file open and save (300 ms debounce) |
| `textDocument/completion` | Autocompletes rule IDs and severity values inside `[workspace.lints]` in `aipm.toml` |
| `textDocument/hover` | Shows rule name, default severity, and help text when hovering a rule ID in `aipm.toml` |

**Supported file patterns** (simplified/relative patterns; see the VS Code extension for the exact document selector globs): `aipm.toml`, `skills/SKILL.md`, `skills/*/SKILL.md`, `agents/*.md`, `hooks/hooks.json`, `.ai/*/aipm.toml`, `.ai/*/.claude-plugin/plugin.json`, `.ai/.claude-plugin/marketplace.json`, `CLAUDE.md`, `AGENTS.md`, `COPILOT.md`, `GEMINI.md`, `INSTRUCTIONS.md`, `*.instructions.md`.

**Binary resolution:** the server looks up the `aipm` binary from `AIPM_PATH` (environment variable) or the `aipm.path` VS Code setting (default: `"aipm"`).

See also: [`docs/guides/vscode-extension.md`](docs/guides/vscode-extension.md) for installation, configuration, and development setup.

---

## `aipm pack` — Author Commands

Pack commands are available under the `aipm pack` subcommand.

### `aipm pack init`

Create a new AI plugin package with manifest and conventional directory layout.

```
aipm pack init [OPTIONS] [DIR]
```

| Flag | Description |
|------|-------------|
| `-y, --yes` | Skip interactive prompts |
| `--name <NAME>` | Package name (defaults to directory name) |
| `--type <TYPE>` | Plugin type: `skill`, `agent`, `mcp`, `hook`, `lsp`, `composite` |

Generates an `aipm.toml` manifest and type-appropriate directory structure.

See also: [`docs/guides/creating-a-plugin.md`](docs/guides/creating-a-plugin.md) for a full authoring walkthrough.

> **ℹ️ Planned commands:** `aipm publish`, `aipm yank`, and `aipm login` are defined in the roadmap but not yet implemented. Track at [#6](https://github.com/TheLarkInn/aipm/issues/6) and [#8](https://github.com/TheLarkInn/aipm/issues/8).

---

## `libaipm` — Core Library

Shared library powering the CLI. All logic lives here; the binary is a thin wrapper.

### Modules

| Module | Purpose |
|--------|---------|
| `manifest` | Parse, validate, and load `aipm.toml` manifests |
| `manifest::types` | Schema types: `Manifest`, `Package`, `Workspace`, `Components`, `Environment`, `DependencySpec` |
| `manifest::validate` | Name format, semver, dependency version, component path validation |
| `init` | Plugin package scaffolding (`aipm pack init`) |
| `workspace_init` | Workspace + `.ai/` marketplace scaffolding (`aipm init`) |
| `workspace_init::adaptors` | Tool-specific config writers (Claude Code; Copilot/Cursor planned) |
| `workspace` | Workspace root discovery and `[workspace].members` glob expansion |
| `migrate` | Tool config migration with recursive discovery, dry-run, and all artifact types |
| `lint` | Quality linting for AI plugin configurations, diagnostics, and reporting |
| `discovery` | Gitignore-aware recursive discovery of AI tool source directories (shared by `lint` and `migrate`) |
| `installer` | Package installation pipeline and manifest editing |
| `linker` | Local dev link overrides (`aipm link` / `unlink`) |
| `lockfile` | Deterministic `aipm.lock` creation and drift detection |
| `resolver` | Semver dependency resolution |
| `store` | Content-addressable global package store |
| `registry` | Registry client interface |
| `spec` | Plugin spec parser (`name@version`, `github:`, `git:`, `local:`, `marketplace:` formats) |
| `acquirer` | Local copy and git clone acquisition with source redirect support |
| `cache` | Download cache with 5 policies and per-entry TTL (`~/.aipm/cache/`) |
| `installed` | Global plugin registry with engine scoping and name conflict detection |
| `marketplace` | TOML marketplace manifest parsing (relative, git, and unsupported source types) |
| `engine` | Two-tier engine validation (`aipm.toml` `engines` field + marker file fallback) |
| `platform` | Runtime OS and architecture detection and compatibility checking |
| `path_security` | `ValidatedPath` — rejects traversal, URL-encoded, and absolute paths |
| `locked_file` | OS-level exclusive file locking for cache and registry writes |
| `security` | Configurable source allowlist with CI enforcement |
| `logging` | Layered `tracing` subscriber initialization (stderr verbosity + rotating file log) |
| `generate` | Centralised JSON generation for `marketplace.json`, `plugin.json`, and `settings.json` (unified read-modify-write helpers used by `workspace_init` and `migrate`) |
| `make` | Idempotent plugin scaffolding pipeline (`aipm make plugin`) — orchestrates `generate`, `manifest`, and `init` into a 9-step action sequence |
| `wizard` | Shared wizard types and theming for interactive CLI prompts; gated behind the `wizard` feature flag (required by `aipm`) |
| `frontmatter` | YAML front-matter parsing for plugin files |
| `fs` | Trait-based filesystem abstraction (`Real` + test mocking) |
| `version` | Crate version constant |

### Manifest Format (`aipm.toml`)

```toml
[package]
name = "@company/ci-tools"
version = "1.2.3"
description = "CI automation skills"
type = "composite"
files = ["skills/", "hooks/", "README.md"]
engines = ["claude", "copilot"]   # optional — omit to support all engines

[package.source]                  # optional — marketplace stub redirect
type = "git"
url = "https://github.com/org/repo"
path = "plugins/ci-tools"

[dependencies]
shared-lint = "^1.0"
core-hooks = { workspace = "*" }
heavy-analyzer = { version = "^1.0", optional = true }
# Source dependency formats:
ui-toolkit   = { github = "org/repo", path = "plugins/ui", ref = "main" }
local-helper = { path = "../local-helper" }
my-market-dep = { marketplace = "my-registry", name = "dep-name", ref = "v2" }

[features]
default = ["basic"]
basic = []
deep = ["dep:heavy-analyzer"]

[components]
skills = ["skills/lint/SKILL.md"]
agents = ["agents/reviewer.md"]
hooks = ["hooks/hooks.json"]
mcp_servers = [".mcp.json"]
lsp_servers = [".lsp.json"]
scripts = ["scripts/format-code.sh"]
output_styles = ["styles/custom.css"]
settings = ["settings.json"]

[environment]
requires = ["git", "docker"]
aipm = ">=0.5.0"
platforms = ["linux-x64", "macos-arm64", "windows-x64"]
strict = true

[environment.runtime]
node = ">=18.0.0"

[install]
allowed_build_scripts = ["native-tool"]
```

**Plugin types:** `skill` · `agent` · `mcp` · `hook` · `lsp` · `composite`

### Workspace Root Manifest

```toml
[workspace]
members = ["plugins/*"]
plugins_dir = "plugins"

[workspace.dependencies]
common-skill = "^2.0"

[workspace.lints]
# Promote to error — fail CI if descriptions are missing
"skill/missing-description" = "error"
# Suppress entirely
"skill/oversized" = "allow"
# Per-rule ignore paths (lint ignore matching uses full file paths, so use `**/` to match anywhere)
"source/misplaced-features" = { level = "warn", ignore = ["**/.claude/skills/legacy-*/**"] }
# instructions/oversized — relax size limits for large monorepo instruction files
# (also supports `resolve-imports = true` to count imported content toward the limit)
"instructions/oversized" = { lines = 200, characters = 20000 }

[workspace.lints.ignore]
paths = ["**/vendor/**", "**/third-party/**"]

[overrides]
"vulnerable-lib" = "^2.0.0"

[catalog]
lint-skill = "^1.5.0"

[catalogs.stable]
framework = "^1.0.0"

[catalogs.next]
framework = "^2.0.0-beta.1"
```

### Editor Schema Support for `aipm.toml`

A JSON Schema for `aipm.toml` provides autocomplete and inline validation for `[workspace.lints]` in any editor that understands TOML or JSON Schema:

**Schema URL:**
```
https://raw.githubusercontent.com/TheLarkInn/aipm/main/schemas/aipm.toml.schema.json
```

- **VS Code** — install the [`vscode-aipm`](docs/guides/vscode-extension.md) extension; it registers the schema automatically via `tomlValidation`. Requires [Even Better TOML](https://marketplace.visualstudio.com/items?itemName=tamasfe.even-better-toml) or Taplo for validation and autocomplete.
- **Neovim / Helix / Emacs (Taplo)** — add a `.taplo.toml` to your project root:
  ```toml
  [[rule]]
  include = ["**/aipm.toml"]
  schema = "https://raw.githubusercontent.com/TheLarkInn/aipm/main/schemas/aipm.toml.schema.json"
  ```
- **SchemaStore** — a catalog entry is prepared at `schemas/schemastore-submission/catalog-entry.json` for submission to [SchemaStore.org](https://www.schemastore.org/). Once merged, Taplo and Tombi users get zero-install coverage — no `.taplo.toml` needed.

See [Editor schema support](docs/guides/configuring-lint.md#editor-schema-support) in the lint configuration guide for full details.

---

## Project Structure

```
crates/
  aipm/         CLI binary (init, install, update, uninstall, link, unlink, list, lint, migrate, make, pack, lsp)
  libaipm/      Core library (manifest, validation, migration, scaffolding, lint, install, link, resolve)
vscode-aipm/    VS Code extension (lint diagnostics, completions, hover for aipm.toml)
specs/          Technical design documents
tests/features/ Cucumber BDD feature files (31 files, 300+ scenarios)
research/       Competitive analysis and design research
```

---

## Roadmap

The following features are defined as BDD scenarios and tracked as open issues. Status markers reflect the current implementation:

- ✅ **Working** — available end-to-end today
- 🔨 **Partial** — library implemented and tested, CLI integration incomplete or spec scenarios not yet fully wired
- 📋 **Planned** — spec and BDD scenarios written, implementation not started

### Dependencies

- 🔨 **Resolution** — library complete (69 resolver tests, backtracking, overrides); not yet fully exposed through `aipm install` ([#1](https://github.com/TheLarkInn/aipm/issues/1))
- 🔨 **Lockfile** — library complete (31 tests); `--locked` CI mode works in `aipm install`; full drift-detection UI pending ([#2](https://github.com/TheLarkInn/aipm/issues/2))
- 🔨 **Features** — library complete (default features, additive unification in resolver); not yet wired to CLI ([#3](https://github.com/TheLarkInn/aipm/issues/3))
- 📋 **Patching** — `aipm patch` workflow for editing transitive deps without forking ([#4](https://github.com/TheLarkInn/aipm/issues/4))

### Registry

- 🔨 **Install** — local, git, github, and marketplace sources work end-to-end; bare registry-name resolution is blocked pending registry wiring ([#5](https://github.com/TheLarkInn/aipm/issues/5))
- 📋 **Publish** — `aipm pack` / `aipm publish` with `.aipm` archives, dry-run, file allowlist, size limits ([#6](https://github.com/TheLarkInn/aipm/issues/6))
- 🔨 **Security** — source allowlist and path traversal protection ship today (see [`docs/guides/source-security.md`](docs/guides/source-security.md)); `aipm audit` and advisory checks are not yet implemented ([#7](https://github.com/TheLarkInn/aipm/issues/7))
- 📋 **Yank** — `aipm yank` / un-yank, deprecation messages ([#8](https://github.com/TheLarkInn/aipm/issues/8))
- ✅ **Link** — `aipm link` / `unlink` for local dev overrides are fully functional today ([#9](https://github.com/TheLarkInn/aipm/issues/9))
- 🔨 **Local + Registry Coexistence** — directory links, gitignore management work; registry download integration pending ([#10](https://github.com/TheLarkInn/aipm/issues/10))

### Monorepo

- 📋 **Orchestration** — workspace protocol, catalogs, filtering by name/path/changed/dependents, Rush/Turborepo integration ([#11](https://github.com/TheLarkInn/aipm/issues/11))

### Environment

- 📋 **Dependencies** — declare required tools, env vars, platforms, MCP runtimes; `aipm doctor` ([#12](https://github.com/TheLarkInn/aipm/issues/12))
- 📋 **Host Versioning** — `[environment.hosts]` section for Claude/Copilot/Cursor version constraints ([#54](https://github.com/TheLarkInn/aipm/issues/54))

### Quality & Portability

- 🔨 **Guardrails** — `aipm lint` with 18 rules and 4 reporters ships today (see [`docs/guides/lint.md`](docs/guides/lint.md)); auto-fix and quality scoring on publish are not yet implemented ([#13](https://github.com/TheLarkInn/aipm/issues/13))
- 🔨 **Compositional Reuse** — spec, acquirer, and marketplace modules ship; full publish/consume workflow for standalone primitives pending ([#14](https://github.com/TheLarkInn/aipm/issues/14))
- 🔨 **Cross-Stack** — Claude Code adaptor ships today; Copilot CLI, Cursor, and OpenCode adaptors planned ([#15](https://github.com/TheLarkInn/aipm/issues/15))

---

## Why not `apm`?

[microsoft/apm](https://github.com/microsoft/apm) (`apm-cli` on PyPI) validates that AI plugin package management is a real problem. However, its architecture falls short for production use across several dimensions:

1. **Runtime dependency.** `apm` requires Python 3.9+ and 13 pip packages. This creates friction for .NET, Rust, Go, and Java teams, and adds version-management overhead. AIPM is a self-contained Rust binary — drop it in any repo regardless of tech stack.

2. **YAML manifest.** `apm.yml` uses YAML, which has the [Norway problem](https://hitchdev.com/strictyaml/why/implicit-typing-removed/) (`3.10` → `3.1`), implicit type coercion (`NO` → `false`), indentation sensitivity, and active security CVEs in parsers. AIPM uses TOML — no coercion, no indentation traps, safe for AI-generated manifests.

3. **No registry.** Packages are git repos. There is no publish lifecycle, no immutable versions, no scoped namespaces, no centralized search, and no way to yank a broken release without deleting git tags. AIPM has a dedicated API registry with publish, yank, scoped packages, and multi-registry routing.

4. **No semver resolution.** `apm` pins by git ref — no `^1.0` ranges, no backtracking, no version unification. Two packages depending on different versions of the same dep each get a full clone. AIPM uses Cargo-model semver with caret/tilde ranges, backtracking, and major-version coexistence.

5. **No integrity verification.** The lockfile records commit SHAs but no file-level hashes. Force-pushes or host compromises silently change what a "version" resolves to. AIPM stores SHA-512 checksums per file and verifies on install.

6. **Full git clones per project.** Every project downloads full copies of every dependency — no deduplication, no global cache. AIPM uses a content-addressable global store (pnpm model) with hard links for 70%+ disk savings.

7. **No dependency isolation.** Everything in `apm_modules/` is accessible to everything else. Phantom dependencies go undetected. AIPM enforces strict isolation — only declared dependencies are accessible.

8. **Minimal security.** No lifecycle script blocking (any package runs arbitrary code), no `audit` command, no principle-of-least-privilege binary split. AIPM blocks scripts by default, separates consumer and author commands, and plans advisory-based auditing.

9. **No transfer format.** Packages are raw git repos — no archive format, no file allowlist, no secrets exclusion. AIPM uses deterministic `.aipm` archives (gzip tar) with sorted files, zeroed timestamps, and default secrets exclusion.

10. **No offline support.** Every install requires network access. AIPM supports `--offline` installation from the global cache.

11. **No CI lockfile mode.** `apm install` uses the lockfile if present, but there is no `--locked` mode that fails on drift. AIPM follows the Cargo model: `install` never upgrades, `update` explicitly resolves latest, `--locked` fails on any mismatch.

12. **No workspace features.** No workspace protocol, no catalogs, no dependency inheritance, no filtering. AIPM supports all of these for monorepo-scale plugin management.

13. **Compilation coupling.** `apm` tightly couples package management with `AGENTS.md` / `CLAUDE.md` generation. AIPM decouples packaging from host discovery — AI tools discover plugins naturally via directory scanning.

In short: `apm` is a useful prototype that proves the problem space. AIPM is designed to be the production-grade infrastructure for it.

---

## Contributing

Contributions and suggestions are welcome! Please open an issue or pull request on [GitHub](https://github.com/thelarkinn/aipm).

## License

This project is licensed under the [MIT License](LICENSE).
