# Migrating Existing Configurations

`aipm migrate` converts existing AI tool configurations (`.claude/`, `.github/`,
etc.) into structured marketplace plugins inside `.ai/`. It detects all artifact
types automatically, preserves file contents, and registers each plugin in the
marketplace manifest.

## When to Use Migrate

Use `aipm migrate` when you have an existing project with AI tool configurations
spread across tool-specific directories and you want to consolidate them into a
managed `.ai/` marketplace that `aipm install`, `aipm lint`, and other commands
can work with.

## Basic Usage

```bash
# Migrate from the current directory (recursive discovery)
aipm migrate

# Preview what would be migrated without making changes
aipm migrate --dry-run

# Migrate a specific project directory
aipm migrate ./my-project

# Migrate only a single source directory
aipm migrate --source .claude
```

## CLI Flags

| Flag | Description |
|------|-------------|
| `--dry-run` | Preview the migration plan without writing any files |
| `--destructive` | Remove migrated source files after a successful migration |
| `--source <SRC>` | Scan a single source folder (e.g., `.claude`). Omit to discover recursively |
| `--max-depth <N>` | Maximum depth for recursive source discovery |
| `--manifest` | Generate `aipm.toml` plugin manifests for each migrated plugin |

## Detected Artifact Types

`aipm migrate` supports two source ecosystems, each with its own set of detectors.

### Claude Code (`.claude/`)

| Artifact | Source Location | Migrated As |
|----------|----------------|-------------|
| Skills | `.claude/skills/<name>/` | `skill` plugin |
| Agents | `.claude/agents/<name>.md` | `agent` plugin |
| MCP servers | `.mcp.json` | `mcp` plugin |
| Hooks | `hooks.json` or embedded in `.claude/settings.json` | `hook` plugin |
| Commands | `.claude/commands/<name>.md` | `skill` plugin (commands are a skill subtype) |
| Output styles | `.claude/output-styles/<name>.md` | `composite` plugin |

### Copilot CLI (`.github/`)

| Artifact | Source Location | Migrated As |
|----------|----------------|-------------|
| Skills | `.github/skills/<name>/` or `.github/copilot/<name>/` | `skill` plugin |
| Agents | `.github/agents/<name>.md` or `<name>.agent.md` | `agent` plugin |
| MCP servers | `.copilot/mcp-config.json` | `mcp` plugin |
| Hooks | `.github/hooks.json` or `.github/hooks/hooks.json` | `hook` plugin |
| GitHub extensions | `.github/extensions/<name>/` | `composite` plugin |
| LSP servers | `.github/lsp.json` or `lsp.json` | `lsp` plugin |

> **Note**: The Copilot CLI stores skills in `.github/copilot/` by default. The
> legacy `.github/skills/` path is also supported. Both directories are scanned
> automatically; each subdirectory containing a `SKILL.md` file is detected as a
> skill artifact.

Files that are not claimed by any detector (e.g., scripts referenced from a
skill) are also migrated and tracked.

## Dry-Run Mode

Use `--dry-run` to preview the full migration plan before committing to it:

```bash
aipm migrate --dry-run
```

This writes a detailed Markdown report to `aipm-migrate-dryrun-report.md` in the
project root listing:

- Every plugin that would be created
- Every file that would be moved
- Any naming conflicts and how they would be resolved
- Scripts and external references detected

No files are created, moved, or deleted during a dry run.

## Destructive Mode

By default, `aipm migrate` **copies** artifacts to `.ai/` and leaves the
originals in place. Pass `--destructive` to remove the source files after a
successful migration:

```bash
aipm migrate --destructive
```

On an interactive terminal, if `--destructive` is not passed, the command prompts
whether to clean up the originals after the migration succeeds.

> **Caution**: Destructive cleanup cannot be undone. Run `--dry-run` first to
> verify the migration plan.

## Recursive Discovery

By default, `aipm migrate` recursively searches subdirectories for supported
source folders (`.claude/`, `.github/`, etc.), respecting `.gitignore` rules.
This handles monorepos where multiple packages each have their own configurations.

Limit the search depth with `--max-depth`:

```bash
aipm migrate --max-depth 2
```

To migrate a single known directory instead of searching recursively:

```bash
aipm migrate --source .claude
```

## Generating Manifests

Pass `--manifest` to generate an `aipm.toml` for each migrated plugin:

```bash
aipm migrate --manifest
```

Each generated manifest includes the plugin `name`, `version`, `type`, and
`description` inferred from the artifact's frontmatter (when available). You can
edit these manifests afterward to add `engines`, `environment`, or `dependencies`.

## Output Structure

After migration, the `.ai/` directory follows the standard marketplace layout:

```
.ai/
  .claude-plugin/
    marketplace.json          # auto-generated plugin registry
  <marketplace-name>/
    <plugin-name>/            # one directory per migrated plugin
      aipm.toml               # only if --manifest was passed
      SKILL.md / agent.md / hooks.json / .mcp.json / ...
```

The `marketplace.json` is automatically created or updated to register every
successfully migrated plugin.

## Example Walkthrough

### Before migration

```
.claude/
  skills/
    deploy/
      SKILL.md
      scripts/deploy.sh
  agents/
    reviewer.md
  settings.json               # contains hooks
.mcp.json
```

### Run migration

```bash
aipm migrate --dry-run        # review the plan
aipm migrate --manifest       # apply it with manifest generation
```

### After migration

```
.ai/
  .claude-plugin/
    marketplace.json
  local-repo-plugins/
    deploy/
      aipm.toml
      SKILL.md
      scripts/deploy.sh
    reviewer/
      aipm.toml
      reviewer.md
    hooks/
      aipm.toml
      hooks.json
    mcp-server/
      aipm.toml
      .mcp.json
.claude/                      # originals still present (remove with --destructive)
  skills/deploy/...
```

## Naming Conflicts

When two artifacts would produce the same plugin name — whether from separate source
directories or when an artifact's name matches an existing `.ai/` directory — aipm
automatically renames one of them with a numeric suffix and reports the rename:

```
Warning: renamed 'deploy' → 'deploy-renamed-1' (plugin 'deploy' already exists in .ai/)
```

This also applies when re-running `aipm migrate`: any artifact whose name collides with
an already-migrated `.ai/` directory is renamed rather than skipped. See
[issue #314](https://github.com/TheLarkInn/aipm/issues/314) for the planned idempotent
behavior.

Always review the output (or use `--dry-run` first) to verify that rename
decisions are acceptable before committing.

## Skipped Artifacts

Artifacts that cannot be safely migrated are skipped with an explanation:

```
Skipped 'my/../tool': unsafe artifact name 'my/../tool': must be a single path segment without separators or '..'
```

Common skip reasons:

| Reason | Explanation |
|--------|-------------|
| Source directory is empty | No files found in the artifact directory |
| External reference only | The file is referenced by another plugin; it will be migrated with that plugin |
| Already migrated | A plugin with the same name already exists in `.ai/` |
| Non-regular file | The path is a symlink to a directory, a device file, or another special file; only regular files are copied. A warning is emitted at `-v` verbosity |

## External References

If a script referenced inside a skill lives outside the source directory (e.g.,
at the project root), `aipm migrate` reports it as an external reference instead
of moving it:

```
External reference detected: scripts/common.sh (referenced by deploy)
```

These files are not moved automatically. Resolve them manually after migration by
copying the file into the plugin directory and updating the reference in
`SKILL.md`.
