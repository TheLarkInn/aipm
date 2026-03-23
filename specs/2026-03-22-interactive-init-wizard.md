# Interactive Init Wizard with `inquire`

| Document Metadata      | Details                                                                                     |
| ---------------------- | ------------------------------------------------------------------------------------------- |
| Author(s)              | selarkin                                                                                    |
| Status                 | Draft (WIP)                                                                                 |
| Team / Owner           | AI Dev Tooling                                                                              |
| Created / Last Updated | 2026-03-22                                                                                  |
| Research               | [research/docs/2026-03-22-rust-interactive-cli-prompts.md](../research/docs/2026-03-22-rust-interactive-cli-prompts.md) |

## 1. Executive Summary

This spec adds interactive wizards to `aipm init` and `aipm-pack init` using the [`inquire`](https://crates.io/crates/inquire) crate with minimal features (no `zeroize`, `chrono`, `tempfile`, or `fuzzy-matcher`). Running either command **without** a `--yes` / `-y` flag in a terminal launches a step-by-step wizard with placeholder defaults, validation, and styled output. The `-y` flag preserves today's non-interactive behavior exactly. When stdin is not a TTY (CI, pipes), defaults are used automatically. All interactive logic lives in thin presentation modules in each CLI crate — the existing `libaipm` init functions remain unchanged and untouched by this work.

## 2. Context and Motivation

### 2.1 Current State

Both `aipm init` and `aipm-pack init` are fully non-interactive. All configuration is via CLI flags with hardcoded defaults ([research §1–2](../research/docs/2026-03-22-rust-interactive-cli-prompts.md)):

```
aipm init [--workspace] [--marketplace] [--no-starter] [DIR]
aipm-pack init [--name NAME] [--type TYPE] [DIR]
```

If a user runs `aipm init` with no flags, they get marketplace-only mode with `local-repo-plugins` as the marketplace name, `starter-aipm-plugin` as the starter, and Claude Code as the configured tool — with no opportunity to change any of these values.

**Current defaults (workspace init):**
- Setup mode: marketplace only
- Marketplace name: `"local-repo-plugins"`
- Starter plugin name: `"starter-aipm-plugin"`
- Include starter: yes
- Tool adaptor: Claude Code

**Current defaults (package init):**
- Package name: directory basename
- Plugin type: `composite`
- Version: `"0.1.0"`
- Edition: `"2024"`

### 2.2 The Problem

| Problem | Impact |
|---------|--------|
| No discoverability of init options | Users must read `--help` to know what flags exist |
| No way to customize defaults interactively | Marketplace name, starter name, and tool selection are take-it-or-leave-it |
| No description field in package init | `aipm-pack init` generates a manifest with no `description` |
| Unfamiliar feel for JS/Node developers | Modern CLI tools (create-next-app, create-svelte, npm init) all use interactive wizards |

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [ ] Add `--yes` / `-y` flag to both `aipm init` and `aipm-pack init`
- [ ] When `-y` is passed OR stdin is not a TTY: use all defaults (today's behavior, no prompts)
- [ ] When run interactively (TTY, no `-y`): launch wizard with placeholder defaults
- [ ] Wizard prompts use `inquire` crate with minimal feature set (no zeroize, no chrono, no tempfile)
- [ ] All existing CLI flags continue to work as overrides (e.g., `aipm init --workspace` skips the setup-mode prompt)
- [ ] Pressing Enter on any prompt accepts the placeholder default
- [ ] Invalid input shows inline validation errors without crashing
- [ ] Existing `libaipm` init functions (`workspace_init::init`, `init::init`) are not modified
- [ ] All existing tests continue to pass unchanged

### 3.2 Non-Goals (Out of Scope)

- [ ] We will NOT add a progress spinner or progress bar (inquire doesn't include one; if needed later, add `indicatif` separately)
- [ ] We will NOT add cliclack-style vertical sidebar framing (would require `cliclack` and its unconditional `zeroize`/`indicatif` deps)
- [ ] We will NOT add password, date-select, or editor prompts
- [ ] We will NOT modify `libaipm` library code — wizards live in the CLI crates only
- [ ] We will NOT add interactive prompts to any other commands (install, publish, etc.) in this work
- [ ] We will NOT directly test the interactive prompts in CI (they require a TTY) — only the non-interactive paths are tested

## 4. Proposed Solution (High-Level Design)

### 4.1 Dependency

Add to workspace `Cargo.toml`:

```toml
[workspace.dependencies]
inquire = { version = "0.9", default-features = false, features = ["crossterm", "one-liners"] }
```

This pulls in only `crossterm` (terminal backend) and `one-liners` (convenience functions). Excluded:
- `fuzzy` → `fuzzy-matcher` crate (not needed for small option lists)
- `date` → `chrono` crate
- `editor` → `tempfile` crate
- `macros` → validator macros (we use closure validators instead)

Add to `crates/aipm/Cargo.toml` and `crates/aipm-pack/Cargo.toml`:

```toml
[dependencies]
inquire = { workspace = true }
```

`libaipm` does **not** depend on `inquire`. The library remains headless.

### 4.2 Architecture

```
┌─────────────────────────────────────┐
│  CLI crate (aipm / aipm-pack)       │
│                                     │
│  main.rs                            │
│    ├── Cli / Commands (clap)        │
│    ├── run() → decides interactive  │
│    └── calls wizard or defaults     │
│                                     │
│  wizard.rs  (NEW)                   │
│    ├── workspace_wizard() → Options │  ← inquire prompts
│    └── package_wizard() → Options   │  ← inquire prompts
│                                     │
├─────────────────────────────────────┤
│  libaipm (unchanged)                │
│    ├── workspace_init::init(opts)   │  ← receives Options, does the work
│    └── init::init(opts)             │  ← receives Options, does the work
└─────────────────────────────────────┘
```

The wizard module is a **thin presentation layer**. It collects user input via `inquire` prompts, constructs the same `Options` struct that the CLI currently builds from flags, and passes it to the existing `libaipm` init functions. No business logic in the wizard.

### 4.3 TTY Detection

```rust
use std::io::IsTerminal;

let interactive = !yes && std::io::stdin().is_terminal();
```

`IsTerminal` is stable since Rust 1.70. No additional crate needed.

### 4.4 Flag Override Logic

When a user provides explicit CLI flags alongside interactive mode, those flags **pre-fill** the wizard and skip the corresponding prompt:

| Scenario | Behavior |
|----------|----------|
| `aipm init` (no flags, TTY) | Full wizard, all prompts shown |
| `aipm init -y` | All defaults, no prompts (today's behavior) |
| `aipm init --workspace` (TTY) | Setup-mode prompt skipped (workspace selected), remaining prompts shown |
| `aipm init --workspace --marketplace` (TTY) | Setup-mode prompt skipped (both selected), remaining prompts shown |
| `aipm init` (piped stdin) | All defaults, no prompts |
| `aipm-pack init --name foo` (TTY) | Name prompt skipped (pre-filled), remaining prompts shown |
| `aipm-pack init --type skill` (TTY) | Type prompt skipped (pre-filled), remaining prompts shown |
| `aipm-pack init -y` | All defaults, no prompts (today's behavior) |

## 5. Detailed Design

### 5.1 CLI Changes — `aipm init`

**`crates/aipm/src/main.rs`** — Add `-y` flag to `Commands::Init`:

```rust
#[derive(Subcommand)]
enum Commands {
    /// Initialize a workspace for AI plugin management.
    Init {
        /// Skip interactive prompts, use all defaults.
        #[arg(short = 'y', long)]
        yes: bool,

        /// Generate a workspace manifest (aipm.toml with [workspace] section).
        #[arg(long)]
        workspace: bool,

        /// Generate a .ai/ local marketplace with tool settings.
        #[arg(long)]
        marketplace: bool,

        /// Skip the starter plugin (create bare .ai/ directory only).
        #[arg(long)]
        no_starter: bool,

        /// Directory to initialize (defaults to current directory).
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
}
```

**`run()` flow:**

```rust
Some(Commands::Init { yes, workspace, marketplace, no_starter, dir }) => {
    let dir = if dir.as_os_str() == "." { std::env::current_dir()? } else { dir };
    let interactive = !yes && std::io::stdin().is_terminal();

    let (do_workspace, do_marketplace, do_no_starter) = if interactive {
        wizard::workspace_wizard(workspace, marketplace, no_starter)?
    } else {
        // Today's defaulting logic
        let (w, m) = if !workspace && !marketplace {
            (false, true)
        } else {
            (workspace, marketplace)
        };
        (w, m, no_starter)
    };

    let adaptors = libaipm::workspace_init::adaptors::defaults();
    let opts = libaipm::workspace_init::Options {
        dir: &dir,
        workspace: do_workspace,
        marketplace: do_marketplace,
        no_starter: do_no_starter,
    };
    let result = libaipm::workspace_init::init(&opts, &adaptors, &libaipm::fs::Real)?;
    // ... print actions (unchanged) ...
}
```

### 5.2 CLI Changes — `aipm-pack init`

**`crates/aipm-pack/src/main.rs`** — Add `-y` flag to `Commands::Init`:

```rust
Init {
    /// Skip interactive prompts, use all defaults.
    #[arg(short = 'y', long)]
    yes: bool,

    /// Package name (defaults to directory name).
    #[arg(long)]
    name: Option<String>,

    /// Plugin type: skill, agent, mcp, hook, lsp, composite.
    #[arg(long, rename_all = "kebab-case", value_name = "TYPE")]
    r#type: Option<String>,

    /// Directory to initialize (defaults to current directory).
    #[arg(default_value = ".")]
    dir: PathBuf,
}
```

**`run()` flow:**

```rust
Some(Commands::Init { yes, name, r#type, dir }) => {
    let dir = if dir.as_os_str() == "." { std::env::current_dir()? } else { dir };
    let interactive = !yes && std::io::stdin().is_terminal();

    let plugin_type = r#type.as_deref().map(str::parse::<PluginType>).transpose()?;

    let (final_name, final_type) = if interactive {
        wizard::package_wizard(&dir, name.as_deref(), plugin_type)?
    } else {
        (name, plugin_type)
    };

    let opts = Options { dir: &dir, name: final_name.as_deref(), plugin_type: final_type };
    init::init(&opts, &libaipm::fs::Real)?;
    // ... print success (unchanged) ...
}
```

### 5.3 Wizard Module — `aipm init`

**New file: `crates/aipm/src/wizard.rs`**

```rust
use inquire::{Select, Confirm};

/// Workspace init wizard. Returns (workspace, marketplace, no_starter).
///
/// Pre-filled flags skip their corresponding prompt.
pub fn workspace_wizard(
    flag_workspace: bool,
    flag_marketplace: bool,
    flag_no_starter: bool,
) -> Result<(bool, bool, bool), Box<dyn std::error::Error>> {
    // --- Step 1: Setup mode (skip if any flag was explicitly set) ---
    let (do_workspace, do_marketplace) = if flag_workspace || flag_marketplace {
        (flag_workspace, flag_marketplace)
    } else {
        let options = vec![
            "Marketplace only (recommended)",
            "Workspace manifest only",
            "Both workspace + marketplace",
        ];
        let choice = Select::new("What would you like to set up?", options)
            .with_help_message("Use arrow keys, Enter to select")
            .prompt()?;

        match choice {
            "Marketplace only (recommended)" => (false, true),
            "Workspace manifest only" => (true, false),
            _ => (true, true),
        }
    };

    // --- Step 2: Include starter plugin? (skip if --no-starter was set) ---
    let no_starter = if flag_no_starter || !do_marketplace {
        flag_no_starter
    } else {
        let include = Confirm::new("Include starter plugin?")
            .with_default(true)
            .with_help_message("Adds scaffold-plugin skill, marketplace-scanner agent, and logging hook")
            .prompt()?;
        !include
    };

    Ok((do_workspace, do_marketplace, no_starter))
}
```

> **Note on marketplace name and starter plugin name:** These are currently not configurable in the `Options` struct — they are hardcoded in `workspace_init::scaffold_marketplace()`. Adding prompts for these would require expanding the `Options` struct in `libaipm`, which is out of scope for this initial version. If configurability is desired later, it can be added as a follow-up by extending `Options` with optional `marketplace_name` and `starter_name` fields.

### 5.4 Wizard Module — `aipm-pack init`

**New file: `crates/aipm-pack/src/wizard.rs`**

```rust
use inquire::{Text, Select};
use libaipm::manifest::types::PluginType;

/// Package init wizard. Returns (name, plugin_type).
///
/// Pre-filled flags skip their corresponding prompt.
pub fn package_wizard(
    dir: &std::path::Path,
    flag_name: Option<&str>,
    flag_type: Option<PluginType>,
) -> Result<(Option<String>, Option<PluginType>), Box<dyn std::error::Error>> {
    // --- Step 1: Package name ---
    let name = match flag_name {
        Some(n) => Some(n.to_string()),
        None => {
            let default_name = dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("my-plugin");

            let input = Text::new("Package name:")
                .with_placeholder(default_name)
                .with_help_message("Lowercase alphanumeric with hyphens, or @org/name")
                .with_validator(|input: &str| {
                    if input.is_empty() {
                        // Empty means accept placeholder default
                        Ok(inquire::validator::Validation::Valid)
                    } else if input.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '@' || c == '/'
                    }) {
                        Ok(inquire::validator::Validation::Valid)
                    } else {
                        Ok(inquire::validator::Validation::Invalid(
                            "Must be lowercase alphanumeric with hyphens".into(),
                        ))
                    }
                })
                .prompt()?;

            if input.is_empty() { None } else { Some(input) }
        }
    };

    // --- Step 2: Description ---
    let _description = Text::new("Description:")
        .with_placeholder("An AI plugin package")
        .prompt()?;
    // Note: description is collected for future use but not yet passed to Options
    // (Options struct doesn't have a description field today)

    // --- Step 3: Plugin type ---
    let plugin_type = match flag_type {
        Some(t) => Some(t),
        None => {
            let options = vec![
                "composite — skills + agents + hooks (recommended)",
                "skill    — single skill",
                "agent    — autonomous agent",
                "mcp      — Model Context Protocol server",
                "hook     — lifecycle hook",
                "lsp      — Language Server Protocol",
            ];
            let choice = Select::new("Plugin type:", options)
                .with_help_message("Use arrow keys, Enter to select")
                .prompt()?;

            let type_str = choice.split_whitespace().next().unwrap_or("composite");
            Some(type_str.parse::<PluginType>()?)
        }
    };

    Ok((name, plugin_type))
}
```

> **Note on description and version:** The current `init::Options` struct has no `description` or `version` fields — these are hardcoded in `generate_manifest()`. The wizard collects a description for future use, but passing it through requires expanding `Options`. This can be a follow-up. Version is always `"0.1.0"` and not prompted (convention over configuration).

### 5.5 `inquire` Theming

Use `inquire`'s `RenderConfig` to customize the visual style for a modern feel. This is set once at the top of `run()`:

```rust
use inquire::ui::{RenderConfig, Color, StyleSheet, Attributes};

fn styled_render_config() -> RenderConfig<'static> {
    let mut config = RenderConfig::default_colored();
    // Customize prompt prefix
    config.prompt_prefix = inquire::ui::Styled::new("?").with_fg(Color::LightCyan);
    // Customize answered prefix
    config.answered_prefix = inquire::ui::Styled::new("✓").with_fg(Color::LightGreen);
    // Placeholder in dim grey
    config.placeholder = StyleSheet::new().with_fg(Color::DarkGrey);
    config
}
```

Apply globally before any prompt:

```rust
inquire::set_global_render_config(styled_render_config());
```

### 5.6 User Cancellation (Ctrl+C / Esc)

`inquire` returns `Err(InquireError::OperationCanceled)` on Esc and `Err(InquireError::OperationInterrupted)` on Ctrl+C. Both propagate via `?` to `run()`, which prints the error to stderr and returns `ExitCode::FAILURE`. No special handling needed — the existing error path handles this correctly.

### 5.7 Visual Mockup — `aipm init`

```
? What would you like to set up?
  Marketplace only (recommended)    ← highlighted with arrow
  Workspace manifest only
  Both workspace + marketplace
  [Use arrow keys, Enter to select]

✓ What would you like to set up? · Marketplace only (recommended)

? Include starter plugin? (Y/n) · yes
  [Adds scaffold-plugin skill, marketplace-scanner agent, and logging hook]

✓ Include starter plugin? · yes

Created .ai/ marketplace with starter plugin
Configured Claude Code settings
```

### 5.8 Visual Mockup — `aipm-pack init`

```
? Package name: (my-cool-project)      ← grey placeholder = dir name
  [Lowercase alphanumeric with hyphens, or @org/name]

✓ Package name: · my-cool-project      ← accepted default by pressing Enter

? Description: (An AI plugin package)   ← grey placeholder

✓ Description: · A plugin for code review

? Plugin type:
  composite — skills + agents + hooks (recommended)    ← highlighted
  skill    — single skill
  agent    — autonomous agent
  mcp      — Model Context Protocol server
  hook     — lifecycle hook
  lsp      — Language Server Protocol
  [Use arrow keys, Enter to select]

✓ Plugin type: · composite — skills + agents + hooks (recommended)

Initialized plugin package in Q:\projects\my-cool-project
```

## 6. Alternatives Considered

| Option | Pros | Cons | Reason for Rejection |
|--------|------|------|---------------------|
| **cliclack** | Most modern visual style, built-in intro/outro/spinner, vertical sidebar | No feature flags; unconditionally pulls `zeroize` (password zeroing), `indicatif` (progress bars), `strsim`, `textwrap`. Cannot trim unused deps. | Binary size matters; unnecessary transitive deps violate lean dependency principle. ([research §3](../research/docs/2026-03-22-rust-interactive-cli-prompts.md)) |
| **dialoguer** | Most popular (6.6M downloads), part of console-rs ecosystem | No placeholder text support; tests hang due to hard terminal dependency. | No placeholder defaults is a hard requirement for the wizard UX. ([research §6](../research/docs/2026-03-22-rust-interactive-cli-prompts.md)) |
| **requestty** | Most prompt types (11), dynamic flow support | Unmaintained since May 2021; last commit 4+ years ago. | Unmaintained library is a non-starter. ([research §7](../research/docs/2026-03-22-rust-interactive-cli-prompts.md)) |
| **inquire (full features)** | All features including fuzzy matching, date picker, editor | Default features pull in `fuzzy-matcher`, `chrono`, `tempfile`. | We only need Text, Select, Confirm — minimal features give us exactly that. |
| **inquire (selected)** | Feature-gated: only pulls `crossterm`. Placeholder support, RenderConfig theming, custom validators, good testability. Windows support. | No built-in spinner or intro/outro framing. | **Selected.** Lean deps, full control over styling, placeholder defaults work out of the box. Spinner/framing can be added later with `indicatif` if needed. |

## 7. Cross-Cutting Concerns

### 7.1 Binary Size

The minimal `inquire` configuration adds only `crossterm` as a meaningful transitive dependency. `crossterm` is widely used (40M+ downloads) and already battle-tested on Windows. No proc-macro crates are pulled in by our selected features.

To verify impact after implementation:

```bash
# Before
cargo build --release -p aipm && ls -la target/release/aipm
cargo build --release -p aipm-pack && ls -la target/release/aipm-pack

# After (compare)
```

### 7.2 Windows Compatibility

`inquire` uses `crossterm` as its terminal backend, which has first-class Windows support via the Windows Console API. No ANSI escape code workarounds needed. Tested on Windows 10+ including Windows Terminal, cmd.exe, and PowerShell.

### 7.3 CI / Non-Interactive Environments

Two safeguards prevent interactive prompts from blocking CI:

1. **`-y` flag**: Explicitly skips prompts.
2. **TTY detection**: `std::io::stdin().is_terminal()` returns `false` in CI, pipes, and redirected stdin — automatically uses defaults.

No changes needed to existing CI workflows or E2E tests.

### 7.4 Lint Compliance

The wizard modules must comply with all `Cargo.toml` lint rules:
- No `unwrap()`, `expect()`, `panic!()` — use `?` operator throughout
- No `println!()` — all output through existing `writeln!(stdout, ...)` pattern
- No `#[allow(...)]` attributes
- `inquire` errors propagate via `Box<dyn std::error::Error>` in `run()`

**Known lint concern:** The `unwrap_or("my-plugin")` on the default name derivation in the package wizard needs to use a pattern that satisfies the `unwrap_in_result` lint. Since `wizard` functions return `Result`, `unwrap_or` on an `Option` (not `Result`) should be fine — it's not `unwrap()` on a `Result`. Verify during implementation.

## 8. Migration, Rollout, and Testing

### 8.1 Backward Compatibility

| Existing command | New behavior |
|-----------------|--------------|
| `aipm init` (TTY) | **Changed**: launches wizard instead of silent defaults |
| `aipm init` (non-TTY) | **Unchanged**: same defaults as today |
| `aipm init -y` | **New flag**: same behavior as today's `aipm init` |
| `aipm init --workspace` | **Unchanged**: creates workspace manifest (prompts for remaining options if TTY) |
| `aipm-pack init` (TTY) | **Changed**: launches wizard |
| `aipm-pack init` (non-TTY) | **Unchanged**: same defaults as today |
| `aipm-pack init -y` | **New flag**: same behavior as today's `aipm-pack init` |
| All E2E tests | **Unchanged**: tests pipe stdin (non-TTY), so defaults are used automatically |

### 8.2 Test Plan — Design Principle: Don't Trust the Prompt Library

`inquire` renders prompts to a real terminal. We cannot drive that in CI (no TTY). But "can't test the TTY interaction" is **not** an excuse to leave the wizard untested. The wizard contains real logic — which prompts appear, what defaults are used, how answers map to `Options`, what validation accepts or rejects — and every bit of that logic must be covered.

The strategy: **decompose the wizard so that everything except the final `.prompt()` call is unit-testable and snapshot-tested.**

#### 8.2.1 Architecture for Testability

Each wizard module is split into two layers:

1. **Prompt definitions** (pure functions, fully testable) — build prompt configurations, determine which prompts to show based on flags, define validators, map answers to `Options`.
2. **Prompt execution** (thin, untested) — calls `.prompt()` on each definition. This is the only part that touches the terminal.

```rust
// === Testable layer (wizard.rs) ===

/// Describes a single prompt step in the wizard.
/// This struct captures everything about the prompt EXCEPT the terminal interaction.
pub struct PromptStep {
    pub label: &'static str,
    pub kind: PromptKind,
    pub help: Option<&'static str>,
}

pub enum PromptKind {
    Select { options: Vec<&'static str>, default_index: usize },
    Confirm { default: bool },
    Text { placeholder: String },
}

/// Build the list of prompts for workspace init, given pre-filled flags.
/// Returns only the prompts that need to be shown (flags skip their prompt).
pub fn workspace_prompt_steps(
    flag_workspace: bool,
    flag_marketplace: bool,
    flag_no_starter: bool,
) -> Vec<PromptStep> { ... }

/// Build the list of prompts for package init, given pre-filled flags.
pub fn package_prompt_steps(
    dir: &Path,
    flag_name: Option<&str>,
    flag_type: Option<PluginType>,
) -> Vec<PromptStep> { ... }

/// Map raw wizard answers to the final Options values.
/// answers[i] corresponds to prompt_steps[i].
pub fn resolve_workspace_answers(
    answers: &[PromptAnswer],
    flag_workspace: bool,
    flag_marketplace: bool,
    flag_no_starter: bool,
) -> (bool, bool, bool) { ... }

pub fn resolve_package_answers(
    answers: &[PromptAnswer],
    dir: &Path,
    flag_name: Option<&str>,
    flag_type: Option<PluginType>,
) -> (Option<String>, Option<PluginType>) { ... }

/// Validate a package name input. Extracted so it can be tested directly.
pub fn validate_package_name(input: &str) -> Result<(), String> { ... }

// === Thin untested layer (main.rs or wizard.rs) ===

/// Execute the prompt steps against the real terminal.
/// This is the ONLY function that calls inquire::*.prompt().
fn execute_prompts(steps: &[PromptStep]) -> Result<Vec<PromptAnswer>, ...> {
    // For each step, construct the inquire prompt and call .prompt()
}
```

#### 8.2.2 Snapshot Tests — Prompt Definitions

Snapshot every prompt configuration using `insta::assert_snapshot!`. This catches:
- Prompt labels changing unexpectedly
- Placeholder defaults drifting from hardcoded values in `libaipm`
- Help text being removed or garbled
- Select option lists changing order or wording
- Flag-skip logic dropping or showing the wrong prompts

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;

    // --- Workspace wizard prompt snapshots ---

    #[test]
    fn workspace_prompts_no_flags_snapshot() {
        let steps = workspace_prompt_steps(false, false, false);
        assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_workspace_flag_snapshot() {
        let steps = workspace_prompt_steps(true, false, false);
        assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_marketplace_flag_snapshot() {
        let steps = workspace_prompt_steps(false, true, false);
        assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_both_flags_snapshot() {
        let steps = workspace_prompt_steps(true, true, false);
        assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_no_starter_flag_snapshot() {
        let steps = workspace_prompt_steps(false, true, true);
        assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_all_flags_snapshot() {
        let steps = workspace_prompt_steps(true, true, true);
        // All flags set → no prompts needed
        assert_snapshot!(format_steps(&steps));
    }

    // --- Package wizard prompt snapshots ---

    #[test]
    fn package_prompts_no_flags_snapshot() {
        let dir = std::path::Path::new("/projects/my-cool-project");
        let steps = package_prompt_steps(dir, None, None);
        assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn package_prompts_name_flag_snapshot() {
        let dir = std::path::Path::new("/projects/my-cool-project");
        let steps = package_prompt_steps(dir, Some("custom-name"), None);
        assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn package_prompts_type_flag_snapshot() {
        let dir = std::path::Path::new("/projects/my-cool-project");
        let steps = package_prompt_steps(dir, None, Some(PluginType::Skill));
        assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn package_prompts_all_flags_snapshot() {
        let dir = std::path::Path::new("/projects/my-cool-project");
        let steps = package_prompt_steps(dir, Some("foo"), Some(PluginType::Mcp));
        assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn package_prompts_placeholder_uses_dir_name() {
        let dir = std::path::Path::new("/projects/my-cool-project");
        let steps = package_prompt_steps(dir, None, None);
        // The first step (name) should have placeholder = "my-cool-project"
        let name_step = &steps[0];
        match &name_step.kind {
            PromptKind::Text { placeholder } => {
                assert_eq!(placeholder, "my-cool-project");
            }
            other => panic!("expected Text prompt, got {:?}", other),
        }
    }

    /// Helper: serialize prompt steps into a human-readable string for snapshots.
    fn format_steps(steps: &[PromptStep]) -> String {
        let mut out = String::new();
        for (i, step) in steps.iter().enumerate() {
            out.push_str(&format!("Step {}:\n", i + 1));
            out.push_str(&format!("  Label: {}\n", step.label));
            match &step.kind {
                PromptKind::Select { options, default_index } => {
                    out.push_str(&format!("  Kind: Select (default: {})\n", default_index));
                    for (j, opt) in options.iter().enumerate() {
                        let marker = if j == *default_index { " *" } else { "  " };
                        out.push_str(&format!("  {}[{}] {}\n", marker, j, opt));
                    }
                }
                PromptKind::Confirm { default } => {
                    out.push_str(&format!("  Kind: Confirm (default: {})\n", default));
                }
                PromptKind::Text { placeholder } => {
                    out.push_str(&format!("  Kind: Text (placeholder: \"{}\")\n", placeholder));
                }
            }
            if let Some(help) = step.help {
                out.push_str(&format!("  Help: {}\n", help));
            }
            out.push('\n');
        }
        out
    }
}
```

**Expected snapshot for `workspace_prompts_no_flags_snapshot`:**

```
Step 1:
  Label: What would you like to set up?
  Kind: Select (default: 0)
   *[0] Marketplace only (recommended)
    [1] Workspace manifest only
    [2] Both workspace + marketplace
  Help: Use arrow keys, Enter to select

Step 2:
  Label: Include starter plugin?
  Kind: Confirm (default: true)
  Help: Adds scaffold-plugin skill, marketplace-scanner agent, and logging hook
```

#### 8.2.3 Snapshot Tests — Answer Resolution

Snapshot the mapping from raw answers → final `Options` values. This is the logic that converts "user selected option index 2" into `(workspace=true, marketplace=true)`. Test every meaningful combination:

```rust
#[test]
fn resolve_workspace_marketplace_only_snapshot() {
    let answers = vec![PromptAnswer::Selected(0), PromptAnswer::Bool(true)];
    let result = resolve_workspace_answers(&answers, false, false, false);
    assert_snapshot!(format!("{:?}", result)); // (false, true, false)
}

#[test]
fn resolve_workspace_both_snapshot() {
    let answers = vec![PromptAnswer::Selected(2), PromptAnswer::Bool(true)];
    let result = resolve_workspace_answers(&answers, false, false, false);
    assert_snapshot!(format!("{:?}", result)); // (true, true, false)
}

#[test]
fn resolve_workspace_manifest_only_snapshot() {
    let answers = vec![PromptAnswer::Selected(1)];
    let result = resolve_workspace_answers(&answers, false, false, false);
    assert_snapshot!(format!("{:?}", result)); // (true, false, false)
}

#[test]
fn resolve_workspace_decline_starter_snapshot() {
    let answers = vec![PromptAnswer::Selected(0), PromptAnswer::Bool(false)];
    let result = resolve_workspace_answers(&answers, false, false, false);
    assert_snapshot!(format!("{:?}", result)); // (false, true, true)
}

#[test]
fn resolve_package_defaults_snapshot() {
    let dir = std::path::Path::new("/projects/my-cool-project");
    let answers = vec![
        PromptAnswer::Text(String::new()),       // empty = use placeholder
        PromptAnswer::Text(String::new()),       // empty description
        PromptAnswer::Selected(0),               // composite
    ];
    let result = resolve_package_answers(&answers, dir, None, None);
    assert_snapshot!(format!("{:?}", result)); // (None, Some(Composite))
}

#[test]
fn resolve_package_custom_name_snapshot() {
    let dir = std::path::Path::new("/projects/whatever");
    let answers = vec![
        PromptAnswer::Text("my-plugin".to_string()),
        PromptAnswer::Text("A cool plugin".to_string()),
        PromptAnswer::Selected(1),               // skill
    ];
    let result = resolve_package_answers(&answers, dir, None, None);
    assert_snapshot!(format!("{:?}", result)); // (Some("my-plugin"), Some(Skill))
}
```

#### 8.2.4 Unit Tests — Validators

Every validator closure is extracted as a named function and tested directly against valid and invalid inputs. This is critical — validators run inside `inquire` and would otherwise be completely untested.

```rust
#[test]
fn validate_package_name_accepts_lowercase() {
    assert!(validate_package_name("my-plugin").is_ok());
}

#[test]
fn validate_package_name_accepts_scoped() {
    assert!(validate_package_name("@org/my-plugin").is_ok());
}

#[test]
fn validate_package_name_accepts_empty_for_default() {
    assert!(validate_package_name("").is_ok());
}

#[test]
fn validate_package_name_rejects_uppercase() {
    assert!(validate_package_name("MyPlugin").is_err());
}

#[test]
fn validate_package_name_rejects_spaces() {
    assert!(validate_package_name("my plugin").is_err());
}

#[test]
fn validate_package_name_rejects_special_chars() {
    assert!(validate_package_name("my_plugin!").is_err());
}
```

#### 8.2.5 Snapshot Test — Theming Configuration

Snapshot the `RenderConfig` to detect unintended visual regressions:

```rust
#[test]
fn styled_render_config_snapshot() {
    let config = styled_render_config();
    // Snapshot the prompt prefix, answered prefix, placeholder style
    let summary = format!(
        "prompt_prefix: {:?}\nanswered_prefix: {:?}\nplaceholder_fg: {:?}",
        config.prompt_prefix,
        config.answered_prefix,
        config.placeholder,
    );
    assert_snapshot!(summary);
}
```

#### 8.2.6 Summary — What Is and Is Not Tested

| Layer | Tested? | How |
|-------|---------|-----|
| **Prompt step definitions** (which prompts, labels, placeholders, options, defaults) | Yes | `insta::assert_snapshot!` on `PromptStep` lists for every flag combination |
| **Answer → Options mapping** (converting user selections to init config) | Yes | `insta::assert_snapshot!` on resolved outputs for every answer combination |
| **Validators** (package name, any future input validation) | Yes | Direct unit tests against valid/invalid inputs |
| **Theming** (RenderConfig colors, prefixes, styles) | Yes | `insta::assert_snapshot!` on config summary |
| **Flag-skip logic** (which prompts are suppressed by CLI flags) | Yes | Snapshot tests with various flag combinations showing reduced prompt lists |
| **TTY detection** (`is_terminal()` → interactive vs default path) | Yes | E2E tests run in non-TTY (piped stdin) confirming default path; `-y` flag E2E tests |
| **Terminal rendering** (actual ANSI output, cursor movement, key handling) | No | Owned by `inquire` + `crossterm`; not our code |
| **`.prompt()` call** (the 1-line bridge from PromptStep to inquire) | No | Thin glue; no logic to test |

This gives us **full branch coverage** on all wizard logic without needing a TTY, and any drift in prompt configuration — a label change, a missing option, a wrong default — will fail a snapshot.

#### 8.2.7 Existing Tests (No Changes Needed)

- All E2E tests in `crates/aipm/tests/init_e2e.rs` use `assert_cmd::Command` which pipes stdin (non-TTY) → defaults are used → tests pass unchanged.
- All E2E tests in `crates/aipm-pack/tests/init_e2e.rs` same.
- All unit tests in `libaipm` are unaffected (library code unchanged).
- All BDD scenarios in `tests/features/manifest/` are unaffected.

#### 8.2.8 New E2E Tests

- [ ] **E2E: `-y` flag accepted** — `aipm init -y <dir>` succeeds and produces same output as today's `aipm init <dir>`
- [ ] **E2E: `-y` flag accepted (pack)** — `aipm-pack init -y <dir>` succeeds and produces same output as today's `aipm-pack init <dir>`
- [ ] **E2E: `-y` with explicit flags** — `aipm init -y --workspace --marketplace <dir>` produces both workspace and marketplace
- [ ] **E2E: `--yes` long form** — `aipm init --yes <dir>` works identically to `-y`

### 8.3 BDD Feature Updates

Add scenarios to `tests/features/manifest/workspace-init.feature`:

```gherkin
Scenario: The --yes flag skips interactive prompts
  Given an empty directory "ws"
  When I run "aipm init --yes ws"
  Then the command succeeds
  And the directory "ws/.ai/starter-aipm-plugin" exists

Scenario: The -y short flag works
  Given an empty directory "ws"
  When I run "aipm init -y ws"
  Then the command succeeds
  And the directory "ws/.ai" exists
```

Add scenarios to `tests/features/manifest/init.feature`:

```gherkin
Scenario: The --yes flag skips interactive prompts for package init
  Given an empty directory "pkg"
  When I run "aipm-pack init --yes pkg"
  Then the command succeeds
  And the file "pkg/aipm.toml" contains "type = \"composite\""
```

## 9. Implementation Order

| Step | Files | Description |
|------|-------|-------------|
| 1 | `Cargo.toml`, `crates/aipm/Cargo.toml`, `crates/aipm-pack/Cargo.toml` | Add `inquire` dependency with minimal features |
| 2 | `crates/aipm-pack/src/wizard.rs` | Package wizard: `PromptStep`/`PromptKind`/`PromptAnswer` types, `package_prompt_steps()`, `resolve_package_answers()`, `validate_package_name()`, `execute_prompts()` |
| 3 | `crates/aipm-pack/src/wizard.rs` (`#[cfg(test)]`) | Snapshot tests for package prompt steps (all flag combos), answer resolution, validator unit tests |
| 4 | `crates/aipm-pack/src/main.rs` | Add `-y` flag, `mod wizard`, TTY detection, wire up wizard |
| 5 | `crates/aipm-pack/tests/init_e2e.rs` | Add `-y` / `--yes` E2E tests |
| 6 | `crates/aipm/src/wizard.rs` | Workspace wizard: `workspace_prompt_steps()`, `resolve_workspace_answers()`, `execute_prompts()` |
| 7 | `crates/aipm/src/wizard.rs` (`#[cfg(test)]`) | Snapshot tests for workspace prompt steps (all 6 flag combos), answer resolution (all select indices), theming snapshot |
| 8 | `crates/aipm/src/main.rs` | Add `-y` flag, `mod wizard`, TTY detection, wire up wizard |
| 9 | `crates/aipm/tests/init_e2e.rs` | Add `-y` / `--yes` E2E tests |
| 10 | BDD feature files | Add `--yes` scenarios |
| 11 | Review snapshots | `cargo insta review` — inspect every `.snap` file before committing |
| 12 | Verify all four gates pass | `cargo build`, `cargo test`, `cargo clippy`, `cargo fmt` |
| 13 | Verify coverage gate | `cargo +nightly llvm-cov` — wizard logic must hit 89% branch coverage like everything else |

## 10. Open Questions / Unresolved Issues

- [ ] **Marketplace name configurability**: Should we expand `workspace_init::Options` with an optional `marketplace_name` field so the wizard can prompt for it? Or keep `"local-repo-plugins"` hardcoded for now?
- [ ] **Starter plugin name configurability**: Same question for `"starter-aipm-plugin"`. Expanding `Options` would mean modifying `libaipm` (counter to non-goal), but is a natural next step.
- [ ] **Description field in package init**: `init::Options` has no `description` field. The wizard collects one but cannot pass it through today. Should we add `description: Option<&str>` to `Options` and include it in `generate_manifest()` as part of this work, or defer?
- [ ] **Tool selection prompt**: The wizard design shows a multi-select for AI tools (Claude, Cursor, Windsurf), but only Claude has an adaptor today. Should we show the prompt with only Claude, or skip the prompt entirely until more adaptors exist?
- [ ] **Ctrl+C behavior**: Should `OperationInterrupted` print a friendly "Cancelled." message or just exit silently? Current behavior propagates the error string.
