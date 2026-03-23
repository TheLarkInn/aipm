---
date: 2026-03-22 12:53:02 PDT
researcher: Claude (Opus 4.6)
git_commit: 085498aafcdaee6eab99ccbbe9b7b3d44401d786
branch: main
repository: aipm
topic: "Rust Interactive CLI Prompt Libraries & aipm init Wizard Design"
tags: [research, codebase, cli, tui, interactive-prompts, init, wizard]
status: complete
last_updated: 2026-03-22
last_updated_by: Claude (Opus 4.6)
---

# Research: Rust Interactive CLI Prompt Libraries & aipm init Wizard Design

## Research Question

Is there an "Inquirer" equivalent in Rust for building interactive CLI wizards with TUI menus, selectors, and styled prompts? Design an interactive wizard flow for `aipm init` (both workspace and package) with placeholder defaults, marketplace naming, and modern aesthetics.

## Summary

Four Rust crates provide Inquirer.js-equivalent interactive prompts. **cliclack** is the strongest fit for aipm — it provides a modern, beautiful wizard experience with placeholder text, validation, spinners, and a vertical sidebar that visually connects multi-step flows (inspired by `@clack/prompts`, used by create-next-app and create-svelte). The current `aipm init` and `aipm-pack init` commands accept all configuration via CLI flags with hardcoded defaults — no interactive prompts exist today.

---

## Detailed Findings

### 1. Current `aipm init` Flow (Workspace Init)

**CLI entry point:** `crates/aipm/src/main.rs:20-36`

```
aipm init [--workspace] [--marketplace] [--no-starter] [DIR]
```

Current flags:
| Flag | Default | Effect |
|------|---------|--------|
| `--workspace` | false | Create `aipm.toml` with `[workspace]` section |
| `--marketplace` | false | Create `.ai/` marketplace directory |
| `--no-starter` | false | Skip starter plugin |
| `DIR` | `.` (cwd) | Target directory |

**Default behavior** (no flags): marketplace only (`do_marketplace = true`), no workspace manifest.

**Logic:** `crates/libaipm/src/workspace_init/mod.rs:98-121`
- If `workspace` → writes `aipm.toml` with `[workspace]` section
- If `marketplace` → scaffolds `.ai/` with marketplace.json, starter plugin, Claude settings
- Tool adaptors (currently just Claude) apply settings via `ToolAdaptor::apply()`

**Generated defaults:**
- Workspace manifest: `members = [".ai/*"]`, `plugins_dir = ".ai"`
- Marketplace name: `"local-repo-plugins"`
- Starter plugin name: `"starter-aipm-plugin"`
- Starter plugin version: `"0.1.0"`
- Starter plugin type: `"composite"`

### 2. Current `aipm-pack init` Flow (Package Init)

**CLI entry point:** `crates/aipm-pack/src/main.rs:20-34`

```
aipm-pack init [--name NAME] [--type TYPE] [DIR]
```

Current flags:
| Flag | Default | Effect |
|------|---------|--------|
| `--name` | directory name | Package name |
| `--type` | composite | Plugin type (skill/agent/mcp/hook/lsp/composite) |
| `DIR` | `.` (cwd) | Target directory |

**Logic:** `crates/libaipm/src/init.rs:57-96`
- Validates package name
- Creates directory layout based on plugin type
- Generates `aipm.toml` with `[package]` section

**Generated manifest:**
```toml
[package]
name = "<name>"
version = "0.1.0"
type = "<type>"
edition = "2024"
```

### 3. Library Comparison

| Feature | inquire | dialoguer | cliclack | requestty |
|---------|---------|-----------|----------|-----------|
| **Version** | 0.9.4 | 0.12.0 | 0.5.0 | 0.6.3 |
| **Downloads** | ~1.3M | ~6.6M | ~276k | Low |
| **Maintained** | Yes | Yes | Yes | No (2021) |
| **Text input** | Yes | Yes | Yes | Yes |
| **Select** | Yes | Yes | Yes | Yes |
| **MultiSelect** | Yes | Yes | Yes | Yes |
| **Confirm** | Yes | Yes | Yes | Yes |
| **Password** | Yes | Yes | Yes | Yes |
| **Placeholder text** | Yes | No | Yes | No |
| **Default value** | Yes | Yes | Yes | Yes |
| **Validation** | Macros | Trait | Closure | Yes |
| **Theming** | RenderConfig | Theme trait | Theme trait | No |
| **Colored output** | Yes | ColorfulTheme | Default | Basic |
| **Spinner** | No | No* | Yes | No |
| **Progress bar** | No | No* | Yes | No |
| **Intro/Outro framing** | No | No | Yes | No |
| **Wizard sidebar** | No | No | Yes | No |
| **Windows** | Yes | Yes | Yes | Yes |
| **Testability** | Good | Poor | Unknown | Unknown |
| **Fuzzy search** | Custom | FuzzySelect | Type-to-filter | No |

*dialoguer composes with indicatif for progress bars.

### 4. cliclack — Recommended Library

- **Crate**: https://crates.io/crates/cliclack
- **GitHub**: https://github.com/fadeevab/cliclack
- **Docs**: https://docs.rs/cliclack/latest/cliclack/
- **Inspired by**: [@clack/prompts](https://www.npmjs.com/package/@clack/prompts)

Key features for aipm:
1. **Vertical sidebar** connecting wizard steps visually
2. **`intro()` / `outro()`** for framing the wizard session
3. **Placeholder text** rendered as grey hint text in inputs
4. **`select()` with descriptions** — items have label + hint text
5. **Validation** via simple closures returning `Result`
6. **Spinner** for async operations (e.g., "Creating marketplace...")
7. **Cross-platform** including Windows

Example:
```rust
use cliclack::{intro, outro, input, select, confirm, spinner};

intro("aipm init")?;

let name: String = input("Marketplace name?")
    .placeholder("local-repo-plugins")
    .validate(|input: &String| {
        if input.is_empty() { Err("Name required") } else { Ok(()) }
    })
    .interact()?;

outro("Workspace initialized!")?;
```

### 5. inquire — Alternative Library

- **Crate**: https://crates.io/crates/inquire
- **GitHub**: https://github.com/mikaelmello/inquire
- **Docs**: https://docs.rs/inquire/latest/inquire/

Advantages over cliclack:
- More prompt types (DateSelect, Editor, CustomType)
- Deeper theming via `RenderConfig`
- Better testability (custom backends)
- Autocompletion on text inputs

### 6. dialoguer — Not Recommended

Despite being most popular by downloads, has two dealbreakers for aipm:
- **No placeholder text** — can't show grey hint defaults
- **Poor testability** — tests that use dialoguer hang due to hard terminal dependency

### 7. requestty — Not Recommended

Unmaintained since May 2021. Should not be used for new projects.

---

## Proposed Wizard Flow Designs

### A. `aipm init` Interactive Wizard (Workspace)

```
┌  aipm init
│
◇  What would you like to set up?
│  ● Marketplace only (recommended)
│  ○ Workspace manifest only
│  ○ Both workspace + marketplace
│
◇  Marketplace name
│  local-repo-plugins          ← grey placeholder, empty input
│
◇  Starter plugin name
│  starter-aipm-plugin         ← grey placeholder, empty input
│
◇  Include starter plugin?
│  Yes / No                    ← default: Yes
│
◇  Which AI tools should we configure?
│  ◻ Claude Code (recommended)
│  ◻ Cursor
│  ◻ Windsurf
│
◆  Creating marketplace...
│
└  Done! Created .ai/ marketplace with starter plugin
   Configured Claude Code settings
```

**Flow when user just presses Enter on every prompt (accepts all defaults):**
- Setup: Marketplace only
- Marketplace name: `local-repo-plugins`
- Starter plugin name: `starter-aipm-plugin`
- Include starter: Yes
- Tools: Claude Code

This matches today's `aipm init` behavior exactly.

**`aipm init -y` / `aipm init --yes`:**
Skips all prompts, uses all defaults (equivalent to today's `aipm init`).

### B. `aipm-pack init` Interactive Wizard (Package)

```
┌  aipm-pack init
│
◇  Package name
│  my-project                  ← grey placeholder = directory name
│
◇  Description
│  An AI plugin package        ← grey placeholder
│
◇  Plugin type
│  ● Composite (skills + agents + hooks)
│  ○ Skill (single skill)
│  ○ Agent (autonomous agent)
│  ○ MCP (Model Context Protocol server)
│  ○ Hook (lifecycle hook)
│  ○ LSP (Language Server Protocol)
│
◇  Version
│  0.1.0                       ← grey placeholder
│
◆  Creating package...
│
└  Done! Initialized plugin package in ./my-project
```

**`aipm-pack init -y` / `aipm-pack init --yes`:**
Skips all prompts — uses directory name, no description, composite type, 0.1.0.

### C. Visual Reference — cliclack Aesthetic

The vertical bar `│` on the left connects all steps into a visual flow.
Answered prompts show a filled diamond `◆` with the chosen value.
Pending prompts show an open diamond `◇`.
The intro `┌` and outro `└` frame the entire session.

```
┌  aipm init
│
◆  What would you like to set up?
│  Marketplace only
│
◇  Marketplace name
│  _                           ← cursor blinking, placeholder in grey
│
```

After answering:

```
┌  aipm init
│
◆  What would you like to set up?
│  Marketplace only
│
◆  Marketplace name
│  my-custom-marketplace
│
◇  Starter plugin name
│  _
│
```

---

## Implementation Notes

### Dependency Addition

Add to `Cargo.toml` workspace dependencies:
```toml
cliclack = "0.5"
```

Add to `crates/aipm/Cargo.toml` and `crates/aipm-pack/Cargo.toml`:
```toml
cliclack = { workspace = true }
```

### Integration Pattern with Clap

```rust
// In Commands enum, add --yes flag:
Init {
    /// Skip interactive prompts, use all defaults.
    #[arg(short = 'y', long)]
    yes: bool,
    // ... existing flags become overrides for specific values
}

// In run():
if yes || !std::io::stdin().is_terminal() {
    // Use defaults (today's behavior)
} else {
    // Launch interactive wizard
}
```

### Testability Consideration

Interactive prompts are inherently untestable in CI. The pattern should be:
1. **Wizard function** collects user input → returns a config struct
2. **Init function** (existing) takes the config struct → performs the init
3. Tests only exercise the init function with predetermined configs
4. The wizard is a thin presentation layer, not tested directly

### Non-Interactive Detection

Use `std::io::stdin().is_terminal()` (stabilized in Rust 1.70) or the `atty` crate to detect if stdin is a terminal. If piped (CI, scripts), auto-use defaults.

---

## Code References

- `crates/aipm/src/main.rs:20-36` — CLI arg definitions for `aipm init`
- `crates/aipm/src/main.rs:39-88` — `run()` handler for `aipm init`
- `crates/aipm-pack/src/main.rs:20-34` — CLI arg definitions for `aipm-pack init`
- `crates/aipm-pack/src/main.rs:37-60` — `run()` handler for `aipm-pack init`
- `crates/libaipm/src/workspace_init/mod.rs:33-42` — `Options` struct
- `crates/libaipm/src/workspace_init/mod.rs:98-121` — `init()` function
- `crates/libaipm/src/workspace_init/mod.rs:146-165` — workspace manifest defaults
- `crates/libaipm/src/workspace_init/mod.rs:171-241` — marketplace scaffolding
- `crates/libaipm/src/workspace_init/mod.rs:243-261` — starter manifest defaults
- `crates/libaipm/src/init.rs:12-20` — Package `Options` struct
- `crates/libaipm/src/init.rs:57-96` — Package `init()` function
- `crates/libaipm/src/init.rs:187-204` — Package manifest generation
- `crates/libaipm/src/workspace_init/adaptors/claude.rs` — Claude tool adaptor

## Sources

### Library Documentation
- [cliclack on crates.io](https://crates.io/crates/cliclack)
- [cliclack GitHub](https://github.com/fadeevab/cliclack)
- [cliclack docs.rs](https://docs.rs/cliclack/latest/cliclack/)
- [inquire on crates.io](https://crates.io/crates/inquire)
- [inquire GitHub](https://github.com/mikaelmello/inquire)
- [inquire docs.rs](https://docs.rs/inquire/latest/inquire/)
- [dialoguer on crates.io](https://crates.io/crates/dialoguer)
- [dialoguer GitHub](https://github.com/console-rs/dialoguer)
- [requestty GitHub](https://github.com/Lutetium-Vanadium/requestty)
- [@clack/prompts (npm)](https://www.npmjs.com/package/@clack/prompts)

### Comparison Articles
- [Comparison of Rust CLI Prompts](https://fadeevab.com/comparison-of-rust-cli-prompts/) — by Alexander Fadeev (cliclack author)

## Open Questions

1. Should the marketplace name be configurable at all, or should it always be `local-repo-plugins`?
2. Should the wizard support going back to a previous step (cliclack doesn't support this natively)?
3. Should we detect existing `.claude/settings.json` and pre-select Claude in the tool list?
4. How should the wizard behave when some flags are provided but not all? (e.g., `aipm init --workspace` — prompt for marketplace options only?)
5. Should `aipm-pack init` also get a `--yes` flag for consistency?
