# Local Plugin Development

Use `aipm link` and `aipm unlink` to develop a plugin locally and see changes reflected immediately — without publishing a new version.

## Overview

When you `link` a plugin:

1. A directory link is created from `.ai/<plugin-name>/` to your local plugin directory.
2. The link is recorded in `.aipm/links.toml`.
3. The plugin name entry is added to `.ai/.gitignore` so the link is not committed.

Changes to the local plugin directory are reflected immediately — no reinstall needed.

## Requirements

The local plugin directory must contain a valid `aipm.toml` with a `[package]` section that includes a `name` field.

```toml
# my-plugin/aipm.toml
[package]
name = "my-plugin"
version = "0.1.0"
type = "skill"
```

## Linking a plugin

```bash
aipm link ../my-plugin
```

This reads the package name from `../my-plugin/aipm.toml` and creates a link at `.ai/my-plugin/`.

```bash
# From the consuming project:
aipm link /absolute/path/to/my-plugin

# Or relative to the current directory:
aipm link ../my-plugin
aipm link plugins/my-plugin
```

## Unlinking a plugin

Restore the registry version by removing the link:

```bash
aipm unlink my-plugin
```

This removes the directory link, the `.aipm/links.toml` entry, and the `.ai/.gitignore` entry.

## Listing active links

```bash
aipm list --linked
```

Shows all currently linked packages and the paths they point to.

## Development workflow

```bash
# 1. Clone or create the plugin you want to develop
git clone https://github.com/org/my-plugin ../my-plugin

# 2. Link it into the consuming project
cd my-consuming-project
aipm link ../my-plugin

# 3. Make changes to the plugin
# ... edit ../my-plugin/skills/my-skill/SKILL.md ...

# 4. Changes are reflected immediately — no reinstall needed

# 5. Lint while developing
aipm lint

# 6. When done, unlink and reinstall from the registry
aipm unlink my-plugin
aipm install my-plugin@^1.0
```

## Link state

Active links are recorded in `.aipm/links.toml` at the project root:

```toml
[[link]]
name = "my-plugin"
path = "/absolute/path/to/my-plugin"
linked_at = "2026-01-15T10:30:00Z"
```

## Gitignore management

`aipm link` automatically adds the linked plugin name to `.ai/.gitignore` to prevent accidentally committing the symlink target:

```
# .ai/.gitignore
my-plugin
```

`aipm unlink` removes the entry.

## Relationship to `aipm install --global`

`aipm link` is for **project-local** development overrides. For making a plugin available across all projects, use `aipm install --global` instead. See [`docs/guides/global-plugins.md`](./global-plugins.md).

See also: [`aipm link`](../../README.md#aipm-link), [`aipm unlink`](../../README.md#aipm-unlink), [`aipm list`](../../README.md#aipm-list).
