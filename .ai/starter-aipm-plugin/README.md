# Starter AIPM Plugin

The default plugin created by `aipm init`. It gives AI agents the tools they need to
scaffold new plugins, inspect the local marketplace, and track tool usage — all within
the project's `.ai/` directory.

## What's included

| Component | Name | Purpose |
|-----------|------|---------|
| Skill | `scaffold-plugin` | Scaffold a new AI plugin in the `.ai/` marketplace |
| Agent | `marketplace-scanner` | Read-only analysis of all installed plugins |
| Hook | `PostToolUse` | Append an entry to `.ai/.tool-usage.log` on every tool call |
| Script | `scaffold-plugin.ts` | Node.js scaffolding script called by the skill |

## Skills

### `scaffold-plugin`

Instructs an AI agent to create a new plugin under `.ai/` by running the bundled
`scaffold-plugin.ts` script.

**When to invoke:** ask your AI assistant to "create a new plugin" or "scaffold a
skill called `my-analyzer`". The agent will:

1. Ask for a plugin name if not provided.
2. Run `node --experimental-strip-types .ai/starter-aipm-plugin/scripts/scaffold-plugin.ts <plugin-name>`.
3. Report the created file tree.
4. Suggest next steps: editing `SKILL.md`, adding agents or hooks, updating `aipm.toml`.

**What gets created:**

```
.ai/<plugin-name>/
  aipm.toml                    # minimal manifest
  .claude-plugin/
    plugin.json                # Claude registration
  skills/
    <plugin-name>/
      SKILL.md                 # starter skill file
```

The new plugin is automatically registered in `.ai/.claude-plugin/marketplace.json`.

## Agents

### `marketplace-scanner`

A read-only analysis agent that explains the contents of the `.ai/` marketplace
directory.

**When to invoke:** "What plugins do I have installed?" or "Explain my local marketplace."

The agent:
- Lists every plugin directory under `.ai/` that contains an `aipm.toml`.
- Summarises each plugin's name, version, type, description, and declared components.
- Reads and explains specific skills, agents, or hooks on request.
- Never modifies any files.

**Declared tools:** `Read`, `Glob`, `Grep`, `LS`.

## Hooks

### `PostToolUse` — tool usage logger

Every tool call made during a session appends a timestamped line to
`.ai/.tool-usage.log`:

```
2026-04-14T12:34:56Z tool=Write
2026-04-14T12:34:57Z tool=Bash
```

This log is useful for auditing what operations an agent performed. The `.ai/`
directory is typically gitignored for generated runtime state; add
`.ai/.tool-usage.log` to `.gitignore` if you do not want it committed.

## Installation

This plugin is created automatically when you run:

```bash
aipm init
```

or, to add it to an existing project:

```bash
aipm init --marketplace
```

Pass `--no-starter` to skip the plugin entirely:

```bash
aipm init --no-starter
```

## See also

- [Creating a plugin](../../docs/guides/creating-a-plugin.md) — scaffold and publish your own plugin packages
- [Local development](../../docs/guides/local-development.md) — link and test plugins without publishing
- [Using `aipm lint`](../../docs/guides/lint.md) — validate plugin quality
