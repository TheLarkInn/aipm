# Creating a Plugin Package

`aipm-pack init` scaffolds a new AI plugin package with an `aipm.toml` manifest and a conventional directory layout. Use it when you want to create a plugin to share with your team or publish.

## Quick start

```bash
mkdir my-skill && cd my-skill
aipm-pack init
```

When run on a TTY without `--yes`, an interactive wizard prompts for a name and plugin type.

## Plugin types

| Type | Directory layout | Use case |
|------|-----------------|---------|
| `skill` | `skills/` | Standalone skill (e.g., a `SKILL.md`) |
| `agent` | `agents/` | Subagent definitions |
| `mcp` | `mcp/` | MCP server configuration |
| `hook` | `hooks/` | Tool lifecycle hooks |
| `lsp` | _(none)_ | LSP server configuration |
| `composite` | `skills/`, `agents/`, `hooks/` | Bundle of multiple artifact types |

The default type is `composite`.

## Non-interactive usage

Pass `--yes` to accept defaults, or supply flags directly:

```bash
# Use directory name as package name, default type (composite)
aipm-pack init --yes

# Specify name and type
aipm-pack init --name my-linter --type skill

# Initialize in a specific directory
aipm-pack init --name @org/my-agent --type agent ./plugins/my-agent
```

## What gets created

For a `skill` package named `my-linter`:

```
my-linter/
  aipm.toml          # manifest
  skills/
    .gitkeep
```

For a `composite` package named `my-toolkit`:

```
my-toolkit/
  aipm.toml
  skills/
    .gitkeep
  agents/
    .gitkeep
  hooks/
    .gitkeep
```

The generated `aipm.toml`:

```toml
[package]
name = "my-linter"
version = "0.1.0"
type = "skill"
```

## Package name rules

Package names must be:

- Lowercase alphanumeric characters and hyphens only
- Optionally scoped: `@org/name`
- No leading hyphens, spaces, or uppercase letters

Valid examples: `my-plugin`, `ci-tools`, `@acme/code-review`

## Next steps after scaffolding

1. **Add your content** — place `SKILL.md` files under `skills/<name>/`, agents under `agents/`, etc.
2. **Edit the manifest** — add `description`, `engines`, `files`, and `[dependencies]` as needed:

    ```toml
    [package]
    name = "my-linter"
    version = "0.1.0"
    type = "skill"
    description = "Runs project-specific lint checks"
    engines = ["claude", "copilot"]
    files = ["skills/"]
    ```

3. **Lint your plugin** — run `aipm lint` to check for quality issues before publishing.
4. **Link for local testing** — in a consuming project, run `aipm link ../my-linter` to test without publishing.

## Flag reference

```
aipm-pack init [OPTIONS] [DIR]
```

| Flag | Description |
|------|-------------|
| `-y, --yes` | Skip interactive prompts, use defaults |
| `--name <NAME>` | Package name (defaults to directory name) |
| `--type <TYPE>` | Plugin type: `skill`, `agent`, `mcp`, `hook`, `lsp`, `composite` |

See also: [`aipm-pack init`](../../README.md#aipm-pack-init), [Manifest format](../../README.md#manifest-format-aipmtoml), [`docs/guides/local-development.md`](./local-development.md).
