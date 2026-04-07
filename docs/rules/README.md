# Lint Rules Reference

`aipm lint` ships with 12 built-in rules organised into five categories. Each rule links to a dedicated page with examples and fix guidance.

## `skill/`

Rules that validate `SKILL.md` files.

| Rule | Severity | Description |
|------|----------|-------------|
| [`skill/description-too-long`](./skill/description-too-long.md) | warning | `description` frontmatter value exceeds the length limit |
| [`skill/invalid-shell`](./skill/invalid-shell.md) | error | `shell` frontmatter value is not a recognised shell identifier |
| [`skill/missing-description`](./skill/missing-description.md) | warning | `SKILL.md` is missing a `description` field in frontmatter |
| [`skill/missing-name`](./skill/missing-name.md) | warning | `SKILL.md` is missing a `name` field in frontmatter |
| [`skill/name-invalid-chars`](./skill/name-invalid-chars.md) | warning | Skill `name` contains characters that are not allowed |
| [`skill/name-too-long`](./skill/name-too-long.md) | warning | Skill `name` exceeds the maximum length |
| [`skill/oversized`](./skill/oversized.md) | warning | `SKILL.md` file exceeds the recommended size limit |

## `plugin/`

Rules that validate plugin directories and component file references.

| Rule | Severity | Description |
|------|----------|-------------|
| [`plugin/broken-paths`](./plugin/broken-paths.md) | error | Plugin references a file path that does not exist on disk |

## `hook/`

Rules that validate `hooks.json` files.

| Rule | Severity | Description |
|------|----------|-------------|
| [`hook/legacy-event-name`](./hook/legacy-event-name.md) | warning | Hook uses a deprecated (legacy) event name |
| [`hook/unknown-event`](./hook/unknown-event.md) | error | Hook uses an unrecognised event name |

## `agent/`

Rules that validate agent `.md` files inside `agents/` directories.

| Rule | Severity | Description |
|------|----------|-------------|
| [`agent/missing-tools`](./agent/missing-tools.md) | warning | Agent file is missing a `tools` field in frontmatter |

## `source/`

Rules that validate the location of plugin feature files within the project tree.

| Rule | Severity | Description |
|------|----------|-------------|
| [`source/misplaced-features`](./source/misplaced-features.md) | warning | Plugin feature files are outside the `.ai/` marketplace directory |

---

## Configuring rules

All rules can be overridden in the `[workspace.lints]` section of `aipm.toml`.
See the [lint configuration guide](../guides/configuring-lint.md) for severity overrides, path ignores, and per-rule configuration.
