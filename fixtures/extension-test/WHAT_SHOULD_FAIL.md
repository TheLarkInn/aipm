# Lint Fixture — Expected Failures

This workspace is intentionally broken to validate every `aipm lint` rule.
Open any file below in VS Code (with the `aipm lsp` server running) to see inline
diagnostics in the Problems panel. All failures listed here should appear as
squiggly underlines or file-level errors.

---

## `aipm.toml` — Schema Violations (Taplo / tomlValidation)

These are caught by **Taplo's JSON Schema validation**, not `aipm lsp`.
You need the Even Better TOML extension + the bundled schema to see them.

| Line | Violation | Expected |
|------|-----------|----------|
| `"skill/not-a-real-rule" = "allow"` | Unknown rule ID | Schema error: `additionalProperties: false` rejects any key not in the rule ID pattern |
| `"skill/missing-name" = "sometimes"` | Invalid severity value | Schema error: `"sometimes"` is not in `["allow", "warn", "warning", "error", "deny"]` |

---

## `.ai/broken-skills/skills/` — Skill Rule Violations

### `missing-name/SKILL.md`
- **`skill/missing-name`** (Warning): Frontmatter has no `name` field.

### `missing-desc/SKILL.md`
- **`skill/missing-description`** (Warning): Frontmatter has no `description` field.

### `oversized/SKILL.md`
- **`skill/oversized`** (Warning): File body exceeds 15,000 bytes (~28 KB).

### `name-too-long/SKILL.md`
- **`skill/name-too-long`** (Warning): `name` value is 85 chars, limit is 64.

### `invalid-chars/SKILL.md`
- **`skill/name-invalid-chars`** (Warning): `name: has@invalid#chars!` — `@`, `#`, `!` are not in `[a-zA-Z0-9._- ]`.

### `desc-too-long/SKILL.md`
- **`skill/description-too-long`** (Warning): `description` is 1025 chars, limit is 1024.

### `invalid-shell/SKILL.md`
- **`skill/invalid-shell`** (Error): `shell: zsh` — only `bash` and `powershell` are valid.

### `broken-ref/SKILL.md`
- **`plugin/broken-paths`** (Error): Two `${CLAUDE_SKILL_DIR}/` references point to files that do not exist:
  - `scripts/deploy.sh`
  - `bin/setup`

---

## `.ai/broken-skills/agents/` — Agent Rule Violations

### `no-tools.md`
- **`agent/missing-tools`** (Warning): Frontmatter has no `tools` field.

---

## `.ai/broken-skills/hooks/` — Hook Rule Violations

### `hooks.json`
- **`hook/unknown-event`** (Error): `"InvalidEvent"` is not a recognized hook event for any tool.
- **`hook/legacy-event-name`** (Warning): `"Stop"` is a legacy PascalCase name — use `"agentStop"` instead.

---

## `.ai/.claude-plugin/marketplace.json` — Marketplace Rule Violations

- **`marketplace/source-resolve`** (Error): `"source": "./ghost-source"` — the directory `.ai/ghost-source` does not exist on disk.
- **`marketplace/plugin-field-mismatch`** (Error × 2): The `field-mismatch` entry has:
  - `name: "field-mismatch-marketplace"` in marketplace.json vs `"completely-different-name"` in plugin.json
  - `description` also differs between the two files
- **`plugin/missing-registration`** (Error): `.ai/unregistered/` exists on disk but is not listed in `marketplace.json`.
- **`plugin/missing-manifest`** (Error): `.ai/no-manifest/` exists on disk but has no `.claude-plugin/plugin.json`.

---

## `.ai/bad-fields/.claude-plugin/plugin.json` — Plugin Required Fields

- **`plugin/required-fields`** (Error × 3):
  - `name` is present but empty (`""`)
  - `description` field is missing entirely
  - `author` field is missing entirely

---

## `.claude/skills/` — Misplaced Features

### `misplaced-skill/SKILL.md`
- **`source/misplaced-features`** (Warning): This skill lives under `.claude/skills/` instead of `.ai/<plugin>/skills/`. Skills belonging to a marketplace plugin should be inside `.ai/`.

---

## Summary: All 17 Rule IDs That Should Fire

| Rule ID | Severity | File |
|---------|----------|------|
| `skill/missing-name` | Warning | `broken-skills/skills/missing-name/SKILL.md` |
| `skill/missing-description` | Warning | `broken-skills/skills/missing-desc/SKILL.md` |
| `skill/oversized` | Warning | `broken-skills/skills/oversized/SKILL.md` |
| `skill/name-too-long` | Warning | `broken-skills/skills/name-too-long/SKILL.md` |
| `skill/name-invalid-chars` | Warning | `broken-skills/skills/invalid-chars/SKILL.md` |
| `skill/description-too-long` | Warning | `broken-skills/skills/desc-too-long/SKILL.md` |
| `skill/invalid-shell` | Error | `broken-skills/skills/invalid-shell/SKILL.md` |
| `plugin/broken-paths` | Error | `broken-skills/skills/broken-ref/SKILL.md` |
| `agent/missing-tools` | Warning | `broken-skills/agents/no-tools.md` |
| `hook/unknown-event` | Error | `broken-skills/hooks/hooks.json` |
| `hook/legacy-event-name` | Warning | `broken-skills/hooks/hooks.json` |
| `marketplace/source-resolve` | Error | `.ai/.claude-plugin/marketplace.json` |
| `marketplace/plugin-field-mismatch` | Error | `.ai/.claude-plugin/marketplace.json` |
| `plugin/missing-registration` | Error | `.ai/unregistered/` |
| `plugin/missing-manifest` | Error | `.ai/no-manifest/` |
| `plugin/required-fields` | Error | `bad-fields/.claude-plugin/plugin.json` |
| `source/misplaced-features` | Warning | `.claude/skills/` |
