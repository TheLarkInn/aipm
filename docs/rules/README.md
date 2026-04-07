# Lint Rule Reference

`aipm lint` ships with the following built-in rules. All rule IDs follow the `category/rule-name` hierarchy.

Use [`docs/guides/configuring-lint.md`](../guides/configuring-lint.md) to override severity, ignore paths, or suppress rules for your project.

## `skill/` rules

Rules that validate individual `SKILL.md` files.

| Rule | Default severity | Description |
|---|---|---|
| [`skill/missing-name`](skill/missing-name.md) | warning | `name` field absent from frontmatter |
| [`skill/missing-description`](skill/missing-description.md) | warning | `description` field absent from frontmatter |
| [`skill/name-invalid-chars`](skill/name-invalid-chars.md) | warning | `name` contains characters outside `[a-z0-9-_]` |
| [`skill/name-too-long`](skill/name-too-long.md) | warning | `name` exceeds the maximum length |
| [`skill/description-too-long`](skill/description-too-long.md) | warning | `description` exceeds the maximum length |
| [`skill/invalid-shell`](skill/invalid-shell.md) | warning | `shell` field value is not a recognised shell identifier |
| [`skill/oversized`](skill/oversized.md) | warning | File size exceeds 15 000 characters |

## `plugin/` rules

Rules that validate cross-file consistency within a plugin.

| Rule | Default severity | Description |
|---|---|---|
| [`plugin/broken-paths`](plugin/broken-paths.md) | error | `${CLAUDE_SKILL_DIR}/` or `${SKILL_DIR}/` reference points to a non-existent file |

## `hook/` rules

Rules that validate `hooks.json` configuration.

| Rule | Default severity | Description |
|---|---|---|
| [`hook/unknown-event`](hook/unknown-event.md) | error | Event name is not recognised by any supported AI tool |
| [`hook/legacy-event-name`](hook/legacy-event-name.md) | warning | Event name uses a deprecated alias instead of the canonical name |

## `agent/` rules

Rules that validate agent definition files (`.md` files inside `agents/`).

| Rule | Default severity | Description |
|---|---|---|
| [`agent/missing-tools`](agent/missing-tools.md) | warning | `tools` field absent from frontmatter |

## `source/` rules

Rules that validate placement of plugin features relative to the `.ai/` marketplace.

| Rule | Default severity | Description |
|---|---|---|
| [`source/misplaced-features`](source/misplaced-features.md) | warning | Plugin feature directory found in `.claude/` or `.github/` instead of `.ai/` |
