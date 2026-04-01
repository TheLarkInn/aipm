# AIPM — AI Plugin Manager

[![CI](https://github.com/TheLarkInn/aipm/actions/workflows/ci.yml/badge.svg)](https://github.com/TheLarkInn/aipm/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/TheLarkInn/aipm/graph/badge.svg)](https://codecov.io/gh/TheLarkInn/aipm)

A production-grade package manager for AI plugin primitives (skills, agents, MCP servers, hooks). Think npm/Cargo, but purpose-built for the AI plugin ecosystem.

AIPM ships as **two Rust binaries** with **zero runtime dependencies**:

| Binary | Role | Commands |
|--------|------|----------|
| **`aipm`** | Consumer CLI | `init`, `migrate` |
| **`aipm-pack`** | Author CLI | `init` |

Both work across .NET, Python, Node.js, and Rust projects with no runtime dependency.

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

### Build from Source

```bash
cargo build --workspace          # build all crates
cargo test --workspace           # run all tests
```

---

## `aipm` — Consumer CLI

Initializes workspaces and migrates existing AI tool configurations into portable marketplace plugins.

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
- `.ai/<marketplace-name>/` — local marketplace directory
- `.ai/<marketplace-name>/starter-aipm-plugin/` — starter skill plugin (unless `--no-starter`)
- `.ai/.claude/settings.json` — Claude Code marketplace registration
- `.ai/.copilot/` — Copilot agent settings (if detected)
- `aipm.toml` — workspace manifest (with `--workspace`)

### `aipm migrate`

Migrate existing `.claude/` configurations into marketplace plugins. Detects skills, agents, MCP servers, hooks, commands, and output styles.

```
aipm migrate [OPTIONS] [DIR]
```

| Flag | Description |
|------|-------------|
| `--dry-run` | Preview migration without writing files (generates report) |
| `--source <SRC>` | Source folder to scan (e.g., `.claude`). Omit to discover recursively |
| `--max-depth <N>` | Maximum depth for recursive discovery |
| `--manifest` | Generate `aipm.toml` manifests for migrated plugins |

**Detected artifact types:** skills (`SKILL.md`), agents (`*.md` in `agents/`), MCP servers (`.mcp.json`), hooks (`hooks.json`), commands (`commands/*.md`), output styles.

---

## `aipm-pack` — Author CLI

Scaffolds new plugin packages for publishing.

### `aipm-pack init`

Create a new AI plugin package with manifest and conventional directory layout.

```
aipm-pack init [OPTIONS] [DIR]
```

| Flag | Description |
|------|-------------|
| `-y, --yes` | Skip interactive prompts |
| `--name <NAME>` | Package name (defaults to directory name) |
| `--type <TYPE>` | Plugin type: `skill`, `agent`, `mcp`, `hook`, `lsp`, `composite` |

Generates an `aipm.toml` manifest and type-appropriate directory structure.

---

## `libaipm` — Core Library

Shared library powering both CLIs. All logic lives here; the binaries are thin wrappers.

### Modules

| Module | Purpose |
|--------|---------|
| `manifest` | Parse, validate, and load `aipm.toml` manifests |
| `manifest::types` | Schema types: `Manifest`, `Package`, `Workspace`, `Components`, `Environment`, `DependencySpec` |
| `manifest::validate` | Name format, semver, dependency version, component path validation |
| `init` | Plugin package scaffolding (`aipm-pack init`) |
| `workspace_init` | Workspace + `.ai/` marketplace scaffolding (`aipm init`) |
| `workspace_init::adaptors` | Tool-specific config writers (Claude Code, Copilot, Cursor) |
| `migrate` | Tool config migration with recursive discovery, dry-run, and all artifact types |
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

[dependencies]
shared-lint = "^1.0"
core-hooks = { workspace = "^" }
heavy-analyzer = { version = "^1.0", optional = true }

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

[overrides]
"vulnerable-lib" = "^2.0.0"

[catalog]
lint-skill = "^1.5.0"

[catalogs.stable]
framework = "^1.0.0"

[catalogs.next]
framework = "^2.0.0-beta.1"
```

---

## Project Structure

```
crates/
  aipm/         Consumer CLI binary (init, migrate)
  aipm-pack/    Author CLI binary (init)
  libaipm/      Core library (manifest, validation, migration, scaffolding)
specs/          Technical design documents
tests/features/ Cucumber BDD feature files (220+ scenarios)
research/       Competitive analysis and design research
```

---

## Roadmap

The following features are defined as BDD scenarios and tracked as open issues. They represent the full planned scope beyond what is currently implemented.

### Dependencies

- **Resolution** — semver solver with backtracking, version unification, conflict reporting, overrides ([#1](https://github.com/TheLarkInn/aipm/issues/1))
- **Lockfile** — deterministic `aipm.lock` creation, drift detection, `--locked` CI mode ([#2](https://github.com/TheLarkInn/aipm/issues/2))
- **Features** — default features, opt-out, additive feature unification across the graph ([#3](https://github.com/TheLarkInn/aipm/issues/3))
- **Patching** — `aipm patch` workflow for editing transitive deps without forking ([#4](https://github.com/TheLarkInn/aipm/issues/4))

### Registry

- **Install** — `aipm install` with semver resolution, content-addressable store, integrity verification, strict isolation ([#5](https://github.com/TheLarkInn/aipm/issues/5))
- **Publish** — `aipm-pack pack` / `publish` with `.aipm` archives, dry-run, file allowlist, size limits ([#6](https://github.com/TheLarkInn/aipm/issues/6))
- **Security** — checksums, tamper detection, `aipm audit`, auth, scoped org permissions ([#7](https://github.com/TheLarkInn/aipm/issues/7))
- **Yank** — `aipm-pack yank` / un-yank, deprecation messages ([#8](https://github.com/TheLarkInn/aipm/issues/8))
- **Link** — `aipm link` / `unlink` for local dev overrides ([#9](https://github.com/TheLarkInn/aipm/issues/9))
- **Local + Registry Coexistence** — directory links, gitignore management, vendoring ([#10](https://github.com/TheLarkInn/aipm/issues/10))

### Monorepo

- **Orchestration** — workspace protocol, catalogs, filtering by name/path/changed/dependents, Rush/Turborepo integration ([#11](https://github.com/TheLarkInn/aipm/issues/11))

### Environment

- **Dependencies** — declare required tools, env vars, platforms, MCP runtimes; `aipm doctor` ([#12](https://github.com/TheLarkInn/aipm/issues/12))
- **Host Versioning** — `[environment.hosts]` section for Claude/Copilot/Cursor version constraints ([#54](https://github.com/TheLarkInn/aipm/issues/54))

### Quality & Portability

- **Guardrails** — `aipm lint`, auto-fix, quality scoring on publish ([#13](https://github.com/TheLarkInn/aipm/issues/13))
- **Compositional Reuse** — publish/consume standalone skills, agents, MCP configs, hooks as packages ([#14](https://github.com/TheLarkInn/aipm/issues/14))
- **Cross-Stack** — verified portability across Node.js, .NET, Python, Rust, CMake; offline resolution ([#15](https://github.com/TheLarkInn/aipm/issues/15))

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

8. **Minimal security.** No lifecycle script blocking (any package runs arbitrary code), no `audit` command, no principle-of-least-privilege binary split. AIPM blocks scripts by default, ships separate consumer/author binaries, and plans advisory-based auditing.

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
