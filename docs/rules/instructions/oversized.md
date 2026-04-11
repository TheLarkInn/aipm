# instructions/oversized

**Severity:** warning
**Fixable:** No

Checks that instruction files (`CLAUDE.md`, `AGENTS.md`, `COPILOT.md`, `GEMINI.md`,
`INSTRUCTIONS.md`, and `*.instructions.md` files anywhere in the project tree) do not exceed
configurable line and character limits. Oversized instruction files slow down context loading,
consume more tokens, and may be truncated or rejected by AI runtimes.

Up to two diagnostics are emitted per file — one for the line limit and one for the character
limit — so both problems are visible in a single run.

## Default limits

| Threshold | Default | Config key |
|-----------|---------|------------|
| Maximum lines | 100 | `lines` |
| Maximum characters | 15 000 | `characters` |

## Why these defaults?

The **100-line** default encourages writing concise, focused instruction files. An instruction
file that spans hundreds of lines is difficult to maintain, hard for humans to review, and
likely includes content better placed in linked documents.

The **15 000-character** budget mirrors the `SKILL_CHAR_BUDGET` default used by Copilot CLI
and is used here as a **tool-agnostic quality guardrail**: a file within this budget will work
reliably across all supported AI runtimes. Exceeding it risks silent truncation by the runtime.

## Examples

### Incorrect

```markdown
<!-- CLAUDE.md — 150 lines, exceeds the 100-line default -->
# Project rules

[...content spanning more than 100 lines...]
```

### Correct

```markdown
<!-- CLAUDE.md — concise, under both limits -->
# Project rules

Keep changes small and focused. Run `cargo test` before committing.
See [coding-standards.md](./docs/coding-standards.md) for full guidelines.
```

## Configuring thresholds

Override the defaults in your workspace `aipm.toml`:

```toml
[workspace.lints."instructions/oversized"]
level = "error"
lines = 200
characters = 20000
```

You can configure per-rule options without specifying a `level` — the rule will run at its
default severity:

```toml
[workspace.lints."instructions/oversized"]
lines = 200
characters = 20000
```

## Suppress for specific paths

Use rule-level `ignore` to exclude vendor or generated instruction files:

```toml
[workspace.lints."instructions/oversized"]
ignore = ["**/vendor/**", "**/third-party/**"]
```

## Resolve-imports mode

When `resolve-imports = true`, the rule follows `@path/to/file.md` import lines and relative
markdown inline links (`[label](relative.md)`) transitively, accumulating the combined line and
character counts of the entry file and all its imports. This is useful when a root instruction
file is small but references large shared files.

```toml
[workspace.lints."instructions/oversized"]
resolve-imports = true
lines = 200
characters = 20000
```

When a limit is exceeded in resolve-imports mode, the diagnostic message includes both the
resolved total and the direct (entry-file-only) counts:

```
instruction file exceeds 200 line limit (resolved total: 312 lines, direct: 45 lines)
```

### Safety constraints

- Circular import chains are detected and broken — no infinite loops.
- Absolute paths (`/etc/passwd`) and path-traversal segments (`../secrets`) are rejected.
- External URLs (`https://…`) are not followed.

## How to fix

1. **Trim the file** — remove redundant rules, consolidate overlapping sections.
2. **Split into focused files** — move subsections into separate `*.instructions.md` files and
   link to them.  With `resolve-imports = false` (the default), linked files are not counted
   against the root file's limits.
3. **Move reference material externally** — link to `docs/` pages instead of embedding large
   reference tables inline.
4. **Raise the limit** — if the current content is intentionally comprehensive, increase
   `lines` or `characters` in `aipm.toml` (see [Configuring thresholds](#configuring-thresholds)).

## See also

- [skill/oversized](../skill/oversized.md) — similar size check for SKILL.md files
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
- [Configuring lint](../../guides/configuring-lint.md) — override rule severity or suppress rules per path
