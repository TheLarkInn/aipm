---
date: 2026-03-22 08:52:19 PDT
researcher: Claude (Opus 4.6)
git_commit: afef7be62d9856bc53f08afa754195a49e2102af
branch: feat/strict-branch-coverage
repository: aipm
topic: "Branch coverage gap analysis and test plan to reach 90%"
tags: [research, code-coverage, branch-coverage, testing, gap-analysis]
status: complete
last_updated: 2026-03-22
last_updated_by: Claude (Opus 4.6)
---

# Branch Coverage Gap Analysis: Plan to Reach 90%

## Research Question

Analyze the branch coverage report (80.72% overall, 32 of 166 branches missed) and create a prioritized plan for reaching 90% branch coverage, including identification of potentially untestable branches.

## Summary

The workspace currently has **80.72% branch coverage** across production code (excluding tests/research/specs). 32 branches are missed across 6 files. Of these, **25 branches are straightforwardly testable** with unit tests, **~5 are I/O error paths** that require filesystem tricks to test, and **~2 are structurally unreachable** dead code behind prior guards. Reaching 90% requires covering approximately 16 additional branches (from 134/166 to 150/166 = 90.4%).

### Current Coverage Snapshot

| File | Branches | Missed | Coverage | Gap Type |
|------|----------|--------|----------|----------|
| `aipm/src/main.rs` | 10 | 0 | 100% | None |
| `manifest/validate.rs` | 70 | 7 | 90.0% | At threshold |
| `workspace_init/mod.rs` | 20 | 4 | 80.0% | I/O errors + one logic branch |
| `adaptors/claude.rs` | 14 | 3 | 78.6% | Malformed JSON inputs |
| `aipm-pack/main.rs` | 4 | 1 | 75.0% | No-subcommand path |
| `init.rs` | 42 | 13 | 69.1% | Name validation edge cases + I/O errors |
| `version.rs` | 4 | 2 | 50.0% | select_best None + is_prerelease false |
| `manifest/error.rs` | 2 | 2 | 0.0% | format_errors never called |
| Others (no branches) | 0 | 0 | - | - |
| **TOTAL** | **166** | **32** | **80.72%** | |

## Detailed Findings by File (Prioritized by Impact)

### 1. `init.rs` — 13 missed branches (highest impact)

**File**: `crates/libaipm/src/init.rs`

Covering even half of these gets us most of the way to 90%. The missed branches fall into two categories:

#### Category A: Name validation edge cases (5 branches, easily testable)

| Line | Branch | Test Needed |
|------|--------|-------------|
| 106-107 | `rest.find('/')` returns `None` (scoped name with no slash) | `assert!(!is_valid_package_name("@noslash"))` |
| 111 | `scope.is_empty()` true | `assert!(!is_valid_package_name("@/pkg"))` |
| 111 | `pkg.is_empty()` true | `assert!(!is_valid_package_name("@org/"))` |
| 114 | `is_valid_segment(scope)` false (short-circuit &&) | `assert!(!is_valid_package_name("@ORG/my-plugin"))` |
| 114 | `is_valid_segment(pkg)` false (right side of &&) | `assert!(!is_valid_package_name("@org/INVALID"))` |

These can all be added to the existing `invalid_names` test at line 216.

#### Category B: Path edge cases (2-3 branches, testable)

| Line | Branch | Test Needed |
|------|--------|-------------|
| 70-73 | `file_name()` returns `None` or `to_str()` returns `None` → `Error::NoDirectoryName` | Call `init()` with `dir: Path::new("/")` and `name: None` |

#### Category C: I/O error paths (5-6 branches, hard to test)

| Lines | Branch | Why Hard |
|-------|--------|----------|
| 88, 89, 93, 94 | `?` Err from `create_dir_all`, `create_directory_layout`, `File::create`, `write_all` in `init()` | Requires simulating filesystem failures |
| 135-175 | `?` Err from various `create_dir_all`, `create_gitkeep`, `create_skill_template` calls | Same — I/O failures |

**Question for stakeholder**: Should we test I/O error paths by writing to read-only directories, or accept these as inherently untestable and exclude them from the threshold? These account for ~8 of the 13 missed branches.

### 2. `manifest/error.rs` — 2 missed branches (easy win)

**File**: `crates/libaipm/src/manifest/error.rs:77-86`

The `format_errors` function has an `if i > 0` branch at line 80 that is never reached because no test constructs an `Error::Multiple` value. The fix is trivial:

**Test needed**: Call `parse_and_validate` on a manifest with 2+ simultaneous errors, then call `.to_string()` on the result.

```rust
// Example: empty name AND empty version triggers Multiple
let content = "[package]\nname = \"\"\nversion = \"\"";
let err = parse_and_validate(content, None).unwrap_err();
let msg = err.to_string();
assert!(msg.contains(";"));  // separator means both branches hit
```

This covers **both** branches (i == 0 and i > 0) in a single test.

### 3. `version.rs` — 2 missed branches (easy win)

**File**: `crates/libaipm/src/version.rs`

| Line | Branch | Test Needed |
|------|--------|-------------|
| 112 | `select_best` returns `None` (no candidates match) | `req.select_best(&non_matching_candidates)` → assert `None` |
| 51-53 | `is_prerelease` returns `false` on stable version | `assert!(!Version::parse("1.0.0").is_prerelease())` |

Both are trivial additions to the existing test module.

### 4. `aipm-pack/main.rs` — 1 missed branch (easy win)

**File**: `crates/aipm-pack/src/main.rs:54-59`

The `None` arm of `match cli.command` (invoked with no subcommand) is untested.

**Test needed**: Add to `crates/aipm-pack/tests/init_e2e.rs`:

```rust
#[test]
fn no_subcommand_prints_usage() {
    aipm_pack()
        .assert()
        .success()
        .stdout(predicate::str::contains("aipm-pack"));
}
```

### 5. `adaptors/claude.rs` — 3 missed branches (moderate)

**File**: `crates/libaipm/src/workspace_init/adaptors/claude.rs`

| Line | Branch | Test Needed |
|------|--------|-------------|
| 50-51 | `serde_json::from_str` returns `Err` (invalid JSON) | Write `"{{invalid"` to settings file, call `apply()`, assert error |
| 53 | `json.as_object_mut()` returns `None` (root is not object) | Write `"[1,2,3]"` to settings file, call `apply()`, assert error |
| 81 or 93 | `ekm.as_object_mut()` or `enabled.as_object_mut()` returns `None` (value is wrong type) | Write `{"extraKnownMarketplaces": 42}` to settings, call `apply()` |

All three are testable by writing malformed JSON before calling the adaptor.

### 6. `workspace_init/mod.rs` — 4 missed branches (mixed)

**File**: `crates/libaipm/src/workspace_init/mod.rs`

| Line | Branch | Test Needed | Difficulty |
|------|--------|-------------|------------|
| 110 | `adaptor.apply()` returns `Ok(false)` inside `init()` | Pre-create fully-configured `.claude/settings.json`, call `init()` with `marketplace: true` | Easy |
| 132-133 | `parse_and_validate` returns `Err` inside `init_workspace` | Structurally unreachable — `generate_workspace_manifest()` always produces valid content | Dead code |
| 135-137 | I/O errors from `create_dir_all` / `File::create` / `write_all` | Requires filesystem tricks | Hard |

**Question for stakeholder**: Line 132-133 is structurally dead code — the validation error path cannot be reached because the manifest generator always produces valid output. Should this be refactored (remove the validation call), or accepted as defensive coding?

## Prioritized Test Plan

### Phase 1: Quick Wins (covers ~8 branches, gets to ~85.5%)

| # | File | Branches Covered | Effort |
|---|------|-----------------|--------|
| 1 | `init.rs` name validation | 5 | Add 5 assertions to existing `invalid_names` test |
| 2 | `manifest/error.rs` | 2 | One new test with multi-error manifest |
| 3 | `version.rs` | 2 | Two small tests (select_best None + is_prerelease false) |

**Estimated result**: 142/166 = **85.5%** branch coverage

### Phase 2: Moderate Effort (covers ~5 branches, gets to ~88.6%)

| # | File | Branches Covered | Effort |
|---|------|-----------------|--------|
| 4 | `aipm-pack/main.rs` | 1 | One E2E test (no-subcommand invocation) |
| 5 | `adaptors/claude.rs` | 3 | Three unit tests with malformed JSON |
| 6 | `workspace_init/mod.rs` adaptor false path | 1 | One unit test with pre-configured settings |

**Estimated result**: 147/166 = **88.6%** branch coverage

### Phase 3: Close the Gap (covers ~3 branches, reaches 90%+)

| # | File | Branches Covered | Effort |
|---|------|-----------------|--------|
| 7 | `init.rs` path edge case (`NoDirectoryName`) | 2-3 | Test with root path or pathless dir |
| 8 | `validate.rs` (already at 90%, but could pick up 1-2 more) | 1-2 | Depends on which 7 branches are missed |

**Estimated result**: 150-152/166 = **90.4-91.6%** branch coverage

### Branches to Accept as Untestable (~8 branches)

These are I/O error paths behind `?` operators on filesystem operations (`create_dir_all`, `File::create`, `write_all`). They exist in:
- `init.rs` lines 88, 89, 93, 94, 135-175 (~6 branches)
- `workspace_init/mod.rs` lines 135-137 (~2 branches)

Testing these requires:
- Writing to read-only directories (platform-dependent behavior)
- Or injecting mock filesystems (requires refactoring production code to accept a filesystem trait)

**Question for stakeholder**: These 8 branches represent 4.8% of total branches. If we accept them as untestable, the theoretical maximum coverage is ~95.2%. Should we:
1. Accept them and set the threshold at 88% instead of 90%?
2. Test them with platform-specific filesystem tricks (read-only dirs)?
3. Refactor to inject filesystem abstraction (adds complexity)?

## Code References

- `crates/libaipm/src/init.rs` — Plugin init with 13 missed branches (name validation + I/O)
- `crates/libaipm/src/manifest/error.rs:77-86` — `format_errors` with `if i > 0` (2 missed)
- `crates/libaipm/src/version.rs:43-112` — Version parsing + selection (2 missed)
- `crates/aipm-pack/src/main.rs:54-59` — No-subcommand match arm (1 missed)
- `crates/libaipm/src/workspace_init/adaptors/claude.rs:50-93` — JSON parsing/mutation (3 missed)
- `crates/libaipm/src/workspace_init/mod.rs:110-137` — Adaptor apply + workspace I/O (4 missed)
- `crates/libaipm/src/manifest/validate.rs` — At 90% already, 7 missed branches

## Related Research

- `research/docs/2026-03-22-rust-code-coverage-implementation.md` — Coverage tooling and correctness gate framing
- `specs/2026-03-22-strict-branch-coverage.md` — Spec for 90% branch coverage enforcement

## Open Questions

1. **I/O error branches (~8 branches, 4.8%)**: Accept as untestable, test with platform tricks, or refactor to inject filesystem abstraction?
2. **Dead code branch** (`workspace_init/mod.rs:132-133`): Remove the unreachable validation call, or keep as defensive coding?
3. **Threshold adjustment**: If I/O branches are accepted as untestable, should the threshold be 88% instead of 90%? Or keep 90% and require platform-specific I/O tests?
4. **validate.rs**: Should we analyze the 7 missed branches there as well? It's already at 90% so it doesn't strictly need work, but improving it gives headroom.
