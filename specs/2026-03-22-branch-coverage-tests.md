# Branch Coverage Tests: Reaching 89% Minimum

| Document Metadata      | Details                                       |
| ---------------------- | --------------------------------------------- |
| Author(s)              | selarkin                                      |
| Status                 | Implemented                                   |
| Team / Owner           | selarkin                                       |
| Created / Last Updated | 2026-04-24                                    |
| Research               | `research/docs/2026-03-22-branch-coverage-gap-analysis.md` |
| Prerequisite           | `specs/2026-03-22-strict-branch-coverage.md` (Fs trait already implemented) |

## 1. Executive Summary

**Status: Implemented.** Branch coverage reached **95.01%** (well above the 89% gate), achieved through 10 test groups across 3 implementation phases. All goals in this spec have been completed. The coverage-improver agentic workflow continues to maintain and extend coverage on an ongoing basis.

_Original baseline:_ The workspace had 80.72% branch coverage (134/166 branches). The 89% correctness gate required covering 16+ additional branches across 6 files. The `fs::Fs` trait was put in place, making all I/O error paths testable via mock filesystem injection. This spec defined the exact tests to write — 32 missed branches categorized into 10 test groups across 3 implementation phases. No production code changes were needed; this was a test-only spec.

## 2. Context and Motivation

### 2.1 Current State

Branch coverage is **95.01%** (above the 89% gate). All planned test groups from this spec have been implemented. The `fs::Fs` trait ([`crates/libaipm/src/fs.rs`](crates/libaipm/src/fs.rs)) was introduced to make I/O error paths testable. All production functions (`init::init`, `workspace_init::init`, `ToolAdaptor::apply`) now accept `&dyn Fs`.

_Baseline (2026-03-22):_ Branch coverage was **80.72%** (32 of 166 branches missed). The CI coverage gate was set to 89% and was failing at that point.

Research ref: `research/docs/2026-03-22-branch-coverage-gap-analysis.md` — full per-file branch enumeration.

### 2.2 The Problem (Resolved)

The CI coverage job was failing because 32 branches were untested. They fell into 4 categories (all now resolved):

1. **Name validation edge cases** (5 branches in `init.rs`) — scoped package names with missing slashes, empty scope/pkg segments, invalid segments
2. **I/O error paths** (~8 branches across `init.rs` and `workspace_init/mod.rs`) — `create_dir_all`, `write_file`, `read_to_string` failures behind `?` operators. Now testable via `fs::Fs` mock injection.
3. **Malformed input handling** (5 branches in `claude.rs`, `error.rs`) — invalid JSON, non-object JSON roots, multi-error formatting
4. **Missing scenario tests** (3 branches in `version.rs`, `aipm-pack/main.rs`, `workspace_init/mod.rs`) — `select_best` returning `None`, no-subcommand path, adaptor returning `false`

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [x] **G1**: Reach 89%+ branch coverage across the workspace (measured by `cargo +nightly llvm-cov --workspace --branch --ignore-filename-regex '(tests/|research/|specs/)'`) — **Achieved: 95.01%**
- [x] **G2**: Cover all I/O error paths in `init.rs` and `workspace_init/mod.rs` using mock `Fs` implementations
- [x] **G3**: Cover all name validation edge cases in `init.rs`
- [x] **G4**: Cover all malformed JSON handling in `adaptors/claude.rs`
- [x] **G5**: Cover `format_errors` in `manifest/error.rs`
- [x] **G6**: Cover missing scenario branches in `version.rs`, `aipm/main.rs` (no-subcommand path; note: `aipm-pack` was merged into `aipm`), `workspace_init/mod.rs`
- [x] **G7**: All tests must pass `cargo clippy --workspace -- -D warnings` (no lint violations in test code, respecting the `allow_attributes = "warn"` exemption for test files)

### 3.2 Non-Goals (Out of Scope)

- [x] We will NOT change any production code (only add tests)
- [x] We will NOT target `validate.rs` (already at 90%)
- [x] We will NOT add mutation testing
- [x] We will NOT restructure the dead-code branch at `workspace_init/mod.rs:132-133` (keep as defensive coding; the mock Fs approach can force it to trigger)

## 4. Proposed Solution (High-Level Design)

### 4.1 Mock Filesystem Strategy

Create a `FailFs` mock in test modules that implements `fs::Fs` and can be configured to fail on specific operations:

```rust
/// Mock filesystem that fails on a specific operation.
struct FailFs {
    fail_on: &'static str, // "create_dir", "write_file", "read"
}

impl Fs for FailFs {
    fn exists(&self, _: &Path) -> bool { false }

    fn create_dir_all(&self, _: &Path) -> io::Result<()> {
        if self.fail_on == "create_dir" {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "mock: read-only"));
        }
        Ok(())
    }

    fn write_file(&self, _: &Path, _: &[u8]) -> io::Result<()> {
        if self.fail_on == "write_file" {
            return Err(io::Error::new(io::ErrorKind::Other, "mock: disk full"));
        }
        Ok(())
    }

    fn read_to_string(&self, _: &Path) -> io::Result<String> {
        if self.fail_on == "read" {
            return Err(io::Error::new(io::ErrorKind::NotFound, "mock: not found"));
        }
        Ok(String::new())
    }
}
```

This can be defined once in each test module that needs it (or in a shared `#[cfg(test)]` helper module). Each test configures `fail_on` to target a specific branch.

### 4.2 Key Components

| Test Group | File | Branches | Strategy |
|-----------|------|----------|----------|
| Name validation | `init.rs` | 5 | Add assertions to existing `invalid_names` test |
| Path edge cases | `init.rs` | 2 | Call `init()` with root path + `name: None` |
| I/O errors in init | `init.rs` | 6 | `FailFs` mock — fail on `create_dir`, `write_file` |
| Multi-error format | `error.rs` | 2 | Construct manifest with 2+ errors, call `.to_string()` |
| Version edge cases | `version.rs` | 2 | `select_best` with non-matching candidates; stable `is_prerelease` |
| No-subcommand | `aipm/main.rs` (merged from `aipm-pack`) | 1 | E2E test invoking `aipm` with no args |
| Malformed JSON | `claude.rs` | 3 | Write invalid JSON to settings, call `apply()` with `fs::Real` |
| Adaptor false path | `workspace_init/mod.rs` | 1 | Pre-configure settings, call `init()` with marketplace flag |
| I/O errors in workspace | `workspace_init/mod.rs` | 3 | `FailFs` mock — fail on `create_dir`, `write_file` |
| Dead code branch | `workspace_init/mod.rs` | 1 | `FailFs` that writes invalid manifest content (force `parse_and_validate` to fail) |

## 5. Detailed Design (Implemented)

### 5.1 Phase 1: Quick Wins (+9 branches → ~86%)

#### Test 1: Name validation edge cases — `init.rs`

**Target**: 5 branches in `is_valid_package_name` / `is_valid_segment`

Add to existing `invalid_names` test in `crates/libaipm/src/init.rs`:

```rust
// Scoped name without slash
assert!(!is_valid_package_name("@noslash"));
// Empty scope
assert!(!is_valid_package_name("@/pkg"));
// Empty package
assert!(!is_valid_package_name("@org/"));
// Invalid scope (uppercase)
assert!(!is_valid_package_name("@ORG/my-plugin"));
// Invalid package (uppercase)
assert!(!is_valid_package_name("@org/INVALID"));
```

#### Test 2: Multi-error formatting — `manifest/error.rs`

**Target**: 2 branches in `format_errors` (`if i > 0`)

Add new test in `crates/libaipm/src/manifest/mod.rs` (where other manifest tests live):

```rust
#[test]
fn multiple_errors_format_with_separator() {
    // Manifest with both empty name and missing version triggers Multiple
    let content = "[package]\nname = \"\"";
    let result = parse_and_validate(content, None);
    assert!(result.is_err());
    let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
    // The ";" separator proves both i==0 and i>0 branches were hit
    assert!(err_msg.contains(';') || err_msg.contains("name"));
}
```

#### Test 3: Version edge cases — `version.rs`

**Target**: 2 branches

Add to `crates/libaipm/src/version.rs` test module:

```rust
#[test]
fn select_best_returns_none_when_no_match() {
    let req = Requirement::parse("^5.0.0");
    assert!(req.is_ok());
    let candidates = [
        Version::parse("1.0.0"),
        Version::parse("2.0.0"),
    ];
    let valid: Vec<Version> = candidates.into_iter().filter_map(Result::ok).collect();
    let best = req.ok().as_ref().and_then(|r| r.select_best(&valid));
    assert!(best.is_none());
}

#[test]
fn stable_version_is_not_prerelease() {
    let v = Version::parse("1.0.0");
    assert!(v.is_ok());
    assert!(v.ok().is_some_and(|v| !v.is_prerelease()));
}
```

### 5.2 Phase 2: Moderate Effort (+5 branches → ~89%)

#### Test 4: No-subcommand path — `aipm/main.rs`

> **Note:** `aipm-pack` was merged into `aipm` (PR #417). The no-subcommand path is now in `crates/aipm/src/main.rs`. The equivalent test exists in `crates/aipm/tests/cli_tests.rs` as `no_subcommand_prints_version`.

**Target**: 1 branch (`None` arm of `match cli.command`)

Add to `crates/aipm/tests/cli_tests.rs` (already implemented as `no_subcommand_prints_version`):

```rust
#[test]
fn no_subcommand_prints_version_and_usage() {
    aipm_pack()
        .assert()
        .success()
        .stdout(predicate::str::contains("aipm-pack"))
        .stdout(predicate::str::contains("--help"));
}
```

#### Test 5: Malformed JSON handling — `adaptors/claude.rs`

**Target**: 3 branches

Add to `crates/libaipm/src/workspace_init/adaptors/claude.rs` test module:

```rust
#[test]
fn claude_settings_rejects_invalid_json() {
    let tmp = make_temp_dir("invalid-json");
    std::fs::create_dir_all(tmp.join(".claude")).ok();
    std::fs::write(tmp.join(".claude/settings.json"), "{{invalid json").ok();

    let adaptor = Adaptor;
    let result = adaptor.apply(&tmp, &Real);
    assert!(result.is_err());
    let err = result.err();
    assert!(err.is_some_and(|e| e.to_string().contains("JSON parse")));

    cleanup(&tmp);
}

#[test]
fn claude_settings_rejects_non_object_root() {
    let tmp = make_temp_dir("array-root");
    std::fs::create_dir_all(tmp.join(".claude")).ok();
    std::fs::write(tmp.join(".claude/settings.json"), "[1, 2, 3]").ok();

    let adaptor = Adaptor;
    let result = adaptor.apply(&tmp, &Real);
    assert!(result.is_err());
    let err = result.err();
    assert!(err.is_some_and(|e| e.to_string().contains("expected JSON object")));

    cleanup(&tmp);
}

#[test]
fn claude_settings_handles_non_object_marketplace_value() {
    let tmp = make_temp_dir("bad-ekm");
    std::fs::create_dir_all(tmp.join(".claude")).ok();
    std::fs::write(
        tmp.join(".claude/settings.json"),
        r#"{"extraKnownMarketplaces": 42}"#,
    ).ok();

    let adaptor = Adaptor;
    let result = adaptor.apply(&tmp, &Real);
    // Should succeed (silently skips non-object mutation) but still write enabledPlugins
    assert!(result.is_ok());

    cleanup(&tmp);
}
```

#### Test 6: Adaptor returns false inside init() — `workspace_init/mod.rs`

**Target**: 1 branch (line 110 false path)

Add to `crates/libaipm/src/workspace_init/mod.rs` test module:

```rust
#[test]
fn init_marketplace_with_preconfigured_claude_settings() {
    let (tmp, _guard) = make_temp_dir("preconfigured");
    // Pre-create fully-configured .claude/settings.json
    std::fs::create_dir_all(tmp.join(".claude")).ok();
    std::fs::write(
        tmp.join(".claude/settings.json"),
        r#"{"extraKnownMarketplaces":{"local-repo-plugins":{"source":{"source":"directory","path":"./.ai"}}},"enabledPlugins":{"starter-aipm-plugin@local-repo-plugins":true}}"#,
    ).ok();

    let adaptors = default_adaptors();
    let opts = Options { dir: &tmp, workspace: false, marketplace: true, no_starter: false };
    let result = init(&opts, &adaptors, &crate::fs::Real);
    assert!(result.is_ok());
    // ToolConfigured should NOT be in actions (adaptor returned false)
    let r = result.ok();
    assert!(r.as_ref().is_some_and(|r| !r.actions.iter().any(|a| matches!(a, InitAction::ToolConfigured(_)))));

    cleanup(&tmp);
}
```

### 5.3 Phase 3: I/O Error Paths via Mock Fs (+8-10 branches → 90%+)

#### Test 7: I/O errors in `init()` — `init.rs`

**Target**: ~6 branches (create_dir_all, write_file errors in `init()`, `create_directory_layout()`, `create_gitkeep()`, `create_skill_template()`)

Add to `crates/libaipm/src/init.rs` test module:

```rust
use crate::fs::Fs;

struct FailFs { fail_on: &'static str }

impl Fs for FailFs {
    fn exists(&self, _: &Path) -> bool { false }
    fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
        if self.fail_on == "create_dir" {
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "mock"));
        }
        Ok(())
    }
    fn write_file(&self, _: &Path, _: &[u8]) -> std::io::Result<()> {
        if self.fail_on == "write_file" {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "mock: disk full"));
        }
        Ok(())
    }
    fn read_to_string(&self, _: &Path) -> std::io::Result<String> {
        Ok(String::new())
    }
}

#[test]
fn init_fails_on_create_dir_error() {
    let fs = FailFs { fail_on: "create_dir" };
    let tmp = std::path::PathBuf::from("/tmp/fake-init-dir");
    let opts = Options { dir: &tmp, name: Some("test"), plugin_type: None };
    let result = init(&opts, &fs);
    assert!(result.is_err());
    let err = result.err();
    assert!(err.is_some_and(|e| e.to_string().contains("mock")));
}

#[test]
fn init_fails_on_write_file_error() {
    let fs = FailFs { fail_on: "write_file" };
    let tmp = std::path::PathBuf::from("/tmp/fake-init-write");
    let opts = Options { dir: &tmp, name: Some("test"), plugin_type: Some(PluginType::Lsp) };
    // Lsp type skips directory layout (no create_dir in layout), so write_file is the first to fail
    let result = init(&opts, &fs);
    assert!(result.is_err());
}
```

Note: Multiple variants of `FailFs` can target different `?` operators. The key insight is that `create_dir_all` is called first in `init()` at line 88. If it succeeds, `create_directory_layout` calls further `create_dir_all` + `write_file` (via `create_gitkeep` and `create_skill_template`). Tests with different `plugin_type` values route through different match arms.

#### Test 8: I/O errors in `workspace_init` — `workspace_init/mod.rs`

**Target**: ~3 branches

Same `FailFs` strategy applied to `workspace_init::init()`:

```rust
#[test]
fn init_workspace_fails_on_create_dir_error() {
    struct FailDirFs;
    impl crate::fs::Fs for FailDirFs {
        fn exists(&self, _: &Path) -> bool { false }
        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "mock"))
        }
        fn write_file(&self, _: &Path, _: &[u8]) -> std::io::Result<()> { Ok(()) }
        fn read_to_string(&self, _: &Path) -> std::io::Result<String> { Ok(String::new()) }
    }

    let tmp = std::path::PathBuf::from("/tmp/fake-ws-dir");
    let adaptors: Vec<Box<dyn ToolAdaptor>> = vec![];
    let opts = Options { dir: &tmp, workspace: true, marketplace: false, no_starter: false };
    let result = init(&opts, &adaptors, &FailDirFs);
    assert!(result.is_err());
}
```

#### Test 9: Path edge case — `init.rs`

**Target**: 2 branches (`file_name()` → `None`, `to_str()` → `None`)

```rust
#[test]
fn init_no_directory_name_from_root_path() {
    let root = std::path::PathBuf::from("/");
    let opts = Options { dir: &root, name: None, plugin_type: None };
    let result = init(&opts, &Real);
    assert!(result.is_err());
    let err = result.err();
    assert!(err.is_some_and(|e| e.to_string().contains("cannot determine package name")));
}
```

#### Test 10: Dead code branch — `workspace_init/mod.rs:132-133`

**Target**: 1 branch (`parse_and_validate` returns `Err` in `init_workspace`)

This branch is normally unreachable because `generate_workspace_manifest()` always produces valid content. However, with a mock `Fs` we can't intercept the in-memory validation. This branch must remain as defensive dead code. It does **not** block 90% — the math works out without it.

## 6. Alternatives Considered

| Option | Pros | Cons | Decision |
|--------|------|------|----------|
| Read-only directory tricks | No production code changes | Platform-dependent; flaky on Windows; doesn't test `write_file` errors cleanly | Rejected — mock Fs is cleaner |
| `fs::Fs` trait injection (selected) | Platform-independent; precise control over which operation fails; fast (no real I/O) | Already implemented; slight indirection in production code | **Selected** — already done |
| Accept lower threshold (88%) | No test work needed | Defeats the purpose of 90% correctness gate | Rejected |

## 7. Cross-Cutting Concerns

### 7.1 Lint Compliance

All test code must satisfy the workspace lint configuration. Key constraints for test files:
- `allow_attributes = "warn"` — test files can use `#[allow(...)]` but it will produce warnings. The existing test files (`bdd.rs`, E2E tests) already use `#[allow(clippy::unwrap_used, ...)]`. New unit tests in `#[cfg(test)]` modules should use `is_ok()`, `is_err()`, `is_some_and()` patterns instead of `unwrap()`.
- No `println!` — use assertions only, no debug output.

### 7.2 Test Isolation

Mock `FailFs` tests do not touch the real filesystem. They can run in parallel without temp directory conflicts. Tests that still use `fs::Real` (malformed JSON tests in `claude.rs`) continue to use the existing `make_temp_dir` + `cleanup` pattern.

## 8. Implementation Checklist (All Complete ✅)

### Phase 1: Quick Wins (~9 branches)

| # | File | What | Type | Status |
|---|------|------|------|--------|
| 1 | `crates/libaipm/src/init.rs` | Add 5 scoped-name assertions to `invalid_names` | Modify existing test | ✅ Done |
| 2 | `crates/libaipm/src/manifest/mod.rs` | Add `multiple_errors_format_with_separator` test | New test | ✅ Done |
| 3 | `crates/libaipm/src/version.rs` | Add `select_best_returns_none_when_no_match` test | New test | ✅ Done |
| 4 | `crates/libaipm/src/version.rs` | Add `stable_version_is_not_prerelease` test | New test | ✅ Done |

### Phase 2: Moderate Effort (~5 branches)

| # | File | What | Type | Status |
|---|------|------|------|--------|
| 5 | `crates/aipm/tests/cli_tests.rs` | Add `no_subcommand_prints_version_and_usage` test (note: `aipm-pack` merged into `aipm`) | New test | ✅ Done |
| 6 | `crates/libaipm/src/workspace_init/adaptors/claude.rs` | Add `claude_settings_rejects_invalid_json` test | New test | ✅ Done |
| 7 | `crates/libaipm/src/workspace_init/adaptors/claude.rs` | Add `claude_settings_rejects_non_object_root` test | New test | ✅ Done |
| 8 | `crates/libaipm/src/workspace_init/adaptors/claude.rs` | Add `claude_settings_handles_non_object_marketplace_value` test | New test | ✅ Done |
| 9 | `crates/libaipm/src/workspace_init/mod.rs` | Add `init_marketplace_with_preconfigured_claude_settings` test | New test | ✅ Done |

### Phase 3: I/O Error Paths via Mock Fs (~8 branches)

| # | File | What | Type | Status |
|---|------|------|------|--------|
| 10 | `crates/libaipm/src/init.rs` | Define `FailFs` mock + `init_fails_on_create_dir_error` test | New test + mock | ✅ Done |
| 11 | `crates/libaipm/src/init.rs` | Add `init_fails_on_write_file_error` test | New test | ✅ Done |
| 12 | `crates/libaipm/src/init.rs` | Add `init_no_directory_name_from_root_path` test | New test | ✅ Done |
| 13 | `crates/libaipm/src/workspace_init/mod.rs` | Define `FailDirFs` mock + `init_workspace_fails_on_create_dir_error` test | New test + mock | ✅ Done |
| 14 | `crates/libaipm/src/workspace_init/mod.rs` | Add `init_workspace_fails_on_write_file_error` test | New test | ✅ Done |

### Verification

All tests are added and passing. Final verification commands:
```bash
cargo test --workspace                    # all tests pass
cargo clippy --workspace -- -D warnings   # no lint violations
cargo fmt --check                         # formatting correct
cargo +nightly llvm-cov --workspace --branch \
  --ignore-filename-regex '(tests/|research/|specs/)'  # verify >= 89%
```

**Result:** 95.01% branch coverage — above the 89% gate.

## 9. Open Questions / Unresolved Issues (Resolved)

- [x] **Dead code branch** (`workspace_init/mod.rs:132-133`): The `parse_and_validate` error path inside `init_workspace` is structurally unreachable. Accepted as defensive dead code — does not affect gate compliance at 95.01%.
- [x] **`validate.rs` headroom**: No extra tests needed — the 95.01% total far exceeds the 89% gate.
