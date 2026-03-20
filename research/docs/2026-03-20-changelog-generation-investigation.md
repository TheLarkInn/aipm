---
date: 2026-03-20
researcher: Claude
git_commit: e65b358885ad06b0d3367b0b4ea30d12dedc7727
branch: main
repository: aipm
topic: "Why are changelogs empty and how to automate changelog generation"
tags: [research, changelog, git-cliff, release-plz, conventional-commits]
status: complete
last_updated: 2026-03-20
last_updated_by: Claude
---

# Research

## Research Question
Why are my changelogs not automatically generated? Shouldn't the type-based semver bumping also be able to determine the changelog too? How can I automate changelog generation in the easiest way possible?

## Summary

The changelog pipeline is **fully wired and functional** — `release-plz.toml` configures `cliff.toml` as the changelog engine, `changelog_update = true` is set, and CHANGELOG.md files exist in the workspace root and each crate. The problem is that **commit messages do not follow conventional commit format**, and `cliff.toml` sets `filter_unconventional = true`, which silently discards all non-conventional commits. This results in version headers with no content.

The fix requires no new tooling — just adopting conventional commit prefixes (`feat:`, `fix:`, `refactor:`, etc.) in commit messages going forward. Optionally, a commit-msg lint can enforce this in CI.

## Detailed Findings

### The Symptom: Empty Changelog Entries

All four CHANGELOG.md files ([CHANGELOG.md](https://github.com/TheLarkInn/aipm/blob/e65b358885ad06b0d3367b0b4ea30d12dedc7727/CHANGELOG.md), [crates/aipm/CHANGELOG.md](https://github.com/TheLarkInn/aipm/blob/e65b358885ad06b0d3367b0b4ea30d12dedc7727/crates/aipm/CHANGELOG.md), [crates/libaipm/CHANGELOG.md](https://github.com/TheLarkInn/aipm/blob/e65b358885ad06b0d3367b0b4ea30d12dedc7727/crates/libaipm/CHANGELOG.md), [crates/aipm-pack/CHANGELOG.md](https://github.com/TheLarkInn/aipm/blob/e65b358885ad06b0d3367b0b4ea30d12dedc7727/crates/aipm-pack/CHANGELOG.md)) contain version headers but no entries:

```markdown
## [0.2.1] - 2026-03-19

## [0.2.0] - 2026-03-19

## [0.1.2] - 2026-03-19

## [0.1.1] - 2026-03-19
```

### The Root Cause: Non-Conventional Commit Messages

The `cliff.toml` configuration at [`cliff.toml:28-29`](https://github.com/TheLarkInn/aipm/blob/e65b358885ad06b0d3367b0b4ea30d12dedc7727/cliff.toml#L28-L29) sets:

```toml
conventional_commits = true
filter_unconventional = true
```

This means git-cliff **only includes commits that match the conventional commit format** (`type: description` or `type(scope): description`). All other commits are silently filtered out.

The actual commit messages on `main` do NOT follow this format:

| Commit | Message | Missing Prefix |
|--------|---------|----------------|
| e65b358 | `Better default plugin with scaffold skill, agent, hook, and --no-starter flag (#30) (#32)` | Should be `feat:` |
| cf04f88 | `Fix release PR auto-merge by using correct action output (#29)` | Should be `fix:` |
| 75a0374 | `Fix Claude settings: use correct source type and minimal scaffold (#27)` | Should be `fix:` |
| 0286285 | `Extract ToolAdaptor trait, remove vscode/copilot settings (#25)` | Should be `refactor:` |
| 3564a8a | `Refactor workspace_init into module directory with adaptors (#23)` | Should be `refactor:` |
| 8b950cf | `Replace hand-rolled release.yml with cargo-dist (#22)` | Should be `ci:` or `build:` |

The only commits that DO follow the format are the auto-generated release commits: `chore: release v0.2.1 (#28)`. These match the `^chore` parser but don't produce meaningful changelog content.

### The Existing Pipeline (Already Correct)

The pipeline is fully configured and working as designed:

1. **release-plz.toml** ([`release-plz.toml:2-3`](https://github.com/TheLarkInn/aipm/blob/e65b358885ad06b0d3367b0b4ea30d12dedc7727/release-plz.toml#L2-L3)): `changelog_config = "cliff.toml"` and `changelog_update = true`
2. **cliff.toml** ([`cliff.toml`](https://github.com/TheLarkInn/aipm/blob/e65b358885ad06b0d3367b0b4ea30d12dedc7727/cliff.toml)): Proper conventional commit parsers for feat/fix/doc/perf/refactor/style/test/chore/ci/build
3. **release-plz workflow** ([`.github/workflows/release-plz.yml`](https://github.com/TheLarkInn/aipm/blob/e65b358885ad06b0d3367b0b4ea30d12dedc7727/.github/workflows/release-plz.yml)): Runs `release-pr` on push to main, which invokes git-cliff to generate changelog content
4. **semver_check = true** in release-plz.toml: cargo-semver-checks determines version bump type from API changes

The semver bump and changelog generation are **separate concerns**:
- **Semver bump**: Determined by `cargo-semver-checks` analyzing actual API changes in Rust code (not commit messages)
- **Changelog content**: Determined by `git-cliff` parsing commit messages for conventional commit prefixes

This is why versions bump correctly (semver-checks looks at code) but changelogs are empty (git-cliff looks at commit messages and finds nothing matching).

### The Spec Anticipated This

The CI/CD spec at [`specs/2026-03-16-ci-cd-release-automation.md`](https://github.com/TheLarkInn/aipm/blob/e65b358885ad06b0d3367b0b4ea30d12dedc7727/specs/2026-03-16-ci-cd-release-automation.md) has an open question at line 856:

> **Conventional commit enforcement**: Should we add a `commitlint` or `conform` check to CI that rejects non-conventional commit messages? Or just let release-plz handle whatever commits land?

The answer is now clear: without conventional commits, the changelog will always be empty.

## Easiest Path to Automated Changelogs

### Option A: Just Use Conventional Commit Prefixes (Zero Config Change)

Start writing commit messages like:
- `feat: add scaffold skill, agent, hook, and --no-starter flag`
- `fix: correct release PR auto-merge action output`
- `refactor: extract ToolAdaptor trait`

The existing `cliff.toml` and `release-plz.toml` will handle everything else automatically. No tooling changes needed.

### Option B: Change cliff.toml to Include All Commits (Quick Fix)

In `cliff.toml`, change `filter_unconventional = true` to `filter_unconventional = false` and add a catch-all parser:

```toml
commit_parsers = [
  { message = "^feat", group = "Features" },
  { message = "^fix", group = "Bug Fixes" },
  { message = "^doc", group = "Documentation" },
  { message = "^perf", group = "Performance" },
  { message = "^refactor", group = "Refactoring" },
  { message = "^style", group = "Style" },
  { message = "^test", group = "Testing" },
  { message = "^chore", group = "Miscellaneous" },
  { message = "^ci", group = "CI/CD" },
  { message = "^build", group = "Build" },
  { message = ".*", group = "Other" },  # catch-all for non-conventional
]
```

This would populate changelogs immediately from existing commit history but produces less organized output.

### Option C: Enforce Conventional Commits in CI (Recommended Long-Term)

Add a commit message lint to CI. Options:
- **commitlint** (Node.js) — most popular, runs in CI via GitHub Action
- **conform** (Go binary) — single binary, Rust-ecosystem friendly
- **cocogitto** (Rust-native) — `cog check` validates commit messages, integrates with git hooks

Since this is a Rust project, `cocogitto` (`cog`) is the most natural fit and can be added as a `cargo install cocogitto` step in CI.

## Code References

- [`release-plz.toml`](https://github.com/TheLarkInn/aipm/blob/e65b358885ad06b0d3367b0b4ea30d12dedc7727/release-plz.toml) — release-plz configuration with `changelog_update = true`
- [`cliff.toml:28-29`](https://github.com/TheLarkInn/aipm/blob/e65b358885ad06b0d3367b0b4ea30d12dedc7727/cliff.toml#L28-L29) — `filter_unconventional = true` (the line causing empty changelogs)
- [`cliff.toml:31-42`](https://github.com/TheLarkInn/aipm/blob/e65b358885ad06b0d3367b0b4ea30d12dedc7727/cliff.toml#L31-L42) — commit parsers for conventional commit types
- [`.github/workflows/release-plz.yml`](https://github.com/TheLarkInn/aipm/blob/e65b358885ad06b0d3367b0b4ea30d12dedc7727/.github/workflows/release-plz.yml) — release-plz workflow running `release-pr` and `release`
- [`crates/aipm/CHANGELOG.md`](https://github.com/TheLarkInn/aipm/blob/e65b358885ad06b0d3367b0b4ea30d12dedc7727/crates/aipm/CHANGELOG.md) — empty changelog entries demonstrating the problem
- [`specs/2026-03-16-ci-cd-release-automation.md:856`](https://github.com/TheLarkInn/aipm/blob/e65b358885ad06b0d3367b0b4ea30d12dedc7727/specs/2026-03-16-ci-cd-release-automation.md#L856) — open question about conventional commit enforcement

## Architecture Documentation

The release pipeline flow:
```
push to main → release-plz workflow → cargo-semver-checks (determines bump type)
                                    → git-cliff (generates changelog from conventional commits)
                                    → creates release PR with version bump + CHANGELOG.md
merge release PR → release-plz tags → cargo publish → release.yml builds binaries
```

The semver bump is code-aware (API changes), but the changelog is message-aware (commit prefixes). These are independent systems that happen to run in the same pipeline.

## Related Research

- [`specs/2026-03-16-ci-cd-release-automation.md`](../specs/2026-03-16-ci-cd-release-automation.md) — Full CI/CD spec covering the release-plz + git-cliff + cargo-dist pipeline
- [`research/docs/2026-03-16-rust-cross-platform-release-distribution.md`](2026-03-16-rust-cross-platform-release-distribution.md) — Research on cross-platform release distribution tooling

## Open Questions

- Should Option A (adopt conventional commits) be combined with Option C (enforce in CI) to prevent regressions?
- Should existing CHANGELOGs be retroactively populated by rewriting commit messages, or start fresh from the next release?
- Should the `chore: release` commits be excluded from changelogs via a `skip = true` parser to avoid noise?
