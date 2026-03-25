# Marketplace Name Customization in `aipm init`

| Document Metadata      | Details                              |
| ---------------------- | ------------------------------------ |
| Author(s)              | selarkin                             |
| Status                 | Draft (WIP)                          |
| Team / Owner           | aipm                                 |
| Created / Last Updated | 2026-03-25                           |

## 1. Executive Summary

`aipm init` currently hardcodes the marketplace name as `"local-repo-plugins"` in 14+ locations across the codebase. This spec adds a `--name` CLI flag and an interactive Text prompt to the init wizard so users can choose a custom marketplace name. The default remains `"local-repo-plugins"` for backward compatibility. The name propagates through the `Options` struct, `ToolAdaptor` trait, and scaffold script to all downstream consumers.

## 2. Context and Motivation

### 2.1 Current State

The `aipm init` command scaffolds a `.ai/` local marketplace directory with a `marketplace.json` file. The marketplace name `"local-repo-plugins"` is embedded as a string literal in:

- `generate_marketplace_json()` in `workspace_init/mod.rs` (2 occurrences, [lines 459/471](https://github.com/TheLarkInn/aipm/blob/dd0ee78/crates/libaipm/src/workspace_init/mod.rs#L459))
- `generate_scaffold_script()` in `workspace_init/mod.rs` (2 occurrences, [lines 366/398](https://github.com/TheLarkInn/aipm/blob/dd0ee78/crates/libaipm/src/workspace_init/mod.rs#L366))
- Claude Code adaptor `claude.rs` (8 occurrences across fresh-file templates and merge logic, [lines 32-121](https://github.com/TheLarkInn/aipm/blob/dd0ee78/crates/libaipm/src/workspace_init/adaptors/claude.rs#L32))

There is no shared constant, no field on the `Options` struct, and no CLI flag or wizard prompt for the name. The interactive wizard spec explicitly deferred this as a follow-up ([specs/2026-03-22-interactive-init-wizard.md:308](https://github.com/TheLarkInn/aipm/blob/dd0ee78/specs/2026-03-22-interactive-init-wizard.md#L308)).

### 2.2 The Problem

- **User impact:** Teams with multiple marketplaces or organizational naming conventions cannot customize the marketplace identity.
- **Technical debt:** The name is duplicated across 14+ string literals with no single source of truth.

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [ ] Users can specify a custom marketplace name via `--name <NAME>` CLI flag
- [ ] The interactive wizard prompts for the marketplace name with `"local-repo-plugins"` as the default placeholder
- [ ] The name flows through to `marketplace.json`, Claude Code `settings.json`, and the scaffold script
- [ ] Validation matches existing package-name rules: lowercase alphanumeric + hyphens (plus `@` and `/` for scoped names)
- [ ] Empty input in the wizard uses the default `"local-repo-plugins"`
- [ ] Non-interactive mode (`-y`) uses the default unless `--name` is provided
- [ ] All existing tests are updated to use the parameterized name

### 3.2 Non-Goals (Out of Scope)

- [ ] Customizing the starter plugin name (remains `"starter-aipm-plugin"`)
- [ ] Changes to the `migrate` command (already reads marketplace name dynamically from `marketplace.json`)
- [ ] Adding marketplace name validation to the `migrate` path
- [ ] Renaming existing marketplaces after creation

## 4. Proposed Solution (High-Level Design)

### 4.1 Data Flow

```
CLI: --name "my-plugins"  ──┐
                             ├──► wizard resolves ──► Options.marketplace_name ──┬──► generate_marketplace_json(name, no_starter)
Wizard: "Marketplace name:" ─┘                                                  ├──► generate_scaffold_script(name)
                                                                                ├──► adaptor.apply(dir, no_starter, name, fs)
                                                                                │      └──► Claude settings.json: extraKnownMarketplaces[name]
                                                                                │      └──► enabledPlugins: "starter-aipm-plugin@{name}"
                                                                                └──► stdout: "Created .ai/ marketplace '{name}'"
```

### 4.2 Architectural Pattern

This follows the existing **flag-elision wizard pattern** established by `aipm-pack init` (see [research](../research/docs/2026-03-25-marketplace-name-customization-in-init.md#5-reference-pattern-aipm-pack-wizard-text-input)):

1. If `--name` is provided, skip the wizard prompt
2. If interactive, show a `Text` prompt with `"local-repo-plugins"` as placeholder
3. If non-interactive (`-y`), use the default
4. Resolved name flows into the `Options` struct and downstream

### 4.3 Key Components

| Component | Change | File |
|-----------|--------|------|
| CLI args | Add `--name` field to `Commands::Init` | `crates/aipm/src/main.rs` |
| Wizard types | Add `Text` variant to `PromptKind`, `Text` variant to `PromptAnswer` | `crates/aipm/src/wizard.rs` |
| Wizard steps | Add marketplace-name prompt step | `crates/aipm/src/wizard.rs` |
| Wizard TTY | Add `Text` prompt execution via `inquire::Text` | `crates/aipm/src/wizard_tty.rs` |
| Wizard resolve | Return `Option<String>` for marketplace name | `crates/aipm/src/wizard.rs` |
| `Options` struct | Add `marketplace_name: &'a str` field | `crates/libaipm/src/workspace_init/mod.rs` |
| `generate_marketplace_json()` | Accept `&str` name parameter | `crates/libaipm/src/workspace_init/mod.rs` |
| `generate_scaffold_script()` | Read name from `marketplace.json` at runtime | `crates/libaipm/src/workspace_init/mod.rs` |
| `scaffold_marketplace()` | Thread name through to `generate_marketplace_json()` | `crates/libaipm/src/workspace_init/mod.rs` |
| `ToolAdaptor` trait | Add `marketplace_name: &str` parameter to `apply()` | `crates/libaipm/src/workspace_init/mod.rs` |
| Claude adaptor | Replace all hardcoded `"local-repo-plugins"` with the parameter | `crates/libaipm/src/workspace_init/adaptors/claude.rs` |

## 5. Detailed Design

### 5.1 CLI: Add `--name` Flag

**File:** `crates/aipm/src/main.rs`

Add a new field to the `Commands::Init` variant:

```rust
/// Initialize a workspace for AI plugin management.
Init {
    // ... existing fields ...

    /// Custom marketplace name (default: "local-repo-plugins").
    #[arg(long)]
    name: Option<String>,

    // ... dir field ...
},
```

In the match arm, pass `name` through to the wizard:

```rust
Some(Commands::Init { yes, workspace, marketplace, no_starter, manifest, name, dir }) => {
    let dir = if dir.as_os_str() == "." { std::env::current_dir()? } else { dir };
    let interactive = !yes && std::io::stdin().is_terminal();

    let (do_workspace, do_marketplace, do_no_starter, marketplace_name) =
        wizard_tty::resolve(interactive, (workspace, marketplace, no_starter), name)?;

    let adaptors = libaipm::workspace_init::adaptors::defaults();

    let opts = libaipm::workspace_init::Options {
        dir: &dir,
        workspace: do_workspace,
        marketplace: do_marketplace,
        no_starter: do_no_starter,
        manifest,
        marketplace_name: &marketplace_name,
    };

    let result = libaipm::workspace_init::init(&opts, &adaptors, &libaipm::fs::Real)?;
    // ... output ...
}
```

### 5.2 Wizard Types: Add `Text` Variant

**File:** `crates/aipm/src/wizard.rs`

Extend `PromptKind` and `PromptAnswer` to support text input, mirroring the `aipm-pack` wizard ([`crates/aipm-pack/src/wizard.rs:28-52`](https://github.com/TheLarkInn/aipm/blob/dd0ee78/crates/aipm-pack/src/wizard.rs#L28)):

```rust
pub enum PromptKind {
    Select {
        options: Vec<&'static str>,
        default_index: usize,
    },
    Confirm {
        default: bool,
    },
    /// Free-form text input.
    Text {
        /// Grey placeholder text (shown when input is empty).
        placeholder: String,
        /// Whether to apply marketplace-name validation.
        validate: bool,
    },
}

pub enum PromptAnswer {
    Selected(usize),
    Bool(bool),
    /// Text input.
    Text(String),
}
```

### 5.3 Wizard Steps: Add Marketplace Name Prompt

**File:** `crates/aipm/src/wizard.rs`

Update `workspace_prompt_steps` to accept and optionally generate a name prompt. The name prompt is shown only when:
- Marketplace creation is possible (either `--marketplace` flag is set or the setup prompt will be shown)
- `--name` was not already provided

```rust
pub fn workspace_prompt_steps(
    flag_workspace: bool,
    flag_marketplace: bool,
    flag_no_starter: bool,
    flag_name: Option<&str>,
) -> Vec<PromptStep> {
    let mut steps = Vec::new();

    // Step 1: Setup mode (existing — unchanged)
    let needs_setup_prompt = !flag_workspace && !flag_marketplace;
    if needs_setup_prompt {
        steps.push(PromptStep {
            label: "What would you like to set up?",
            kind: PromptKind::Select { options: SETUP_OPTIONS.to_vec(), default_index: 0 },
            help: Some("Use arrow keys, Enter to select"),
        });
    }

    let marketplace_possible = flag_marketplace || needs_setup_prompt;

    // Step 2: Marketplace name (new — skip if --name was provided or marketplace not possible)
    if marketplace_possible && flag_name.is_none() {
        steps.push(PromptStep {
            label: "Marketplace name:",
            kind: PromptKind::Text {
                placeholder: "local-repo-plugins".to_string(),
                validate: true,
            },
            help: Some("Lowercase alphanumeric with hyphens, or press Enter for default"),
        });
    }

    // Step 3: Include starter plugin? (existing — unchanged)
    if marketplace_possible && !flag_no_starter {
        steps.push(PromptStep {
            label: "Include starter plugin?",
            kind: PromptKind::Confirm { default: true },
            help: Some("Adds scaffold-plugin skill, marketplace-scanner agent, and logging hook"),
        });
    }

    steps
}
```

### 5.4 Wizard Validation: `validate_marketplace_name()`

**File:** `crates/aipm/src/wizard.rs`

Reuse the same rules as `aipm-pack`'s `validate_package_name()` ([`crates/aipm-pack/src/wizard.rs:168-180`](https://github.com/TheLarkInn/aipm/blob/dd0ee78/crates/aipm-pack/src/wizard.rs#L168)):

```rust
/// Validate a marketplace name.
///
/// Empty string is valid (means "use default").
/// Otherwise must be lowercase alphanumeric with hyphens, optionally @org/name.
pub fn validate_marketplace_name(input: &str) -> Result<(), String> {
    if input.is_empty() {
        return Ok(());
    }

    for c in input.chars() {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '@' || c == '/') {
            return Err("Must be lowercase alphanumeric with hyphens".to_string());
        }
    }

    Ok(())
}
```

### 5.5 Wizard Resolution: Return Marketplace Name

**File:** `crates/aipm/src/wizard.rs`

Update `resolve_workspace_answers` to return a 4-tuple including the resolved marketplace name:

```rust
/// Returns `(workspace, marketplace, no_starter, marketplace_name)`.
pub fn resolve_workspace_answers(
    answers: &[PromptAnswer],
    flag_workspace: bool,
    flag_marketplace: bool,
    flag_no_starter: bool,
    flag_name: Option<&str>,
) -> (bool, bool, bool, String) {
    let needs_setup_prompt = !flag_workspace && !flag_marketplace;
    let mut idx = 0;

    // Step 1: Resolve setup mode (existing logic — unchanged)
    let (do_workspace, do_marketplace) = if needs_setup_prompt {
        let result = match answers.get(idx) {
            Some(PromptAnswer::Selected(1)) => (true, false),
            Some(PromptAnswer::Selected(2)) => (true, true),
            _ => (false, true),
        };
        idx += 1;
        result
    } else {
        (flag_workspace, flag_marketplace)
    };

    let marketplace_possible = flag_marketplace || needs_setup_prompt;

    // Step 2: Resolve marketplace name (new)
    let marketplace_name = if let Some(name) = flag_name {
        name.to_string()
    } else if marketplace_possible {
        let resolved = match answers.get(idx) {
            Some(PromptAnswer::Text(t)) if !t.is_empty() => t.clone(),
            _ => "local-repo-plugins".to_string(),
        };
        idx += 1;
        resolved
    } else {
        "local-repo-plugins".to_string()
    };

    // Step 3: Resolve no_starter (existing logic — unchanged)
    let no_starter = if marketplace_possible && !flag_no_starter {
        match answers.get(idx) {
            Some(PromptAnswer::Bool(include)) => {
                if do_marketplace { !include } else { false }
            },
            _ => false,
        }
    } else {
        flag_no_starter
    };

    (do_workspace, do_marketplace, no_starter, marketplace_name)
}
```

Update `resolve_defaults` to return the name:

```rust
pub fn resolve_defaults(
    workspace: bool,
    marketplace: bool,
    no_starter: bool,
    name: Option<&str>,
) -> (bool, bool, bool, String) {
    let (w, m) = if !workspace && !marketplace { (false, true) } else { (workspace, marketplace) };
    let marketplace_name = name.unwrap_or("local-repo-plugins").to_string();
    (w, m, no_starter, marketplace_name)
}
```

### 5.6 Wizard TTY Bridge: Handle `Text` Prompts

**File:** `crates/aipm/src/wizard_tty.rs`

Update `resolve` signature to accept and return the name, and add `Text` handling to `execute_prompts`:

```rust
pub fn resolve(
    interactive: bool,
    flags: (bool, bool, bool),
    flag_name: Option<String>,
) -> Result<(bool, bool, bool, String), Box<dyn std::error::Error>> {
    let (workspace, marketplace, no_starter) = flags;
    if interactive {
        inquire::set_global_render_config(styled_render_config());
        let steps = workspace_prompt_steps(workspace, marketplace, no_starter, flag_name.as_deref());
        let answers = execute_prompts(&steps)?;
        Ok(resolve_workspace_answers(&answers, workspace, marketplace, no_starter, flag_name.as_deref()))
    } else {
        Ok(resolve_defaults(workspace, marketplace, no_starter, flag_name.as_deref()))
    }
}
```

Add `Text` match arm to `execute_prompts` (modeled on [`aipm-pack/wizard_tty.rs:51-65`](https://github.com/TheLarkInn/aipm/blob/dd0ee78/crates/aipm-pack/src/wizard_tty.rs#L51)):

```rust
PromptKind::Text { placeholder, validate } => {
    let mut prompt = inquire::Text::new(step.label).with_placeholder(placeholder);
    if let Some(help) = step.help {
        prompt = prompt.with_help_message(help);
    }
    if *validate {
        prompt =
            prompt.with_validator(|input: &str| match validate_marketplace_name(input) {
                Ok(()) => Ok(inquire::validator::Validation::Valid),
                Err(msg) => Ok(inquire::validator::Validation::Invalid(msg.into())),
            });
    }
    let result = prompt.prompt()?;
    PromptAnswer::Text(result)
},
```

### 5.7 `Options` Struct: Add `marketplace_name` Field

**File:** `crates/libaipm/src/workspace_init/mod.rs`

```rust
pub struct Options<'a> {
    pub dir: &'a Path,
    pub workspace: bool,
    pub marketplace: bool,
    pub no_starter: bool,
    pub manifest: bool,
    /// Marketplace name (e.g., "local-repo-plugins").
    pub marketplace_name: &'a str,
}
```

### 5.8 `scaffold_marketplace()`: Thread Name Through

**File:** `crates/libaipm/src/workspace_init/mod.rs`

Update `scaffold_marketplace` to accept and pass the name:

```rust
fn scaffold_marketplace(
    dir: &Path,
    no_starter: bool,
    manifest: bool,
    marketplace_name: &str,
    fs: &dyn Fs,
) -> Result<(), Error> {
    // ... existing setup ...

    fs.write_file(
        &ai_dir.join(".claude-plugin").join("marketplace.json"),
        generate_marketplace_json(marketplace_name, no_starter).as_bytes(),
    )?;

    // ... rest unchanged, but pass marketplace_name to generate_scaffold_script ...
}
```

Update the call site in `init()`:

```rust
if opts.marketplace {
    scaffold_marketplace(opts.dir, opts.no_starter, opts.manifest, opts.marketplace_name, fs)?;
    actions.push(InitAction::MarketplaceCreated);

    for adaptor in adaptors {
        if adaptor.apply(opts.dir, opts.no_starter, opts.marketplace_name, fs)? {
            actions.push(InitAction::ToolConfigured(adaptor.name().to_string()));
        }
    }
}
```

### 5.9 `generate_marketplace_json()`: Accept Name Parameter

**File:** `crates/libaipm/src/workspace_init/mod.rs`

Replace the hardcoded string literal approach with `serde_json` construction so the name is interpolated safely:

```rust
fn generate_marketplace_json(marketplace_name: &str, no_starter: bool) -> String {
    let mut map = serde_json::Map::new();
    map.insert("name".to_string(), serde_json::Value::String(marketplace_name.to_string()));
    map.insert("owner".to_string(), serde_json::json!({ "name": "local" }));
    map.insert("metadata".to_string(), serde_json::json!({ "description": "Local plugins for this repository" }));

    if no_starter {
        map.insert("plugins".to_string(), serde_json::json!([]));
    } else {
        map.insert("plugins".to_string(), serde_json::json!([
            {
                "name": "starter-aipm-plugin",
                "source": "./starter-aipm-plugin",
                "description": "Default starter plugin \u{2014} scaffold new plugins, scan your marketplace, and log tool usage"
            }
        ]));
    }

    let obj = serde_json::Value::Object(map);
    let mut output = serde_json::to_string_pretty(&obj).unwrap_or_default();
    output.push('\n');
    output
}
```

> **Note:** Using `serde_json` construction instead of string literals eliminates the need to embed the name in two duplicate template strings. This is a structural improvement, not scope creep — it is required to safely interpolate the user-provided name. We must avoid `unwrap()` per project lint rules; use a fallback (`.unwrap_or_default()`) or propagate the error.

### 5.10 `generate_scaffold_script()`: Read Name from `marketplace.json` at Runtime

**File:** `crates/libaipm/src/workspace_init/mod.rs`

The scaffold script already reads and parses `marketplace.json` at runtime. Replace the two hardcoded `"local-repo-plugins"` references with reads from the parsed object.

**Change 1 — Fallback object (line 366):** Keep `"local-repo-plugins"` as a fallback when `marketplace.json` doesn't exist (this is a safety net). No change needed here since the script already reads from the file when it exists.

**Change 2 — `enabledPlugins` key (line 398):** Replace:
```typescript
const pluginKey = `${name}@local-repo-plugins`;
```
with:
```typescript
const pluginKey = `${name}@${marketplace.name}`;
```

This works because `marketplace` is always defined at this point — either parsed from the file or set to the fallback object. The variable is already in scope.

### 5.11 `ToolAdaptor` Trait: Add `marketplace_name` Parameter

**File:** `crates/libaipm/src/workspace_init/mod.rs`

```rust
pub trait ToolAdaptor {
    fn name(&self) -> &'static str;

    fn apply(
        &self,
        dir: &Path,
        no_starter: bool,
        marketplace_name: &str,
        fs: &dyn Fs,
    ) -> Result<bool, Error>;
}
```

### 5.12 Claude Code Adaptor: Use Parameter Instead of Literals

**File:** `crates/libaipm/src/workspace_init/adaptors/claude.rs`

Replace all 8 occurrences of `"local-repo-plugins"` with the `marketplace_name` parameter. Switch from inline string templates to `serde_json` construction for the fresh-file path as well, so the name can be interpolated.

**`apply()` (fresh file path):**

```rust
fn apply(&self, dir: &Path, no_starter: bool, marketplace_name: &str, fs: &dyn Fs) -> Result<bool, Error> {
    let settings_dir = dir.join(".claude");
    let settings_path = settings_dir.join("settings.json");

    if fs.exists(&settings_path) {
        return merge_claude_settings(&settings_path, no_starter, marketplace_name, fs);
    }

    fs.create_dir_all(&settings_dir)?;

    let marketplace_entry = serde_json::json!({
        "source": { "source": "directory", "path": "./.ai" }
    });

    let mut settings = serde_json::Map::new();
    settings.insert(
        "extraKnownMarketplaces".to_string(),
        serde_json::json!({ marketplace_name: marketplace_entry }),
    );

    if !no_starter {
        let plugin_key = format!("starter-aipm-plugin@{marketplace_name}");
        settings.insert(
            "enabledPlugins".to_string(),
            serde_json::json!({ plugin_key: true }),
        );
    }

    let obj = serde_json::Value::Object(settings);
    let mut output = serde_json::to_string_pretty(&obj).unwrap_or_default();
    output.push('\n');

    crate::workspace_init::write_file(&settings_path, &output, fs)?;
    Ok(true)
}
```

**`merge_claude_settings()` — replace all literal references:**

```rust
fn merge_claude_settings(
    settings_path: &Path,
    no_starter: bool,
    marketplace_name: &str,
    fs: &dyn Fs,
) -> Result<bool, Error> {
    // ... existing read/parse logic unchanged ...

    let has_marketplace =
        obj.get("extraKnownMarketplaces").and_then(|ekm| ekm.get(marketplace_name)).is_some();

    let starter_key = format!("starter-aipm-plugin@{marketplace_name}");

    if no_starter {
        if has_marketplace {
            return Ok(false);
        }
    } else {
        let has_enabled = obj
            .get("enabledPlugins")
            .and_then(|ep| ep.as_object())
            .is_some_and(|ep| ep.contains_key(&starter_key));
        if has_marketplace && has_enabled {
            return Ok(false);
        }
    }

    // Insert marketplace entry
    let marketplace_entry = serde_json::json!({
        "source": { "source": "directory", "path": "./.ai" }
    });

    if let Some(ekm) = obj.get_mut("extraKnownMarketplaces") {
        if let Some(ekm_obj) = ekm.as_object_mut() {
            ekm_obj.entry(marketplace_name).or_insert(marketplace_entry);
        }
    } else {
        obj.insert(
            "extraKnownMarketplaces".to_string(),
            serde_json::json!({ marketplace_name: marketplace_entry }),
        );
    }

    if !no_starter {
        let enabled = obj.entry("enabledPlugins").or_insert_with(|| serde_json::json!({}));
        if let Some(enabled_obj) = enabled.as_object_mut() {
            enabled_obj.entry(&starter_key).or_insert(serde_json::json!(true));
        }
    }

    // ... serialize and write (unchanged) ...
}
```

### 5.13 Output: Show Custom Name

**File:** `crates/aipm/src/main.rs`

Update the `MarketplaceCreated` output message to include the marketplace name when it differs from the default:

```rust
libaipm::workspace_init::InitAction::MarketplaceCreated => {
    if do_no_starter {
        format!("Created .ai/ marketplace '{}' (no starter plugin)", marketplace_name)
    } else {
        format!("Created .ai/ marketplace '{}' with starter plugin", marketplace_name)
    }
},
```

## 6. Alternatives Considered

| Option | Pros | Cons | Decision |
|--------|------|------|----------|
| Pass full `Options` struct to `ToolAdaptor::apply()` | Most extensible; future fields require no trait change | Over-couples adaptor to all init options; adaptors don't need `workspace`, `manifest`, `dir` context combined | Rejected — adds unnecessary coupling |
| Adaptor reads `marketplace.json` itself | No trait signature change | Adds file I/O coupling; adaptor needs marketplace.json path; breaks separation of concerns | Rejected |
| Add `marketplace_name` param to `apply()` | Explicit; minimal change; adaptors get exactly what they need | Requires trait signature change (one-time) | **Selected** |
| Template scaffold script at generation time | Simple string interpolation | Couples script to generation-time state; script already reads marketplace.json | Rejected in favor of runtime read |

## 7. Cross-Cutting Concerns

### 7.1 Backward Compatibility

- Default name remains `"local-repo-plugins"` — existing users running `aipm init` or `aipm init -y` see no change.
- The `--name` flag is optional and additive.
- The `migrate` command reads marketplace name dynamically from `marketplace.json` and requires no changes.

### 7.2 Validation

The `validate_marketplace_name()` function reuses the same character-set rules as `aipm-pack`'s `validate_package_name()`:
- Empty input is valid (means "use default")
- Otherwise: lowercase ASCII letters, digits, hyphens, `@`, `/`
- Rejects: uppercase, underscores, spaces, special characters

## 8. Migration, Rollout, and Testing

### 8.1 Test Plan

**Unit tests** (`crates/aipm/src/wizard.rs`):
- [ ] Snapshot: `workspace_prompt_steps` with no flags shows name prompt as step 2
- [ ] Snapshot: `workspace_prompt_steps` with `--name` flag omits name prompt
- [ ] Snapshot: `workspace_prompt_steps` with `--workspace` only (no marketplace) omits name prompt
- [ ] Snapshot: `resolve_workspace_answers` with custom name text input
- [ ] Snapshot: `resolve_workspace_answers` with empty text input → default name
- [ ] `validate_marketplace_name` accepts: `"my-plugins"`, `"@org/plugins"`, `""`, `"123abc"`
- [ ] `validate_marketplace_name` rejects: `"MyPlugins"`, `"my plugins"`, `"my_plugins"`
- [ ] `resolve_defaults` with name flag returns that name
- [ ] `resolve_defaults` without name flag returns `"local-repo-plugins"`
- [ ] Update all existing wizard snapshots for the new 4-tuple return type

**Unit tests** (`crates/libaipm/src/workspace_init/mod.rs`):
- [ ] `generate_marketplace_json("custom-name", false)` produces JSON with `"name": "custom-name"`
- [ ] `generate_marketplace_json("custom-name", true)` produces JSON with `"name": "custom-name"` and empty plugins
- [ ] `generate_marketplace_json("local-repo-plugins", false)` still produces the expected default output
- [ ] Update existing tests: `marketplace_json_with_starter_is_valid`, `marketplace_json_no_starter_has_empty_plugins`, etc. to pass the name parameter
- [ ] Scaffold script contains `marketplace.name` reference for `enabledPlugins` key
- [ ] `scaffold_script_marketplace_name_matches_generator` cross-consistency test updated

**Unit tests** (`crates/libaipm/src/workspace_init/adaptors/claude.rs`):
- [ ] Fresh settings with custom name: `extraKnownMarketplaces` uses `"custom-name"` as key
- [ ] Fresh settings with custom name + starter: `enabledPlugins` key is `"starter-aipm-plugin@custom-name"`
- [ ] Merge path: existing settings missing custom name → inserts it
- [ ] Merge path: existing settings already have custom name → returns `false`
- [ ] Update all existing adaptor tests to pass the `marketplace_name` parameter

**Integration tests** (`crates/libaipm/src/workspace_init/mod.rs`):
- [ ] `init()` with custom name: marketplace.json file contains the custom name
- [ ] `init()` with custom name: Claude settings.json references the custom name
- [ ] `init()` with default name: behavior unchanged from before

**BDD scenarios** (`tests/features/manifest/workspace-init.feature`):
- [ ] Update existing scenarios that assert `the marketplace.json name is "local-repo-plugins"` to accept the default
- [ ] Add scenario: `aipm init --marketplace --name "my-plugins"` → marketplace.json name is `"my-plugins"`

**E2E tests** (`crates/aipm/tests/init_e2e.rs`):
- [ ] Add test: `aipm init --marketplace --name "custom-mkt" -y` → marketplace.json name is `"custom-mkt"`
- [ ] Add test: `aipm init --marketplace -y` → marketplace.json name is `"local-repo-plugins"` (backward compat)
- [ ] Update existing E2E tests that parse marketplace.json for the name field

**Snapshot updates:**
- [ ] Regenerate all wizard snapshots in `crates/aipm/src/snapshots/` (new return type, new prompt step)
- [ ] Regenerate scaffold script snapshot in `crates/libaipm/src/workspace_init/snapshots/`

## 9. Open Questions / Unresolved Issues

All open questions from research have been resolved:

| Question | Decision |
|----------|----------|
| Validation rules | Same as package names: lowercase alphanumeric + hyphens + `@` + `/` |
| Default value | Keep `"local-repo-plugins"` |
| Non-interactive default | Use the default; override with `--name` |
| Scaffold script | Read name from `marketplace.json` at runtime |
| Adaptor trait | Add `marketplace_name: &str` parameter to `apply()` |
| Migrate command | No changes needed |

## References

- Research: [`research/docs/2026-03-25-marketplace-name-customization-in-init.md`](../research/docs/2026-03-25-marketplace-name-customization-in-init.md)
- Original wizard spec: [`specs/2026-03-22-interactive-init-wizard.md`](2026-03-22-interactive-init-wizard.md) (line 308 — deferred note)
- Original init spec: [`specs/2026-03-16-aipm-init-workspace-marketplace.md`](2026-03-16-aipm-init-workspace-marketplace.md)
- Reference pattern: [`crates/aipm-pack/src/wizard.rs`](https://github.com/TheLarkInn/aipm/blob/dd0ee78/crates/aipm-pack/src/wizard.rs) (Text prompt with validation)
