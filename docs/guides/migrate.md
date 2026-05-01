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
| `--no-summary` | Suppress the scan summary line printed to stderr by default |

## Detected Artifact Types

`aipm migrate` supports two source ecosystems, each with its own set of detectors.

### Claude Code (`.claude/`)

| Artifact | Source Location | Migrated As |
|----------|----------------|-------------|
| Skills | `.claude/skills/<name>/` | `skill` plugin |
| Agents | `.claude/agents/<name>.md` | `agent` plugin |
| MCP servers | `.mcp.json` | `mcp` plugin |
| Hooks | `.claude/settings.json` hooks block | `hook` plugin |
| Commands | `.claude/commands/<name>.md` | `skill` plugin (commands are a skill subtype) |
| Output styles | `.claude/output-styles/<name>.md` | `composite` plugin |

### Copilot CLI (`.github/`)

| Artifact | Source Location | Migrated As |
|----------|----------------|-------------|
| Skills | `.github/skills/<name>/`, `.github/copilot/<name>/`, or `.github/copilot/skills/<name>/` | `skill` plugin |
| Agents | `.github/agents/<name>.md` or `<name>.agent.md` | `agent` plugin |
| MCP servers | `.copilot/mcp-config.json` | `mcp` plugin |
| Hooks | `.github/hooks.json` or `.github/hooks/hooks.json` | `hook` plugin |
| GitHub extensions | `.github/extensions/<name>/` | `composite` plugin |
| LSP servers | `.github/lsp.json` or `lsp.json` | `lsp` plugin |

> **Note**: The Copilot CLI stores skills in `.github/copilot/` by default.
> The legacy `.github/skills/` path is also supported, as is the nested
> `.github/copilot/skills/<name>/SKILL.md` layout (issue [#725]). All three
> layouts are scanned automatically — the unified discovery pipeline is always
> active and requires no environment-variable opt-in.
>
> [#725]: https://github.com/TheLarkInn/aipm/issues/725

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

## Scan Summary

After every run, `aipm migrate` prints a single line to **stderr** describing what the
discovery walker found:

```
Scanned 4 directories in [.github, .claude]; matched 3 skills, 1 instruction
```

The summary is shown by default so that "scanned but nothing matched" outcomes are never
silent. It is suppressed automatically when `--log-format=json` is in effect (keeping
stdout machine-parseable), and can be suppressed manually with `--no-summary`:

```bash
aipm migrate --no-summary
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

After migration, the `.ai/` directory follows the standard marketplace layout.
Each plugin directory contains a `.claude-plugin/plugin.json` manifest (always
generated) and an `aipm.toml` manifest (only when `--manifest` is passed). The
artifact files are placed in type-specific subdirectories:

```
.ai/
  .claude-plugin/
    marketplace.json            # auto-generated plugin registry
  <marketplace-name>/
    <plugin-name>/              # one directory per migrated plugin
      .claude-plugin/
        plugin.json             # always generated
      aipm.toml                 # only if --manifest was passed
      skills/
        <name>/
          SKILL.md              # skill artifacts
      agents/
        <name>.md               # agent artifacts
      hooks/
        hooks.json              # hook artifacts
      .mcp.json                 # MCP server artifacts
      lsp.json                  # LSP server artifacts
      <name>.md                 # output style artifacts
      scripts/
        ...                     # referenced scripts (copied to plugin root)
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
      .claude-plugin/
        plugin.json
      aipm.toml
      skills/
        deploy/
          SKILL.md
      scripts/
        deploy.sh               # referenced scripts copied to plugin root
    reviewer/
      .claude-plugin/
        plugin.json
      aipm.toml
      agents/
        reviewer.md
    hooks/
      .claude-plugin/
        plugin.json
      aipm.toml
      hooks/
        hooks.json
    mcp-server/
      .claude-plugin/
        plugin.json
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

## Error Reference

| Error | Cause | Resolution |
|-------|-------|------------|
| `marketplace directory does not exist at <path>` | `.ai/.claude-plugin/` is missing | Run `aipm init --marketplace` first |
| `source directory does not exist: <path>` | The path passed to `--source` does not exist | Verify the path and re-run |
| `unsupported source type '<src>'` | `--source` value is not `.claude` or `.github` | Use a supported source type |
| `failed to parse marketplace.json at <path>: <detail>` | `marketplace.json` contains invalid JSON — `<detail>` includes the line and column | Fix the JSON manually or delete the file and re-run `aipm init --marketplace` |
| `failed to parse SKILL.md frontmatter in <path>: <reason>` | A `SKILL.md` has malformed YAML frontmatter | Correct the frontmatter and re-run |
| `failed to parse <path>: <reason>` | A JSON configuration file (e.g. `hooks.json`) is malformed | Fix the JSON and re-run |

> **Tip**: Run `aipm migrate --dry-run` after fixing errors to confirm the plan before applying changes.

---

See also: [`aipm migrate`](../../README.md#aipm-migrate), [`docs/guides/migrating-existing-configs.md`](./migrating-existing-configs.md), [`docs/guides/creating-a-plugin.md`](./creating-a-plugin.md), [`docs/guides/lint.md`](./lint.md).
