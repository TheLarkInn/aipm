---
date: 2026-04-02 23:53:18 UTC
researcher: Claude Code
git_commit: 4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56
branch: main
repository: aipm
topic: "Implement verbosity levels across aipm (issue #189)"
tags: [research, logging, verbosity, tracing, cli, agentic-logging, diagnostics]
status: complete
last_updated: 2026-04-02
last_updated_by: Claude Code
---

# Research: Implement Verbosity Levels (Issue #189)

## Research Question

How should aipm implement verbosity levels (debug, info, warning, error) across the entire tool? What ecosystem crates fit best? How can logs be "agentic-first" so that LLMs can react to warnings/errors by referencing living documentation?

**Issue**: https://github.com/TheLarkInn/aipm/issues/189

## Summary

The aipm project **already has `tracing` and `tracing-subscriber` as workspace dependencies** and has ~30+ `tracing::info!`/`tracing::debug!`/`tracing::warn!` call sites in `libaipm` -- but **no subscriber is ever initialized**, so all tracing output is silently discarded at runtime. The project's strict `print_stdout = "deny"` and `print_stderr = "deny"` lints mean `tracing` (which writes through `std::io::Write`, not `println!`) is the natural fit.

The recommended approach is to activate the existing tracing infrastructure with a layered subscriber: stderr output controlled by a `-v`/`-q` verbosity flag, plus an always-on file log at `/tmp/aipm-*.log` for post-mortem debugging. Adding structured JSON output (`--log-format=json`) and documentation-referencing fields enables "agentic-first" logging where LLM agents can parse and act on diagnostics.

---

## Detailed Findings

### 1. Current State of Output and Error Handling

#### 1.1 Existing Tracing Infrastructure (Unused)

**Dependencies declared** in workspace [`Cargo.toml:72-73`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/Cargo.toml#L72-L73):

```toml
tracing = "0.1"
tracing-subscriber = "0.3"
```

Used by all three crates: `crates/aipm`, `crates/aipm-pack`, `crates/libaipm`.

**~30+ tracing call sites exist** in `libaipm` production code, all currently silent:

| File | Lines | Level | Message |
|------|-------|-------|---------|
| [`installer/pipeline.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/installer/pipeline.rs) | 78, 86-89, 108, 196-201, 226-232, 254-258, 269, 273, 291-294, 403-408, 479-483, 741, 762-765, 842, 861, 868, 898, 903-907, 944-948 | info/debug/warn | Install and update operations |
| [`linker/pipeline.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/linker/pipeline.rs) | 34, 39-43, 49, 60, 67 | info/debug | Link and unlink operations |
| [`linker/security.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/linker/security.rs) | 40-43, 47-52 | debug/warn | Lifecycle script security checks |
| [`store/mod.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/store/mod.rs) | 175-178 | warn | Hard-link fallback to copy |
| [`workspace/mod.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/workspace/mod.rs) | 84-87, 122 | warn/info | Workspace discovery |
| [`registry/git.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/registry/git.rs) | 83, 92, 154 | info | Registry operations |
| [`main.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm/src/main.rs) | 327 | info | Workspace root found |

**No subscriber initialization exists** anywhere in the codebase. The string `tracing_subscriber` does not appear in any `.rs` file.

#### 1.2 Current User-Facing Output

All output uses `let _ = writeln!(stdout/stderr, ...)`. Key locations in [`crates/aipm/src/main.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm/src/main.rs):

| Lines | Stream | Category | Message |
|-------|--------|----------|---------|
| 311 | stdout | Action result | Init messages ("Initialized workspace in...") |
| 350-354 | stdout | Action result | "Installed N package(s), N up-to-date, N removed" |
| 378-382 | stdout | Action result | "Updated N package(s)..." |
| 427, 450 | stdout | Action result | "Linked '...'" / "Unlinked '...'" |
| 464-492 | stdout | Info listing | List command output |
| 640-717 | stdout | Action result | Migration action reporting |
| 647-650 | stdout | Warning | "Warning: renamed '...' -> '...' (reason)" |
| 675-678 | **stderr** | Warning | "Warning: external file ... not moved" |
| 742-744 | **stderr** | Warning | "warning: --registry is not yet supported" |
| 761-762 | stdout | Info | Version output |
| 771 | **stderr** | Error | "error: {e}" (final error handler) |

[`crates/aipm-pack/src/main.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm-pack/src/main.rs):

| Lines | Stream | Message |
|-------|--------|---------|
| 63 | stdout | "Initialized plugin package in {dir}" |
| 68-69 | stdout | Version info |
| 78 | **stderr** | "error: {e}" |

The lint reporter ([`libaipm/src/lint/reporter.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/lint/reporter.rs)) is the most structured output system, with a `Reporter` trait supporting text and JSON formatters, severity levels, and diagnostic structs. This is a potential model for broader structured output.

#### 1.3 Silent Fallbacks (Places That Need Logging)

These locations silently try one approach and fall back to another with zero diagnostic output:

| File | Lines | What Happens |
|------|-------|--------------|
| [`main.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm/src/main.rs#L205-L215) | 205-215 | `resolve_plugins_dir` falls back from manifest to hardcoded `.ai` |
| [`main.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm/src/main.rs#L552-L619) | 552-619 | `load_lint_config` silently uses `Config::default()` if manifest is unparseable |
| [`workspace/mod.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/workspace/mod.rs#L32-L49) | 32-49 | `find_workspace_root` silently continues walking up if manifest fails to parse |
| [`installer/pipeline.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/installer/pipeline.rs#L638-L646) | 638-646 | `build_pins` silently skips packages with unparseable versions |
| [`lint/mod.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/lint/mod.rs#L22-L31) | 22-31 | `is_ignored` silently skips invalid glob patterns |
| [`lint/rules/scan.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/lint/rules/scan.rs) | 32-131 | `scan_skills`, `scan_agents`, `scan_hook_files` silently skip unreadable directories/files |
| [`store/mod.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/store/mod.rs#L160-L186) | 160-186 | `link_to` falls back from hard-link to copy (has invisible `tracing::warn!`) |
| [`migrate/copilot_extension_detector.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/migrate/copilot_extension_detector.rs#L60-L79) | 60-79 | `try_read_config` tries 9 candidate filenames silently |
| [`migrate/emitter.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/migrate/emitter.rs#L618-L626) | 618-626 | Files that fail to read during migration are silently skipped |
| [`migrate/cleanup.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/migrate/cleanup.rs#L64-L71) | 64-71 | Directories that fail to read during pruning are silently skipped |
| [`fs.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/fs.rs#L201-L205) | 201, 205 | Atomic write backup cleanup on Windows silently discards errors |

#### 1.4 Error Types Catalog

The project has 14 `thiserror`-derived error enums across `libaipm`:

| Module | File | Key Variants |
|--------|------|-------------|
| `version::Error` | [`version.rs:17-31`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/version.rs#L17-L31) | InvalidVersion, InvalidRequirement |
| `manifest::Error` | [`manifest/error.rs:10-84`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/manifest/error.rs#L10-L84) | Parse, MissingField, InvalidName, Multiple(Vec) |
| `init::Error` | [`init.rs:23-49`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/init.rs#L23-L49) | AlreadyInitialized, InvalidName, NoDirectoryName |
| `workspace_init::Error` | [`workspace_init/mod.rs:85-105`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/workspace_init/mod.rs#L85-L105) | WorkspaceAlreadyInitialized, MarketplaceAlreadyExists |
| `workspace::Error` | [`workspace/error.rs:7-16`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/workspace/error.rs#L7-L16) | Discovery, NoWorkspaceRoot |
| `resolver::Error` | [`resolver/error.rs:4-32`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/resolver/error.rs#L4-L32) | NoMatch, Conflict, Registry |
| `installer::Error` | [`installer/error.rs:4-27`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/installer/error.rs#L4-L27) | Io, Manifest, LockfileDrift, Resolution |
| `linker::Error` | [`linker/error.rs:6-30`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/linker/error.rs#L6-L30) | Io, TargetExists, SourceMissing |
| `registry::Error` | [`registry/error.rs:4-48`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/registry/error.rs#L4-L48) | PackageNotFound, VersionNotFound, ChecksumMismatch |
| `lockfile::Error` | [`lockfile/error.rs:6-39`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/lockfile/error.rs#L6-L39) | Io, Parse, UnsupportedVersion, Drift |
| `store::Error` | [`store/error.rs:6-30`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/store/error.rs#L6-L30) | Io, InvalidHash, NotFound |
| `migrate::Error` | [`migrate/mod.rs:247-295`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/migrate/mod.rs#L247-L295) | MarketplaceNotFound, SourceNotFound, UnsupportedSource |
| `lint::Error` | [`lint/mod.rs:157-184`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/lint/mod.rs#L157-L184) | Io, JsonParse, FrontmatterParse, DiscoveryFailed |
| `discovery::Error` | [`discovery.rs:9-14`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/discovery.rs#L9-L14) | WalkFailed |

#### 1.5 Exit Code Handling

Both CLIs use identical binary exit code logic -- `ExitCode::SUCCESS` (0) or `ExitCode::FAILURE` (1):

```rust
fn main() -> std::process::ExitCode {
    if let Err(e) = run() {
        let mut stderr = std::io::stderr();
        let _ = writeln!(stderr, "error: {e}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}
```

There is no distinction between different failure modes. All errors display as `"error: {display_string}"` to stderr with exit code 1.

#### 1.6 CLI Argument Structure

[`crates/aipm/src/main.rs:14-161`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm/src/main.rs#L14-L161) -- Uses `clap::Parser`. **No verbosity flag, no `--quiet` flag, no logging-level argument exists.** Subcommands: Init, Install, Update, Link, Unlink, List, Lint, Migrate.

[`crates/aipm-pack/src/main.rs:15-41`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm-pack/src/main.rs#L15-L41) -- Uses `clap::Parser`. Subcommand: Init. No verbosity flags.

---

### 2. Rust Logging Ecosystem

#### 2.1 `log` Crate + Backends

The [`log`](https://docs.rs/log) crate is a lightweight logging facade. Backends include:

- **[`env_logger`](https://docs.rs/env_logger)**: Writes to **stderr by default** via `std::io::Write`. Does **not** use `println!` -- compatible with aipm's deny lints. Configured via `RUST_LOG` env var.
- **[`simplelog`](https://docs.rs/simplelog)**: Supports simultaneous file + terminal output.
- **[`fern`](https://docs.rs/fern)**: Highly configurable routing to multiple outputs.

#### 2.2 `tracing` Ecosystem (Already in Use)

[`tracing`](https://docs.rs/tracing) (Tokio team) is a superset of `log` with structured spans and events:

- **[`tracing-subscriber`](https://docs.rs/tracing-subscriber)**: Uses `std::io::Write` internally -- **not** `println!`/`eprintln!`. Compatible with deny lints.
- **[`tracing-appender`](https://docs.rs/tracing-appender)**: Non-blocking file appender with rolling file support.
- **[`tracing-log`](https://crates.io/crates/tracing-log)**: Bridges `log` crate records into tracing subscribers.

#### 2.3 Comparison

| Criterion | `log` + `env_logger` | `tracing` + `tracing-subscriber` |
|---|---|---|
| Simplicity | Simpler API, fewer deps | More setup, richer features |
| Structured data | No native support | Built-in key-value fields |
| JSON output | Custom formatting needed | Built-in `.json()` formatter |
| Already in aipm? | No | **Yes -- 30+ call sites exist** |
| Lint compatibility | Yes (writes to `io::Write`) | Yes (writes to `io::Write`) |

**Conclusion**: `tracing` is the clear choice -- it's already a dependency with instrumentation in place.

#### 2.4 `clap-verbosity-flag` Crate

[`clap-verbosity-flag`](https://github.com/clap-rs/clap-verbosity-flag) provides a drop-in clap derive struct:

```rust
use clap_verbosity_flag::{Verbosity, WarnLevel};

#[derive(Debug, Parser)]
struct Cli {
    #[command(flatten)]
    verbose: Verbosity<WarnLevel>,
}
```

Mapping (with `WarnLevel` default):

| Flags | Level |
|---|---|
| `-qq` | Off |
| `-q` | Error |
| (none) | **Warn** |
| `-v` | Info |
| `-vv` | Debug |
| `-vvv` | Trace |

Supports `tracing` via `--no-default-features --features tracing`.

**Sources**: [clap-verbosity-flag docs](https://docs.rs/clap-verbosity-flag), [GitHub](https://github.com/clap-rs/clap-verbosity-flag)

---

### 3. CLI Verbosity Patterns in Popular Tools

| Tool | Default | Quiet | Verbose | Env Var |
|---|---|---|---|---|
| **Cargo** | Warn-ish (errors + status) | `-q` | `-v`, `-vv` | `CARGO_LOG=debug\|trace` |
| **ripgrep** | Off (output only) | N/A | `--debug` | `RUST_LOG` |
| **npm** | Warn | `--silent` | `--verbose` | N/A |
| **pip** | Warn | `-q` | `-v`, `-vv`, `-vvv` | N/A |

The [Command Line Interface Guidelines](https://clig.dev/) recommend:
- **Default**: Show operation results without log-level labels
- **`-q/--quiet`**: Machine-friendly silence
- **`-v/--verbose`**: Developer diagnostics

**Sources**: [Cargo Configuration](https://doc.rust-lang.org/cargo/reference/config.html), [CLI Guidelines](https://clig.dev/), [ripgrep discussion](https://github.com/BurntSushi/ripgrep/discussions/1657)

---

### 4. Agentic-First Logging

#### 4.1 Structured JSON Output

`tracing-subscriber` has a built-in JSON formatter (feature `"json"`):

```rust
tracing_subscriber::fmt()
    .json()
    .with_target(true)
    .with_file(true)
    .with_line_number(true)
    .init();
```

Produces:
```json
{"timestamp":"2026-04-02T12:00:00Z","level":"WARN","target":"libaipm::installer","message":"hard-link failed, falling back to copy","source_path":"/home/user/.ai/store/abc123"}
```

#### 4.2 Documentation-Referencing Log Fields

Using tracing's structured fields, logs can embed documentation URLs:

```rust
tracing::warn!(
    code = "AIPM-W001",
    doc = "https://aipm.dev/docs/errors/W001",
    "manifest missing required field 'version'"
);
```

In JSON mode:
```json
{"level":"WARN","code":"AIPM-W001","doc":"https://aipm.dev/docs/errors/W001","message":"manifest missing required field 'version'"}
```

An LLM agent could parse the `doc` field and fetch the URL for context-aware remediation.

#### 4.3 The `llms.txt` Convention

The [`llms.txt`](https://llmstxt.org/) specification provides LLM-friendly documentation at a site's root. Over 844,000 websites have adopted it (Anthropic, Cloudflare, Stripe). aipm could:

1. Ship error documentation following this convention
2. Reference those docs in structured log output
3. Enable LLM agents to self-serve remediation by following `doc` fields

#### 4.4 Precedent: Rust Compiler Diagnostics

The Rust compiler uses structured diagnostic codes (E0308, etc.) with `--explain E0308` and stable URLs (`https://doc.rust-lang.org/error_codes/E0308.html`). aipm's lint system already has a `rule_id` field in diagnostics -- this pattern extends naturally to logging.

**Sources**: [llms.txt](https://llmstxt.org/), [Pre-RFC for LLM text in rustdoc](https://internals.rust-lang.org/t/pre-rfc-add-llm-text-version-to-rustdoc/22090)

---

### 5. Log-to-File Patterns

#### 5.1 Multi-Output with tracing-subscriber Layers

```rust
use tracing_subscriber::{filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt, Layer};
use tracing_appender::rolling::{RollingFileAppender, Rotation};

fn setup_logging(verbosity: LevelFilter) {
    // Layer 1: stderr at user-requested verbosity
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(verbosity);

    // Layer 2: file at DEBUG (always captures everything)
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("aipm")
        .filename_suffix("log")
        .build("/tmp")
        .expect("failed to create log appender");

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false)
        .with_filter(LevelFilter::DEBUG);

    tracing_subscriber::registry()
        .with(stderr_layer)
        .with(file_layer)
        .init();
}
```

Key points:
- Each layer has its own `LevelFilter`
- `.with_ansi(false)` prevents ANSI escape codes in file output
- The issue specifies "always writes to /tmp log by default" -- this satisfies that requirement

#### 5.2 Required Feature Flags

```toml
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-appender = "0.2"
```

**Sources**: [tracing-appender docs](https://docs.rs/tracing-appender), [tracing multi-file example](https://github.com/tokio-rs/tracing/blob/master/examples/examples/appender-multifile.rs)

---

### 6. Default Verbosity Level Analysis

#### 6.1 What Most Tools Default To

Most CLI tools default to **Warn** level:
- Linux kernel: `KERN_WARNING` (level 4)
- Cargo: Shows errors + status messages (warn-equivalent)
- npm/pip: Warn

#### 6.2 What Belongs at Each Level

| Level | Purpose | aipm Examples |
|---|---|---|
| **ERROR** | Unrecoverable failures, exit code 1 | "failed to resolve package 'foo': registry unreachable" |
| **WARN** | Recoverable issues, exit code 0 | "hard-link failed, falling back to copy", "invalid glob pattern in ignore list" |
| **INFO** | High-level operation progress | "installing package foo@1.2.0", "3 packages linked" |
| **DEBUG** | Implementation details | "resolving dependency tree", "checking hash for abc123" |
| **TRACE** | Wire-level detail | "HTTP GET registry.aipm.dev/...", "comparing semver ^1.2 vs 1.3.0" |

#### 6.3 Mapping to Issue Requirements

From the issue:

| Issue Requirement | Verbosity Level | Exit Code |
|---|---|---|
| "Warnings exit code 0" | WARN | 0 |
| "Errors exit code 1" | ERROR | 1 |
| "Any time a file needs to be resolved" | DEBUG | N/A |
| "Any time an error condition is swallowed" | WARN | 0 |
| "Any time a fallback branch to secondary logic" | INFO or DEBUG | N/A |
| "Every action being performed" | INFO | N/A |

**Sources**: [Log Levels Explained (Better Stack)](https://betterstack.com/community/guides/logging/log-levels-explained/), [CLI Guidelines](https://clig.dev/)

---

## Code References

### Existing tracing infrastructure
- [`Cargo.toml:72-73`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/Cargo.toml#L72-L73) -- workspace tracing dependencies
- [`crates/aipm/Cargo.toml:19-20`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm/Cargo.toml#L19-L20) -- aipm tracing deps
- [`crates/libaipm/src/installer/pipeline.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/installer/pipeline.rs) -- heaviest tracing usage (~20 call sites)
- [`crates/libaipm/src/linker/pipeline.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/linker/pipeline.rs) -- linker tracing (~5 call sites)

### CLI entry points (need subscriber init)
- [`crates/aipm/src/main.rs:768-775`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm/src/main.rs#L768-L775) -- main() exit code handler
- [`crates/aipm-pack/src/main.rs:75-82`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm-pack/src/main.rs#L75-L82) -- aipm-pack main()

### Silent fallbacks (need tracing instrumentation)
- [`main.rs:205-215`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm/src/main.rs#L205-L215) -- plugins dir resolution fallback
- [`main.rs:552-619`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/aipm/src/main.rs#L552-L619) -- lint config loading fallback
- [`workspace/mod.rs:32-49`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/workspace/mod.rs#L32-L49) -- workspace root discovery fallback
- [`installer/pipeline.rs:638-646`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/installer/pipeline.rs#L638-L646) -- version parse skip
- [`lint/mod.rs:22-31`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/lint/mod.rs#L22-L31) -- glob pattern skip
- [`lint/rules/scan.rs:32-131`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/lint/rules/scan.rs#L32-L131) -- scan functions with silent skips
- [`migrate/copilot_extension_detector.rs:60-79`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/migrate/copilot_extension_detector.rs#L60-L79) -- config file fallback chain
- [`migrate/emitter.rs:618-626`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/migrate/emitter.rs#L618-L626) -- file read skip
- [`migrate/cleanup.rs:64-71`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/migrate/cleanup.rs#L64-L71) -- directory read skip

### Lint reporter (model for structured output)
- [`libaipm/src/lint/reporter.rs`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/crates/libaipm/src/lint/reporter.rs) -- text + JSON reporter trait

---

## Architecture Documentation

### Current Output Architecture

```
User-facing output:
  main.rs -> writeln!(stdout, ...) -- action results, listings
  main.rs -> writeln!(stderr, ...) -- errors, warnings

Library diagnostics:
  libaipm -> tracing::info!/warn!/debug! -- CURRENTLY SILENT (no subscriber)
  libaipm/lint/reporter.rs -> write!(&mut dyn Write, ...) -- structured lint output

Error propagation:
  libaipm functions -> Result<T, module::Error>
  CLI run() -> Box<dyn std::error::Error>
  main() -> ExitCode::SUCCESS or ExitCode::FAILURE
```

### Proposed Architecture (from ecosystem research)

```
CLI args:
  -v/-vv/-vvv  -> increase verbosity (default: Warn)
  -q/-qq       -> decrease verbosity
  --log-format -> text (default) or json (agentic)

Subscriber layers:
  Layer 1: stderr -- filtered by -v/-q flags
  Layer 2: /tmp/aipm-YYYY-MM-DD.log -- always at DEBUG level

Output channels:
  stdout -- action results (unchanged: "Installed N packages...")
  stderr -- tracing subscriber output (warnings, errors, debug)
  /tmp   -- full debug log for post-mortem
```

---

## Historical Context (from research/)

- [`research/docs/2026-03-31-110-aipm-lint-architecture-research.md`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/research/docs/2026-03-31-110-aipm-lint-architecture-research.md) -- Documents existing stderr/stdout patterns, error handling via `writeln!(stderr, ...)`, and the lint diagnostic architecture
- [`research/docs/2026-04-02-aipm-lint-configuration-research.md`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/research/docs/2026-04-02-aipm-lint-configuration-research.md) -- Covers `--format` CLI flag for text/json output, diagnostic severity levels, and exit codes
- [`research/docs/2026-03-31-cli-binary-frontmatter-hook-analysis.md`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/research/docs/2026-03-31-cli-binary-frontmatter-hook-analysis.md) -- Agentic hooks and hook event types analysis
- [`research/docs/2026-03-09-cargo-core-principles.md`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/research/docs/2026-03-09-cargo-core-principles.md) -- Cargo's structured metadata and build script communication patterns
- [`specs/2026-03-31-aipm-lint-command.md`](https://github.com/TheLarkInn/aipm/blob/4216c4b98e2c8ead45ae4e1e24855ba8bd67bf56/specs/2026-03-31-aipm-lint-command.md) -- Lint command spec with `Diagnostic` struct, severity enum, and `Reporter` trait (model for structured output)

No prior research specifically about logging/verbosity exists in the research directory.

---

## Related Research

- `research/docs/2026-03-31-110-aipm-lint-architecture-research.md` -- Lint diagnostic output patterns
- `research/docs/2026-04-02-aipm-lint-configuration-research.md` -- Lint configuration and format flags
- `research/docs/2026-03-31-cli-binary-frontmatter-hook-analysis.md` -- Hook event types
- `research/docs/2026-03-09-cargo-core-principles.md` -- Cargo CLI architecture

---

## Open Questions

1. **Should `writeln!(stdout, ...)` messages (action results) also go through tracing, or remain as direct writes?** The current `writeln!` messages are user-facing action results ("Installed 3 packages"). These are semantically different from diagnostic logs. Options:
   - Keep them as direct `writeln!` (clean separation of "output" vs "logs")
   - Route through `tracing::info!` (unified but may feel noisy in `-v` mode)

2. **What default level?** The issue asks this explicitly. Industry convention and the analysis above suggest **Warn** as default, matching cargo/npm/pip behavior.

3. **Error code namespace**: What prefix for agentic error codes? The issue mentions "living documentation" -- something like `AIPM-E001`/`AIPM-W001` with corresponding doc URLs would enable this. What URL base? (e.g., `https://aipm.dev/docs/errors/`)

4. **Should the `/tmp` log use JSON format?** JSON logs are more parseable by LLM agents and log aggregators. Text logs are more human-readable for manual debugging. Could offer both, or default to JSON in `/tmp`.

5. **Log rotation policy**: How many days of logs to retain in `/tmp`? `tracing-appender` supports daily rotation with configurable max files.

6. **Should `aipm-pack` get the same verbosity infrastructure?** It has minimal tracing currently but would benefit from the same `-v`/`-q` flags for consistency.
