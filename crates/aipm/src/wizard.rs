//! Interactive wizard for `aipm` CLI commands (init, migrate, make plugin, pack init).
//!
//! Split into two layers for testability:
//! 1. **Prompt definitions** (pure functions) — build prompt configs, answer mapping.
//! 2. **Prompt execution** (thin bridge) — calls `inquire::*.prompt()`.

use std::path::Path;

use libaipm::manifest::types::PluginType;
pub use libaipm::wizard::{styled_render_config, PromptAnswer, PromptKind, PromptStep};

// =============================================================================
// Prompt definitions — fully testable, no terminal dependency
// =============================================================================

/// Setup mode select options.
const SETUP_OPTIONS: [&str; 3] =
    ["Marketplace only (recommended)", "Workspace manifest only", "Both workspace + marketplace"];

/// Engine options for the `aipm init` scaffold-set `MultiSelect` prompt.
///
/// Each tuple is `(human_label, engine_variant)`. The wizard renders the
/// labels; downstream code maps the selected indices back to
/// `libaipm::Engine` via the second element of each pair.
///
/// Distinct from [`ENGINE_OPTIONS`] (used by `aipm make plugin`) because
/// `init` uses `MultiSelect` (engine 1, 2, ..., N) rather than the
/// Single-Select-with-"Both" pattern. Order mirrors `Engine::ALL`.
pub const ENGINE_OPTIONS_INIT: &[(&str, libaipm::Engine)] =
    &[("Claude Code", libaipm::Engine::Claude), ("Copilot CLI", libaipm::Engine::Copilot)];

/// Build the list of prompts for workspace init, given pre-filled flags.
///
/// Prompts whose corresponding flag is already set are omitted.
///
/// `flag_engine_provided` controls whether the new engine-scaffold
/// `MultiSelect` prompt is emitted: pass `true` when the user supplied
/// `--engine` (CLI wins) so the wizard skips it; pass `false` to ask.
pub fn workspace_prompt_steps(
    flag_workspace: bool,
    flag_marketplace: bool,
    flag_no_starter: bool,
    flag_name: Option<&str>,
    flag_engine_provided: bool,
) -> Vec<PromptStep> {
    let mut steps = Vec::new();

    // Determine if we need to ask about setup mode.
    // If either flag is explicitly set, skip the setup-mode prompt.
    let needs_setup_prompt = !flag_workspace && !flag_marketplace;

    if needs_setup_prompt {
        steps.push(PromptStep {
            label: "What would you like to set up?",
            kind: PromptKind::Select { options: SETUP_OPTIONS.to_vec(), default_index: 0 },
            help: Some("Use arrow keys, Enter to select"),
        });
    }

    // Determine if marketplace will be created (needed to decide about starter prompt).
    // If we're showing the setup prompt, we don't know yet — but we show the confirm
    // prompt conditionally in resolve_workspace_answers. For the step list, we include
    // the confirm prompt if marketplace is possible (i.e., either the flag is set or
    // we're asking the user).
    let marketplace_possible = flag_marketplace || needs_setup_prompt;

    // Engine scaffold prompt (Spec G1 / Feature 11). Skipped when:
    //   * `--engine` was already provided on the CLI, OR
    //   * scope is workspace-only (no marketplace, no adaptors run).
    if marketplace_possible && !flag_engine_provided {
        let labels: Vec<&'static str> = ENGINE_OPTIONS_INIT.iter().map(|(l, _)| *l).collect();
        let defaults: Vec<bool> = ENGINE_OPTIONS_INIT.iter().map(|_| true).collect();
        steps.push(PromptStep {
            label: "Which engine(s) should we scaffold for this project?",
            kind: PromptKind::MultiSelect { options: labels, defaults, min_selections: 1 },
            help: Some(
                "Files will be created under each selected engine's root \
                 (.claude/, .github/copilot-instructions.md). Space to toggle, \
                 Enter to confirm. At least one engine required.",
            ),
        });

        // Engine support prompt (Spec G2 / Feature 12). Always paired with
        // the scaffold prompt above — emitted under identical conditions.
        // Defaults to all engines pre-checked (matches the "supports all"
        // baseline; manifest field is omitted when the user accepts
        // defaults). Strict "support ⊇ scaffold" enforcement happens in
        // `resolve_workspace_answers` (Feature 13); the in-prompt
        // `min_selections: 1` is a weaker safety net that prevents the
        // user from selecting nothing.
        let support_labels: Vec<&'static str> =
            ENGINE_OPTIONS_INIT.iter().map(|(l, _)| *l).collect();
        let support_defaults: Vec<bool> = ENGINE_OPTIONS_INIT.iter().map(|_| true).collect();
        steps.push(PromptStep {
            label: "Which engines does your project support?",
            kind: PromptKind::MultiSelect {
                options: support_labels,
                defaults: support_defaults,
                min_selections: 1,
            },
            help: Some(
                "Defaults to all engines (the manifest will omit the engines \
                 field). Narrow this only if your plugin is engine-specific. \
                 Must include all scaffolded engines.",
            ),
        });
    }

    // Marketplace name prompt — skip if a non-empty --name was provided or marketplace not possible.
    let has_name = flag_name.is_some_and(|s| !s.is_empty());
    if marketplace_possible && !has_name {
        steps.push(PromptStep {
            label: "Marketplace name:",
            kind: PromptKind::Text {
                placeholder: "local-repo-plugins".to_string(),
                validate: true,
            },
            help: Some("Lowercase alphanumeric with hyphens, or press Enter for default"),
        });
    }

    // Include starter prompt only if marketplace is possible AND --no-starter wasn't set.
    if marketplace_possible && !flag_no_starter {
        steps.push(PromptStep {
            label: "Include starter plugin?",
            kind: PromptKind::Confirm { default: true },
            help: Some("Adds scaffold-plugin skill, marketplace-scanner agent, and logging hook"),
        });
    }

    steps
}

/// Final resolved wizard answers consumed by `cmd_init` to construct the
/// `libaipm::workspace_init::Options`.
///
/// Replaces the legacy 4-tuple return type `(bool, bool, bool, String)`
/// from `resolve_workspace_answers`/`resolve_defaults` so the engine
/// fields can flow through alongside the existing four.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WizardAnswers {
    /// Whether to write a `[workspace]` manifest at `<dir>/aipm.toml`.
    pub workspace: bool,
    /// Whether to scaffold a `.ai/` marketplace.
    pub marketplace: bool,
    /// Whether the marketplace scaffold should skip the starter plugin.
    pub no_starter: bool,
    /// Marketplace identifier (e.g., `"local-repo-plugins"`).
    pub marketplace_name: String,
    /// Engines to scaffold for. Drives `workspace_init::Options::engines_scaffold`.
    pub engines_scaffold: libaipm::EngineSet,
    /// Engines the manifest claims to support. `None` means "supports
    /// all" — the manifest engines field is omitted on disk.
    pub engines_support: Option<libaipm::EngineSet>,
}

/// Decode a `MultiSelect` answer (selected indices into [`ENGINE_OPTIONS_INIT`])
/// into an [`libaipm::EngineSet`]. Out-of-range indices are silently
/// ignored — they cannot occur from a well-formed `inquire` flow but the
/// helper stays defensive in case `answers` were constructed by tests.
fn decode_engine_multi_select(answer: Option<&PromptAnswer>) -> libaipm::EngineSet {
    let mut set = libaipm::EngineSet::empty();
    if let Some(PromptAnswer::MultiSelected(indices)) = answer {
        for idx in indices {
            if let Some((_, engine)) = ENGINE_OPTIONS_INIT.get(*idx) {
                set |= engine.as_set();
            }
        }
    }
    set
}

/// Map raw wizard answers to a [`WizardAnswers`] value.
///
/// `answers` correspond 1:1 with the steps returned by
/// [`workspace_prompt_steps`]. `flag_engine_provided` must match the value
/// passed to that builder so the answer cursor advances past the engine
/// prompts (when emitted) and the scaffold-set comes from the right
/// source: the CLI flag (parsed elsewhere) when `flag_engine_provided` is
/// `true`, or the prompt answers otherwise.
///
/// Engine semantics:
/// - When the engine prompts were emitted, `engines_scaffold` is decoded
///   from prompt 2's answer. If the user accepted defaults (all checked),
///   the bitset is `EngineSet::ALL`.
/// - `engines_support` is decoded from prompt 3's answer. If the answer
///   equals `EngineSet::ALL` (the default), it is normalised to `None`
///   so the manifest field is omitted on disk.
/// - Strict spec G10 enforcement: support is auto-widened to be a
///   superset of scaffold (any scaffold engines missing from the user's
///   support selection are added back). This is friendlier than
///   rejecting the answer and re-prompting.
/// - When `flag_engine_provided` is `true`, the engine prompts were
///   skipped. `engines_scaffold` defaults to [`EngineSet::ALL`] here as
///   a placeholder; the caller (`cmd_init`) will overwrite it with the
///   parsed `--engine` value.
#[must_use]
pub fn resolve_workspace_answers(
    answers: &[PromptAnswer],
    flag_workspace: bool,
    flag_marketplace: bool,
    flag_no_starter: bool,
    flag_name: Option<&str>,
    flag_engine_provided: bool,
) -> WizardAnswers {
    let needs_setup_prompt = !flag_workspace && !flag_marketplace;
    let mut idx = 0;

    // Resolve setup mode
    let (do_workspace, do_marketplace) = if needs_setup_prompt {
        let result = match answers.get(idx) {
            Some(PromptAnswer::Selected(1)) => (true, false), // Workspace only
            Some(PromptAnswer::Selected(2)) => (true, true),  // Both
            _ => (false, true),                               // Marketplace only (default)
        };
        idx += 1;
        result
    } else {
        (flag_workspace, flag_marketplace)
    };

    // Decode the paired engine prompts when they were emitted.
    let marketplace_possible = flag_marketplace || needs_setup_prompt;
    let (engines_scaffold, engines_support) = if marketplace_possible && !flag_engine_provided {
        let scaffold = decode_engine_multi_select(answers.get(idx));
        idx += 1;
        let raw_support = decode_engine_multi_select(answers.get(idx));
        idx += 1;
        // Spec G10: support must be a superset of scaffold. Auto-widen
        // rather than reject — friendlier UX and matches the spec's
        // "default to all engines" intent.
        let widened_support = raw_support | scaffold;
        // Normalise "supports all" to None so the manifest field is
        // omitted on disk when the user accepted defaults.
        let support =
            if widened_support == libaipm::EngineSet::ALL { None } else { Some(widened_support) };
        (scaffold, support)
    } else {
        // Placeholder for engines_scaffold when the prompts were
        // skipped; the caller (`cmd_init`) populates it from the
        // parsed `--engine` flag.
        (libaipm::EngineSet::ALL, None)
    };

    // Resolve marketplace name
    let marketplace_name = flag_name.filter(|s| !s.is_empty()).map_or_else(
        || {
            if marketplace_possible {
                let resolved = match answers.get(idx) {
                    Some(PromptAnswer::Text(t)) if !t.is_empty() => t.clone(),
                    _ => "local-repo-plugins".to_string(),
                };
                idx += 1;
                resolved
            } else {
                "local-repo-plugins".to_string()
            }
        },
        str::to_string,
    );

    // Resolve no_starter
    let no_starter = if marketplace_possible && !flag_no_starter {
        // There was a confirm prompt
        match answers.get(idx) {
            Some(PromptAnswer::Bool(include)) if do_marketplace => !include,
            _ => false,
        }
    } else {
        flag_no_starter
    };

    WizardAnswers {
        workspace: do_workspace,
        marketplace: do_marketplace,
        no_starter,
        marketplace_name,
        engines_scaffold,
        engines_support,
    }
}

// =============================================================================
// Non-interactive defaults
// =============================================================================

/// Apply today's defaulting logic for the non-interactive path.
///
/// If neither `--workspace` nor `--marketplace` is set, default to
/// marketplace only.
///
/// `engine_flag` is the parsed list of `--engine` values from the CLI:
/// - non-empty: parsed via [`parse_engine_list`] and used as the
///   scaffold set;
/// - empty AND marketplace in scope: defaults to
///   [`libaipm::EngineSet::COPILOT`] per spec §5.2.3 / Round 6;
/// - empty AND workspace-only scope: empty bitset (no adaptors run).
///
/// `engines_support` is always `None` in headless mode — the manifest
/// engines field is omitted on disk and the user can narrow it later by
/// re-running `aipm init` interactively.
///
/// # Errors
///
/// Returns the stringly-typed error from [`parse_engine_list`] when the
/// `--engine` values include unknown engine names or empty entries.
pub fn resolve_defaults(
    workspace: bool,
    marketplace: bool,
    no_starter: bool,
    name: Option<&str>,
    engine_flag: &[String],
) -> Result<WizardAnswers, String> {
    let (w, m) = if !workspace && !marketplace { (false, true) } else { (workspace, marketplace) };
    let marketplace_name =
        name.filter(|s| !s.is_empty()).unwrap_or("local-repo-plugins").to_string();

    let engines_scaffold = if engine_flag.is_empty() {
        if m {
            // Spec G5 / Round 6: --yes mode without --engine defaults to
            // Copilot only.
            libaipm::EngineSet::COPILOT
        } else {
            libaipm::EngineSet::empty()
        }
    } else {
        parse_engine_list(engine_flag)?
    };

    Ok(WizardAnswers {
        workspace: w,
        marketplace: m,
        no_starter,
        marketplace_name,
        engines_scaffold,
        engines_support: None,
    })
}

/// Render a human-readable summary of [`WizardAnswers`] suitable for
/// printing to stderr after an interactive `aipm init` wizard run.
///
/// The summary recaps what the wizard decided so the user has a chance
/// to confirm before scaffolding starts. Format:
///
/// ```text
/// ✓ Setup mode: Marketplace only
/// ✓ Scaffold engines: Claude Code, Copilot CLI
/// ✓ Support engines: all (engines field omitted)
/// ✓ Marketplace name: local-repo-plugins
/// ✓ Include starter plugin: yes
/// ```
///
/// Pure function — no I/O. The TTY bridge ([`super::wizard_tty`]) writes
/// the result to stderr via `std::io::Write` to satisfy the workspace's
/// no-`println!` lint policy.
#[must_use]
pub fn format_wizard_summary(answers: &WizardAnswers) -> String {
    let setup_mode = match (answers.workspace, answers.marketplace) {
        (false, true) => "Marketplace only",
        (true, false) => "Workspace manifest only",
        (true, true) => "Both workspace + marketplace",
        (false, false) => "Nothing (no scope selected)",
    };

    let scaffold_engines = format_engine_set_for_summary(answers.engines_scaffold);
    let support_engines = answers
        .engines_support
        .map_or_else(|| "all (engines field omitted)".to_string(), format_engine_set_for_summary);

    let starter = if answers.no_starter { "no" } else { "yes" };

    format!(
        "\u{2713} Setup mode: {setup_mode}\n\
         \u{2713} Scaffold engines: {scaffold_engines}\n\
         \u{2713} Support engines: {support_engines}\n\
         \u{2713} Marketplace name: {marketplace_name}\n\
         \u{2713} Include starter plugin: {starter}\n",
        marketplace_name = answers.marketplace_name,
    )
}

/// Format an [`libaipm::EngineSet`] as a comma-separated list of human
/// labels (e.g., `"Claude Code, Copilot CLI"`). Empty bitsets render as
/// `"none"`.
fn format_engine_set_for_summary(set: libaipm::EngineSet) -> String {
    if set.is_empty() {
        return "none".to_string();
    }
    let names: Vec<&str> = ENGINE_OPTIONS_INIT
        .iter()
        .filter(|(_, e)| set.contains(e.as_set()))
        .map(|(l, _)| *l)
        .collect();
    names.join(", ")
}

/// Parse the values from `aipm init --engine <list>` into an `EngineSet`.
///
/// Each value is trimmed and looked up via [`libaipm::Engine::from_name`].
/// Empty input strings (e.g. from `--engine ''`) and unknown names produce
/// human-readable error messages. An empty input slice (no `--engine`
/// flag was passed) returns `Ok(EngineSet::empty())` — callers decide
/// whether that means "use defaults" or "error required".
///
/// # Errors
///
/// Returns a stringly-typed error so the caller can wrap it in any CLI
/// error variant. Errors on:
/// - any entry that is empty after trimming
/// - any entry that does not map to a known engine via `Engine::from_name`
pub fn parse_engine_list(values: &[String]) -> Result<libaipm::EngineSet, String> {
    let mut set = libaipm::EngineSet::empty();
    for raw in values {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(
                "--engine value must not be empty (e.g. `--engine claude` or `--engine claude,copilot`)"
                    .to_string(),
            );
        }
        if let Some(engine) = libaipm::Engine::from_name(trimmed) {
            set |= engine.as_set();
        } else {
            let known: Vec<&str> = libaipm::Engine::ALL.iter().map(|e| e.name()).collect();
            return Err(format!(
                "unknown engine '{trimmed}' (known engines: {known})",
                known = known.join(", ")
            ));
        }
    }
    Ok(set)
}

// =============================================================================
// Migrate cleanup prompts
// =============================================================================

/// Generates the post-migration cleanup prompt step.
///
/// Returns an empty vec if there is nothing to clean up (`migrated_count == 0`).
pub fn migrate_cleanup_prompt_steps(migrated_count: usize) -> Vec<PromptStep> {
    if migrated_count == 0 {
        return Vec::new();
    }

    vec![PromptStep {
        label: "Remove original source files that were migrated?",
        kind: PromptKind::Confirm { default: false },
        help: Some(
            "The migrated files have been copied to .ai/ plugins. \
             Answering 'yes' removes the originals from .claude/. \
             Use --destructive to skip this prompt.",
        ),
    }]
}

/// Resolves the migrate cleanup wizard answer.
///
/// Returns `true` if the user chose to remove source files.
pub const fn resolve_migrate_cleanup_answer(answers: &[PromptAnswer]) -> bool {
    matches!(answers.first(), Some(PromptAnswer::Bool(true)))
}

// =============================================================================
// Pack init prompts — `aipm pack init` (absorbed from aipm-pack)
// =============================================================================

/// Plugin type select options. Order matters — index maps to `PluginType`.
const PLUGIN_TYPE_OPTIONS: [&str; 6] = [
    "composite \u{2014} skills + agents + hooks (recommended)",
    "skill    \u{2014} single skill",
    "agent    \u{2014} autonomous agent",
    "mcp      \u{2014} Model Context Protocol server",
    "hook     \u{2014} lifecycle hook",
    "lsp      \u{2014} Language Server Protocol",
];

/// Map a select index to a `PluginType`.
const fn plugin_type_from_index(index: usize) -> Option<PluginType> {
    match index {
        0 => Some(PluginType::Composite),
        1 => Some(PluginType::Skill),
        2 => Some(PluginType::Agent),
        3 => Some(PluginType::Mcp),
        4 => Some(PluginType::Hook),
        5 => Some(PluginType::Lsp),
        _ => None,
    }
}

/// Build the list of prompts for package init, given pre-filled flags.
///
/// Prompts whose corresponding flag is already set are omitted.
pub fn package_prompt_steps(
    dir: &Path,
    flag_name: Option<&str>,
    flag_type: Option<PluginType>,
) -> Vec<PromptStep> {
    let mut steps = Vec::new();

    // Step 1: Package name (skip if --name was provided)
    if flag_name.is_none() {
        let placeholder = dir
            .file_name()
            .and_then(|n| n.to_str())
            .map_or_else(|| "my-plugin".to_string(), String::from);

        steps.push(PromptStep {
            label: "Package name:",
            kind: PromptKind::Text { placeholder, validate: true },
            help: Some("Lowercase alphanumeric with hyphens, or @org/name"),
        });
    }

    // Step 2: Description (always shown — no flag for it yet)
    steps.push(PromptStep {
        label: "Description:",
        kind: PromptKind::Text { placeholder: "An AI plugin package".to_string(), validate: false },
        help: None,
    });

    // Step 3: Plugin type (skip if --type was provided)
    if flag_type.is_none() {
        steps.push(PromptStep {
            label: "Plugin type:",
            kind: PromptKind::Select { options: PLUGIN_TYPE_OPTIONS.to_vec(), default_index: 0 },
            help: Some("Use arrow keys, Enter to select"),
        });
    }

    steps
}

/// Map raw wizard answers to final `(name, plugin_type)` values.
///
/// `answers` correspond 1:1 with the steps returned by [`package_prompt_steps`].
/// Flags that were set skip their prompt, so their answer is not in the array.
pub fn resolve_package_answers(
    answers: &[PromptAnswer],
    flag_name: Option<&str>,
    flag_type: Option<PluginType>,
) -> (Option<String>, Option<PluginType>) {
    let mut idx = 0;

    // Name
    let name = flag_name.map_or_else(
        || {
            let result = match answers.get(idx) {
                Some(PromptAnswer::Text(t)) if t.is_empty() => None,
                Some(PromptAnswer::Text(t)) => Some(t.clone()),
                _ => None,
            };
            idx += 1;
            result
        },
        |n| Some(n.to_string()),
    );

    // Description (consumed but not returned — Options has no description field)
    idx += 1;

    // Plugin type
    let plugin_type = flag_type.map_or_else(
        || match answers.get(idx) {
            Some(PromptAnswer::Selected(i)) => plugin_type_from_index(*i),
            _ => Some(PluginType::Composite),
        },
        Some,
    );

    (name, plugin_type)
}

// =============================================================================
// Make plugin prompts — compiled under test until `aipm make` wires them in
// =============================================================================

/// Engine select options for `aipm make plugin`.
pub const ENGINE_OPTIONS: &[&str] = &["Claude Code", "Copilot CLI", "Both"];

/// Build wizard prompt steps for `aipm make plugin`, skipping prompts
/// whose values are already provided via CLI flags.
#[cfg(test)]
pub fn make_plugin_prompt_steps(
    flag_name: Option<&str>,
    flag_engine: Option<&str>,
    flag_features: &[String],
    engine_feature_labels: &[&'static str],
    engine_feature_defaults: &[bool],
) -> Vec<PromptStep> {
    let mut steps = Vec::new();

    if flag_name.is_none() {
        steps.push(PromptStep {
            label: "Plugin name",
            kind: PromptKind::Text { placeholder: "my-plugin".to_string(), validate: true },
            help: Some("Lowercase, hyphens allowed"),
        });
    }

    if flag_engine.is_none() {
        steps.push(PromptStep {
            label: "Target engine",
            kind: PromptKind::Select { options: ENGINE_OPTIONS.to_vec(), default_index: 0 },
            help: Some("Which AI coding tool will this plugin target?"),
        });
    }

    if flag_features.is_empty() {
        steps.push(PromptStep {
            label: "AI features to include",
            kind: PromptKind::MultiSelect {
                options: engine_feature_labels.to_vec(),
                defaults: engine_feature_defaults.to_vec(),
                min_selections: 0,
            },
            help: Some("Select the features for your plugin"),
        });
    }

    steps
}

/// Map an engine select index to the engine CLI string.
pub const fn engine_from_index(index: usize) -> &'static str {
    match index {
        0 => "claude",
        1 => "copilot",
        _ => "both",
    }
}

/// Map raw prompt answers back to typed values for `aipm make plugin`.
///
/// Consumes answers in the same conditional order as
/// [`make_plugin_prompt_steps`], using an `idx` counter that only
/// advances for prompts that were actually shown.
#[cfg(test)]
pub fn resolve_make_plugin_answers(
    answers: &[PromptAnswer],
    flag_name: Option<&str>,
    flag_engine: Option<&str>,
    flag_features: &[String],
    feature_cli_names: &[&str],
) -> (String, String, Vec<String>) {
    let mut idx = 0;

    // Name
    let name = flag_name.map_or_else(
        || {
            let result = match answers.get(idx) {
                Some(PromptAnswer::Text(t)) => t.clone(),
                _ => String::new(),
            };
            idx += 1;
            result
        },
        str::to_string,
    );

    // Engine
    let engine = flag_engine.map_or_else(
        || {
            let result = match answers.get(idx) {
                Some(PromptAnswer::Selected(i)) => engine_from_index(*i).to_string(),
                _ => "claude".to_string(),
            };
            idx += 1;
            result
        },
        str::to_string,
    );

    // Features
    let features = if flag_features.is_empty() {
        match answers.get(idx) {
            Some(PromptAnswer::MultiSelected(indices)) => indices
                .iter()
                .filter_map(|&i| feature_cli_names.get(i).map(|s| (*s).to_string()))
                .collect(),
            _ => Vec::new(),
        }
    } else {
        flag_features.to_vec()
    };

    (name, engine, features)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize prompt steps into a human-readable string for snapshot testing.
    fn format_steps(steps: &[PromptStep]) -> String {
        let mut out = String::new();
        if steps.is_empty() {
            out.push_str("(no prompts)\n");
            return out;
        }
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
                },
                PromptKind::Confirm { default } => {
                    out.push_str(&format!("  Kind: Confirm (default: {})\n", default));
                },
                PromptKind::Text { placeholder, validate } => {
                    out.push_str(&format!("  Kind: Text (placeholder: \"{}\")\n", placeholder));
                    if *validate {
                        out.push_str("  Validate: marketplace-name\n");
                    }
                },
                PromptKind::MultiSelect { options, defaults, min_selections } => {
                    out.push_str("  Kind: MultiSelect\n");
                    if *min_selections > 0 {
                        out.push_str(&format!("  Min selections: {min_selections}\n"));
                    }
                    for (j, opt) in options.iter().enumerate() {
                        let marker =
                            if defaults.get(j).copied().unwrap_or(false) { " *" } else { "  " };
                        out.push_str(&format!("  {}[{}] {}\n", marker, j, opt));
                    }
                },
            }
            if let Some(help) = step.help {
                out.push_str(&format!("  Help: {}\n", help));
            }
            out.push('\n');
        }
        out
    }

    /// Extract the legacy 4-tuple from a [`WizardAnswers`] for terse
    /// equality assertions in tests that don't care about the engine
    /// fields.
    fn summary(a: WizardAnswers) -> (bool, bool, bool, String) {
        (a.workspace, a.marketplace, a.no_starter, a.marketplace_name)
    }

    // =========================================================================
    // Prompt step snapshots — flag combinations
    // =========================================================================

    #[test]
    fn workspace_prompts_no_flags_snapshot() {
        let steps = workspace_prompt_steps(false, false, false, None, true);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_workspace_flag_snapshot() {
        let steps = workspace_prompt_steps(true, false, false, None, true);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_marketplace_flag_snapshot() {
        let steps = workspace_prompt_steps(false, true, false, None, true);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_both_flags_snapshot() {
        let steps = workspace_prompt_steps(true, true, false, None, true);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_no_starter_flag_snapshot() {
        let steps = workspace_prompt_steps(false, true, true, None, true);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_all_flags_snapshot() {
        let steps = workspace_prompt_steps(true, true, true, None, true);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_name_flag_omits_name_prompt() {
        let steps = workspace_prompt_steps(false, false, false, Some("custom-mkt"), true);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_workspace_only_omits_name_prompt() {
        // --workspace alone means no marketplace, so no name prompt
        let steps = workspace_prompt_steps(true, false, false, None, true);
        insta::assert_snapshot!(format_steps(&steps));
    }

    // ---------- engine-scaffold MultiSelect (Feature 11 / Spec G1) ----------

    #[test]
    fn engine_prompt_emitted_when_engine_flag_absent_and_marketplace_in_scope() {
        // No flags + no --engine → setup prompt + engine prompt + name + starter.
        let steps = workspace_prompt_steps(false, false, false, None, false);
        // Find the engine prompt by label.
        let engine_step = steps
            .iter()
            .find(|s| s.label == "Which engine(s) should we scaffold for this project?");
        assert!(engine_step.is_some(), "engine prompt should be emitted");
    }

    #[test]
    fn engine_prompt_skipped_when_engine_flag_provided() {
        // CLI provided --engine → skip the prompt.
        let steps = workspace_prompt_steps(false, false, false, None, true);
        let engine_step = steps
            .iter()
            .find(|s| s.label == "Which engine(s) should we scaffold for this project?");
        assert!(engine_step.is_none(), "engine prompt should be skipped when --engine provided");
    }

    #[test]
    fn engine_prompt_skipped_for_workspace_only_scope() {
        // --workspace alone (no marketplace) → no adaptors run, no engine prompt.
        let steps = workspace_prompt_steps(true, false, false, None, false);
        let engine_step = steps
            .iter()
            .find(|s| s.label == "Which engine(s) should we scaffold for this project?");
        assert!(engine_step.is_none(), "engine prompt should be skipped in workspace-only scope");
    }

    #[test]
    fn engine_prompt_emitted_for_marketplace_only_scope() {
        // --marketplace alone (no workspace) → engine prompt should appear.
        let steps = workspace_prompt_steps(false, true, false, None, false);
        let engine_step = steps
            .iter()
            .find(|s| s.label == "Which engine(s) should we scaffold for this project?");
        assert!(engine_step.is_some(), "engine prompt should appear in marketplace-only scope");
    }

    #[test]
    fn engine_prompt_kind_and_defaults() {
        let steps = workspace_prompt_steps(false, true, false, None, false);
        let engine_step = steps
            .iter()
            .find(|s| s.label == "Which engine(s) should we scaffold for this project?")
            .expect("engine prompt should be present");

        match &engine_step.kind {
            PromptKind::MultiSelect { options, defaults, min_selections } => {
                assert_eq!(options.len(), ENGINE_OPTIONS_INIT.len(), "options count");
                assert_eq!(options[0], "Claude Code");
                assert_eq!(options[1], "Copilot CLI");
                assert!(defaults.iter().all(|d| *d), "all engines pre-checked by default");
                assert_eq!(*min_selections, 1, "min_selections must be 1 (required)");
            },
            other => panic!("engine prompt kind should be MultiSelect, got {other:?}"),
        }
    }

    #[test]
    fn engine_prompt_help_text_mentions_engine_root_paths() {
        let steps = workspace_prompt_steps(false, true, false, None, false);
        let engine_step = steps
            .iter()
            .find(|s| s.label == "Which engine(s) should we scaffold for this project?")
            .expect("engine prompt should be present");

        let help = engine_step.help.unwrap_or("");
        assert!(help.contains(".claude/"), "help should mention .claude/: {help}");
        assert!(
            help.contains("copilot-instructions.md"),
            "help should mention copilot file: {help}"
        );
        assert!(
            help.contains("at least one") || help.contains("required"),
            "help should mention min-1 requirement: {help}"
        );
    }

    // ---------- engine-support MultiSelect (Feature 12 / Spec G2) ----------

    #[test]
    fn support_prompt_emitted_alongside_scaffold_prompt() {
        // The two engine prompts are paired — both are emitted under the
        // same conditions (marketplace in scope AND --engine not given).
        let steps = workspace_prompt_steps(false, false, false, None, false);
        let support = steps.iter().find(|s| s.label == "Which engines does your project support?");
        assert!(support.is_some(), "support prompt should be emitted alongside scaffold prompt");
    }

    #[test]
    fn support_prompt_skipped_when_engine_flag_provided() {
        let steps = workspace_prompt_steps(false, false, false, None, true);
        let support = steps.iter().find(|s| s.label == "Which engines does your project support?");
        assert!(support.is_none(), "support prompt should be skipped when --engine provided");
    }

    #[test]
    fn support_prompt_skipped_for_workspace_only_scope() {
        let steps = workspace_prompt_steps(true, false, false, None, false);
        let support = steps.iter().find(|s| s.label == "Which engines does your project support?");
        assert!(support.is_none(), "support prompt should be skipped in workspace-only scope");
    }

    #[test]
    fn support_prompt_kind_and_defaults() {
        let steps = workspace_prompt_steps(false, true, false, None, false);
        let support = steps
            .iter()
            .find(|s| s.label == "Which engines does your project support?")
            .expect("support prompt should be present");

        match &support.kind {
            PromptKind::MultiSelect { options, defaults, min_selections } => {
                assert_eq!(options.len(), ENGINE_OPTIONS_INIT.len(), "options count");
                assert_eq!(options[0], "Claude Code");
                assert_eq!(options[1], "Copilot CLI");
                assert!(defaults.iter().all(|d| *d), "all engines pre-checked by default");
                assert_eq!(*min_selections, 1, "min_selections must be 1");
            },
            other => panic!("support prompt kind should be MultiSelect, got {other:?}"),
        }
    }

    #[test]
    fn support_prompt_appears_after_scaffold_prompt() {
        // Order matters: scaffold is asked first (the user must pick at
        // least one engine to actually create), then support is asked
        // (defaults to all). resolve_workspace_answers depends on this
        // ordering for index advancement.
        let steps = workspace_prompt_steps(false, false, false, None, false);

        let scaffold_idx = steps
            .iter()
            .position(|s| s.label == "Which engine(s) should we scaffold for this project?")
            .expect("scaffold prompt should be present");
        let support_idx = steps
            .iter()
            .position(|s| s.label == "Which engines does your project support?")
            .expect("support prompt should be present");

        assert!(
            support_idx == scaffold_idx + 1,
            "support prompt should immediately follow scaffold prompt: scaffold={scaffold_idx} support={support_idx}"
        );
    }

    #[test]
    fn support_prompt_help_describes_default_and_constraint() {
        let steps = workspace_prompt_steps(false, true, false, None, false);
        let support = steps
            .iter()
            .find(|s| s.label == "Which engines does your project support?")
            .expect("support prompt should be present");
        let help = support.help.unwrap_or("");
        assert!(help.contains("all engines"), "help should explain default = all: {help}");
        assert!(
            help.contains("scaffolded"),
            "help should mention superset-of-scaffold constraint: {help}"
        );
    }

    #[test]
    fn engine_options_init_constant_pairs_labels_with_engine_variants() {
        // ENGINE_OPTIONS_INIT must contain exactly the engines the spec
        // supports today (Claude + Copilot) and pair human-readable
        // labels with the underlying Engine variant.
        assert_eq!(ENGINE_OPTIONS_INIT.len(), 2, "expected 2 engines today");
        assert_eq!(ENGINE_OPTIONS_INIT[0].0, "Claude Code");
        assert_eq!(ENGINE_OPTIONS_INIT[0].1, libaipm::Engine::Claude);
        assert_eq!(ENGINE_OPTIONS_INIT[1].0, "Copilot CLI");
        assert_eq!(ENGINE_OPTIONS_INIT[1].1, libaipm::Engine::Copilot);
    }

    // =========================================================================
    // Answer resolution snapshots
    // =========================================================================

    #[test]
    fn resolve_workspace_marketplace_only_snapshot() {
        let answers = vec![
            PromptAnswer::Selected(0),
            PromptAnswer::Text(String::new()),
            PromptAnswer::Bool(true),
        ];
        let result = resolve_workspace_answers(&answers, false, false, false, None, true);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_manifest_only_snapshot() {
        // Selecting "Workspace only" — confirm prompt is shown but ignored (no_starter
        // stays false because do_marketplace resolved to false)
        let answers = vec![
            PromptAnswer::Selected(1),
            PromptAnswer::Text(String::new()),
            PromptAnswer::Bool(true),
        ];
        let result = resolve_workspace_answers(&answers, false, false, false, None, true);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_both_snapshot() {
        let answers = vec![
            PromptAnswer::Selected(2),
            PromptAnswer::Text(String::new()),
            PromptAnswer::Bool(true),
        ];
        let result = resolve_workspace_answers(&answers, false, false, false, None, true);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_decline_starter_snapshot() {
        let answers = vec![
            PromptAnswer::Selected(0),
            PromptAnswer::Text(String::new()),
            PromptAnswer::Bool(false),
        ];
        let result = resolve_workspace_answers(&answers, false, false, false, None, true);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_flags_bypass_snapshot() {
        // Both flags set — name + confirm prompts shown (setup skipped)
        let answers = vec![PromptAnswer::Text(String::new()), PromptAnswer::Bool(true)];
        let result = resolve_workspace_answers(&answers, true, true, false, None, true);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_all_flags_no_prompts_snapshot() {
        let answers: Vec<PromptAnswer> = vec![];
        let result = resolve_workspace_answers(&answers, true, true, true, Some("my-mkt"), true);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_workspace_only_skips_marketplace_prompt() {
        // flag_workspace=true, flag_marketplace=false → marketplace_possible=false.
        // Covers the False branches of:
        //   - "if marketplace_possible" (name resolution skipped, uses default)
        //   - "if marketplace_possible && !flag_no_starter" (starter prompt skipped)
        let answers: Vec<PromptAnswer> = vec![];
        let result = resolve_workspace_answers(&answers, true, false, false, None, true);
        assert_eq!(summary(result), (true, false, false, "local-repo-plugins".to_string()));
    }

    #[test]
    fn resolve_workspace_custom_name_snapshot() {
        let answers = vec![
            PromptAnswer::Selected(0),
            PromptAnswer::Text("my-custom-plugins".to_string()),
            PromptAnswer::Bool(true),
        ];
        let result = resolve_workspace_answers(&answers, false, false, false, None, true);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_empty_name_uses_default_snapshot() {
        let answers = vec![
            PromptAnswer::Selected(0),
            PromptAnswer::Text(String::new()),
            PromptAnswer::Bool(true),
        ];
        let result = resolve_workspace_answers(&answers, false, false, false, None, true);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_name_flag_snapshot() {
        // --name flag provided — name prompt skipped, only setup + confirm
        let answers = vec![PromptAnswer::Selected(0), PromptAnswer::Bool(true)];
        let result =
            resolve_workspace_answers(&answers, false, false, false, Some("preset-mkt"), true);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    // =========================================================================
    // Theming snapshot
    // =========================================================================

    #[test]
    fn styled_render_config_snapshot() {
        let config = styled_render_config();
        let summary = format!(
            "prompt_prefix: {:?}\nanswered_prompt_prefix: {:?}\nplaceholder: {:?}",
            config.prompt_prefix, config.answered_prompt_prefix, config.placeholder,
        );
        insta::assert_snapshot!(summary);
    }

    // =========================================================================
    // resolve_defaults
    // =========================================================================

    #[test]
    fn resolve_defaults_no_flags() {
        // Neither flag → marketplace only, default name
        assert_eq!(
            summary(resolve_defaults(false, false, false, None, &[]).expect("ok")),
            (false, true, false, "local-repo-plugins".to_string())
        );
    }

    #[test]
    fn resolve_defaults_workspace_only() {
        assert_eq!(
            summary(resolve_defaults(true, false, false, None, &[]).expect("ok")),
            (true, false, false, "local-repo-plugins".to_string())
        );
    }

    #[test]
    fn resolve_defaults_both_flags() {
        assert_eq!(
            summary(resolve_defaults(true, true, false, None, &[]).expect("ok")),
            (true, true, false, "local-repo-plugins".to_string())
        );
    }

    #[test]
    fn resolve_defaults_no_starter() {
        assert_eq!(
            summary(resolve_defaults(false, false, true, None, &[]).expect("ok")),
            (false, true, true, "local-repo-plugins".to_string())
        );
    }

    #[test]
    fn resolve_defaults_with_name() {
        assert_eq!(
            summary(resolve_defaults(false, false, false, Some("custom-mkt"), &[]).expect("ok")),
            (false, true, false, "custom-mkt".to_string())
        );
    }

    // =========================================================================
    // validate_marketplace_name (now delegates to shared validator)
    // =========================================================================

    fn validate_name_interactive(input: &str) -> Result<(), String> {
        libaipm::manifest::validate::check_name(
            input,
            libaipm::manifest::validate::ValidationMode::Interactive,
        )
    }

    #[test]
    fn validate_marketplace_name_accepts_lowercase() {
        assert!(validate_name_interactive("my-plugins").is_ok());
    }

    #[test]
    fn validate_marketplace_name_accepts_scoped() {
        assert!(validate_name_interactive("@org/plugins").is_ok());
    }

    #[test]
    fn validate_marketplace_name_accepts_empty_for_default() {
        assert!(validate_name_interactive("").is_ok());
    }

    #[test]
    fn validate_marketplace_name_accepts_digits() {
        assert!(validate_name_interactive("123abc").is_ok());
    }

    #[test]
    fn validate_marketplace_name_rejects_uppercase() {
        assert!(validate_name_interactive("MyPlugins").is_err());
    }

    #[test]
    fn validate_marketplace_name_rejects_spaces() {
        assert!(validate_name_interactive("my plugins").is_err());
    }

    #[test]
    fn validate_marketplace_name_rejects_underscores() {
        assert!(validate_name_interactive("my_plugins").is_err());
    }

    // =========================================================================
    // Migrate cleanup prompt steps
    // =========================================================================

    #[test]
    fn migrate_cleanup_zero_migrated_snapshot() {
        let steps = migrate_cleanup_prompt_steps(0);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn migrate_cleanup_with_migrated_snapshot() {
        let steps = migrate_cleanup_prompt_steps(3);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn resolve_migrate_cleanup_answer_true() {
        assert!(resolve_migrate_cleanup_answer(&[PromptAnswer::Bool(true)]));
    }

    #[test]
    fn resolve_migrate_cleanup_answer_false() {
        assert!(!resolve_migrate_cleanup_answer(&[PromptAnswer::Bool(false)]));
    }

    #[test]
    fn resolve_migrate_cleanup_answer_empty() {
        assert!(!resolve_migrate_cleanup_answer(&[]));
    }

    #[test]
    fn format_steps_text_validate_false_and_no_help() {
        // Covers: `if *validate` false branch (Text step with validate=false)
        // and `if let Some(help)` None branch (step without help).
        let steps = vec![PromptStep {
            label: "test step",
            kind: PromptKind::Text { placeholder: "placeholder".to_string(), validate: false },
            help: None,
        }];
        let output = format_steps(&steps);
        assert!(!output.contains("Validate: marketplace-name"));
        assert!(!output.contains("Help:"));
        assert!(output.contains("placeholder: \"placeholder\""));
    }

    // =========================================================================
    // Pack init prompt steps (absorbed from aipm-pack)
    // =========================================================================

    #[test]
    fn package_prompts_no_flags_snapshot() {
        let dir = std::path::Path::new("/projects/my-cool-project");
        let steps = package_prompt_steps(dir, None, None);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn package_prompts_name_flag_snapshot() {
        let dir = std::path::Path::new("/projects/my-cool-project");
        let steps = package_prompt_steps(dir, Some("custom-name"), None);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn package_prompts_type_flag_snapshot() {
        let dir = std::path::Path::new("/projects/my-cool-project");
        let steps = package_prompt_steps(dir, None, Some(PluginType::Skill));
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn package_prompts_all_flags_snapshot() {
        let dir = std::path::Path::new("/projects/my-cool-project");
        let steps = package_prompt_steps(dir, Some("foo"), Some(PluginType::Mcp));
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn package_prompts_placeholder_uses_dir_name() {
        let dir = std::path::Path::new("/projects/my-cool-project");
        let steps = package_prompt_steps(dir, None, None);
        let name_step = &steps[0];
        match &name_step.kind {
            PromptKind::Text { placeholder, .. } => {
                assert_eq!(placeholder, "my-cool-project");
            },
            other => {
                panic!("expected Text prompt for package name, got {other:?}");
            },
        }
    }

    #[test]
    fn resolve_package_defaults_snapshot() {
        let answers = vec![
            PromptAnswer::Text(String::new()), // empty = use placeholder
            PromptAnswer::Text(String::new()), // empty description
            PromptAnswer::Selected(0),         // composite
        ];
        let result = resolve_package_answers(&answers, None, None);
        insta::assert_snapshot!(format!("{result:?}"));
    }

    #[test]
    fn resolve_package_custom_name_snapshot() {
        let answers = vec![
            PromptAnswer::Text("my-plugin".to_string()),
            PromptAnswer::Text("A cool plugin".to_string()),
            PromptAnswer::Selected(1), // skill
        ];
        let result = resolve_package_answers(&answers, None, None);
        insta::assert_snapshot!(format!("{result:?}"));
    }

    #[test]
    fn resolve_package_each_type_snapshot() {
        let types = ["Composite", "Skill", "Agent", "Mcp", "Hook", "Lsp"];
        let mut out = String::new();
        for (i, label) in types.iter().enumerate() {
            let answers = vec![
                PromptAnswer::Text(String::new()),
                PromptAnswer::Text(String::new()),
                PromptAnswer::Selected(i),
            ];
            let (_, pt) = resolve_package_answers(&answers, None, None);
            out.push_str(&format!("index {i} -> {pt:?} (expected {label})\n"));
        }
        insta::assert_snapshot!(out);
    }

    #[test]
    fn resolve_package_with_name_flag_snapshot() {
        let answers = vec![
            PromptAnswer::Text(String::new()), // description
            PromptAnswer::Selected(2),         // agent
        ];
        let result = resolve_package_answers(&answers, Some("preset-name"), None);
        insta::assert_snapshot!(format!("{result:?}"));
    }

    #[test]
    fn resolve_package_with_type_flag_snapshot() {
        let answers =
            vec![PromptAnswer::Text("custom".to_string()), PromptAnswer::Text(String::new())];
        let result = resolve_package_answers(&answers, None, Some(PluginType::Agent));
        insta::assert_snapshot!(format!("{result:?}"));
    }

    #[test]
    fn resolve_package_with_both_flags_snapshot() {
        let answers = vec![PromptAnswer::Text("desc".to_string())];
        let result = resolve_package_answers(&answers, Some("preset"), Some(PluginType::Hook));
        insta::assert_snapshot!(format!("{result:?}"));
    }

    #[test]
    fn plugin_type_from_index_out_of_range() {
        assert!(plugin_type_from_index(6).is_none());
        assert!(plugin_type_from_index(100).is_none());
    }

    #[test]
    fn plugin_type_from_index_all_valid() {
        assert_eq!(plugin_type_from_index(0), Some(PluginType::Composite));
        assert_eq!(plugin_type_from_index(1), Some(PluginType::Skill));
        assert_eq!(plugin_type_from_index(2), Some(PluginType::Agent));
        assert_eq!(plugin_type_from_index(3), Some(PluginType::Mcp));
        assert_eq!(plugin_type_from_index(4), Some(PluginType::Hook));
        assert_eq!(plugin_type_from_index(5), Some(PluginType::Lsp));
    }

    // =========================================================================
    // Make plugin prompt steps
    // =========================================================================

    #[test]
    fn make_plugin_steps_all_flags_set() {
        let steps = make_plugin_prompt_steps(
            Some("foo"),
            Some("claude"),
            &["skill".to_string()],
            &["Skills"],
            &[true],
        );
        assert!(steps.is_empty(), "all flags set = no prompts");
    }

    #[test]
    fn make_plugin_steps_no_flags() {
        let steps =
            make_plugin_prompt_steps(None, None, &[], &["Skills", "Agents"], &[true, false]);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].label, "Plugin name");
        assert_eq!(steps[1].label, "Target engine");
        assert_eq!(steps[2].label, "AI features to include");
    }

    #[test]
    fn make_plugin_steps_name_only() {
        let steps = make_plugin_prompt_steps(Some("already-set"), None, &[], &["Skills"], &[true]);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].label, "Target engine");
        assert_eq!(steps[1].label, "AI features to include");
    }

    #[test]
    fn resolve_make_plugin_from_flags() {
        let (name, engine, features) = resolve_make_plugin_answers(
            &[],
            Some("my-plugin"),
            Some("copilot"),
            &["skill".to_string(), "agent".to_string()],
            &[],
        );
        assert_eq!(name, "my-plugin");
        assert_eq!(engine, "copilot");
        assert_eq!(features, vec!["skill", "agent"]);
    }

    #[test]
    fn resolve_make_plugin_from_answers() {
        let answers = vec![
            PromptAnswer::Text("test-plug".to_string()),
            PromptAnswer::Selected(1), // Copilot
            PromptAnswer::MultiSelected(vec![0, 2]),
        ];
        let cli_names = &["skill", "agent", "mcp"];
        let (name, engine, features) =
            resolve_make_plugin_answers(&answers, None, None, &[], cli_names);
        assert_eq!(name, "test-plug");
        assert_eq!(engine, "copilot");
        assert_eq!(features, vec!["skill", "mcp"]);
    }

    #[test]
    fn resolve_make_plugin_engine_both() {
        let answers = vec![
            PromptAnswer::Selected(2), // Both
        ];
        let (_, engine, _) =
            resolve_make_plugin_answers(&answers, Some("x"), None, &["skill".to_string()], &[]);
        assert_eq!(engine, "both");
    }

    #[test]
    fn resolve_defaults_marketplace_only() {
        // workspace=false, marketplace=true: takes the else branch directly, covering
        // the False branch of `!marketplace` in `if !workspace && !marketplace`.
        assert_eq!(
            summary(resolve_defaults(false, true, false, None, &[]).expect("ok")),
            (false, true, false, "local-repo-plugins".to_string())
        );
    }

    // ---------- engine-aware resolve_* (Feature 13) ----------

    #[test]
    fn resolve_defaults_marketplace_no_engine_flag_defaults_to_copilot() {
        // Spec G5 / Round 6: --yes mode without --engine in marketplace
        // scope defaults to Copilot only.
        let result = resolve_defaults(false, false, false, None, &[]).expect("ok");
        assert_eq!(result.engines_scaffold, libaipm::EngineSet::COPILOT);
        assert_eq!(result.engines_support, None);
    }

    #[test]
    fn resolve_defaults_workspace_only_no_engine_flag_returns_empty_set() {
        // workspace-only scope = no marketplace = no adaptors = empty
        // scaffold set.
        let result = resolve_defaults(true, false, false, None, &[]).expect("ok");
        assert_eq!(result.engines_scaffold, libaipm::EngineSet::empty());
        assert_eq!(result.engines_support, None);
    }

    #[test]
    fn resolve_defaults_with_engine_flag_uses_parsed_set() {
        let result =
            resolve_defaults(false, false, false, None, &["claude".to_string()]).expect("ok");
        assert_eq!(result.engines_scaffold, libaipm::EngineSet::CLAUDE);
    }

    #[test]
    fn resolve_defaults_with_multi_engine_flag() {
        let engine = vec!["claude".to_string(), "copilot".to_string()];
        let result = resolve_defaults(false, false, false, None, &engine).expect("ok");
        assert_eq!(result.engines_scaffold, libaipm::EngineSet::ALL);
    }

    #[test]
    fn resolve_defaults_unknown_engine_flag_errors() {
        let err = resolve_defaults(false, false, false, None, &["gemini".to_string()])
            .expect_err("unknown engine should error");
        assert!(err.contains("unknown engine 'gemini'"), "unexpected error: {err}");
    }

    #[test]
    fn resolve_workspace_decodes_engine_scaffold_from_multiselect() {
        // Setup answer (marketplace only) + scaffold MultiSelect
        // (Claude only) + support MultiSelect (all) + name + starter.
        let answers = vec![
            PromptAnswer::Selected(0),               // marketplace only
            PromptAnswer::MultiSelected(vec![0]),    // scaffold = Claude
            PromptAnswer::MultiSelected(vec![0, 1]), // support = ALL
            PromptAnswer::Text("local-repo-plugins".to_string()),
            PromptAnswer::Bool(true),
        ];
        let result = resolve_workspace_answers(&answers, false, false, false, None, false);
        assert_eq!(result.engines_scaffold, libaipm::EngineSet::CLAUDE);
        assert_eq!(result.engines_support, None, "support=ALL should normalise to None");
    }

    #[test]
    fn resolve_workspace_decodes_engine_support_narrower_than_all() {
        let answers = vec![
            PromptAnswer::Selected(0),            // marketplace only
            PromptAnswer::MultiSelected(vec![0]), // scaffold = Claude
            PromptAnswer::MultiSelected(vec![0]), // support = Claude only (narrower)
            PromptAnswer::Text("local-repo-plugins".to_string()),
            PromptAnswer::Bool(true),
        ];
        let result = resolve_workspace_answers(&answers, false, false, false, None, false);
        assert_eq!(result.engines_scaffold, libaipm::EngineSet::CLAUDE);
        assert_eq!(result.engines_support, Some(libaipm::EngineSet::CLAUDE));
    }

    #[test]
    fn resolve_workspace_auto_widens_support_to_superset_of_scaffold() {
        // Spec G10: if user picks scaffold=[claude,copilot] but support=[claude]
        // (would violate superset), auto-widen support to include scaffold.
        let answers = vec![
            PromptAnswer::Selected(0),               // marketplace only
            PromptAnswer::MultiSelected(vec![0, 1]), // scaffold = ALL
            PromptAnswer::MultiSelected(vec![0]),    // support = Claude only (violates superset)
            PromptAnswer::Text("local-repo-plugins".to_string()),
            PromptAnswer::Bool(true),
        ];
        let result = resolve_workspace_answers(&answers, false, false, false, None, false);
        assert_eq!(result.engines_scaffold, libaipm::EngineSet::ALL);
        // Support auto-widened to ALL (= scaffold | original support).
        // Then normalised to None because it equals EngineSet::ALL.
        assert_eq!(result.engines_support, None);
    }

    #[test]
    fn resolve_workspace_engine_flag_provided_skips_decoding() {
        // When `flag_engine_provided` is true, the wizard skipped both
        // engine prompts. The placeholder `EngineSet::ALL` is returned;
        // the caller (cmd_init) overrides with the parsed flag.
        let answers = vec![
            PromptAnswer::Selected(0), // marketplace only
            PromptAnswer::Text("local-repo-plugins".to_string()),
            PromptAnswer::Bool(true),
        ];
        let result = resolve_workspace_answers(&answers, false, false, false, None, true);
        assert_eq!(result.engines_scaffold, libaipm::EngineSet::ALL);
        assert_eq!(result.engines_support, None);
    }

    // ---------- format_wizard_summary (Feature 14 / Spec §5.2.4) ----------

    fn make_test_answers() -> WizardAnswers {
        WizardAnswers {
            workspace: false,
            marketplace: true,
            no_starter: false,
            marketplace_name: "local-repo-plugins".to_string(),
            engines_scaffold: libaipm::EngineSet::CLAUDE | libaipm::EngineSet::COPILOT,
            engines_support: None,
        }
    }

    #[test]
    fn format_wizard_summary_marketplace_only_with_all_engines() {
        let summary = format_wizard_summary(&make_test_answers());
        assert!(summary.contains("Setup mode: Marketplace only"));
        assert!(summary.contains("Scaffold engines: Claude Code, Copilot CLI"));
        assert!(summary.contains("Support engines: all (engines field omitted)"));
        assert!(summary.contains("Marketplace name: local-repo-plugins"));
        assert!(summary.contains("Include starter plugin: yes"));
    }

    #[test]
    fn format_wizard_summary_workspace_only_setup_mode() {
        let answers = WizardAnswers { workspace: true, marketplace: false, ..make_test_answers() };
        let summary = format_wizard_summary(&answers);
        assert!(summary.contains("Setup mode: Workspace manifest only"));
    }

    #[test]
    fn format_wizard_summary_both_setup_mode() {
        let answers = WizardAnswers { workspace: true, marketplace: true, ..make_test_answers() };
        let summary = format_wizard_summary(&answers);
        assert!(summary.contains("Setup mode: Both workspace + marketplace"));
    }

    #[test]
    fn format_wizard_summary_no_starter_renders_no() {
        let answers = WizardAnswers { no_starter: true, ..make_test_answers() };
        let summary = format_wizard_summary(&answers);
        assert!(summary.contains("Include starter plugin: no"));
    }

    #[test]
    fn format_wizard_summary_narrowed_support_renders_engine_list() {
        let answers = WizardAnswers {
            engines_support: Some(libaipm::EngineSet::CLAUDE),
            ..make_test_answers()
        };
        let summary = format_wizard_summary(&answers);
        assert!(summary.contains("Support engines: Claude Code"));
        assert!(!summary.contains("all (engines field omitted)"));
    }

    #[test]
    fn format_wizard_summary_empty_scaffold_renders_none() {
        let answers =
            WizardAnswers { engines_scaffold: libaipm::EngineSet::empty(), ..make_test_answers() };
        let summary = format_wizard_summary(&answers);
        assert!(summary.contains("Scaffold engines: none"));
    }

    #[test]
    fn format_wizard_summary_uses_check_marker() {
        // Spec §5.2.4 example uses ✓ U+2713 markers.
        let summary = format_wizard_summary(&make_test_answers());
        assert!(summary.contains('\u{2713}'), "summary should use ✓ markers: {summary}");
    }

    #[test]
    fn format_wizard_summary_copilot_only_scaffold() {
        let answers =
            WizardAnswers { engines_scaffold: libaipm::EngineSet::COPILOT, ..make_test_answers() };
        let summary = format_wizard_summary(&answers);
        assert!(summary.contains("Scaffold engines: Copilot CLI"));
        assert!(!summary.contains("Claude"));
    }

    // ---------- parse_engine_list (Feature 10 / Spec G4) ----------

    #[test]
    fn parse_engine_list_empty_input_returns_empty_set() {
        // Empty Vec means `--engine` was not passed; the helper does not
        // error so callers can decide what to do (use defaults or require
        // explicit selection).
        let values: Vec<String> = Vec::new();
        let result = parse_engine_list(&values);
        assert_eq!(result, Ok(libaipm::EngineSet::empty()));
    }

    #[test]
    fn parse_engine_list_single_claude() {
        let values = vec!["claude".to_string()];
        let result = parse_engine_list(&values);
        assert_eq!(result, Ok(libaipm::EngineSet::CLAUDE));
    }

    #[test]
    fn parse_engine_list_single_copilot() {
        let values = vec!["copilot".to_string()];
        let result = parse_engine_list(&values);
        assert_eq!(result, Ok(libaipm::EngineSet::COPILOT));
    }

    #[test]
    fn parse_engine_list_multi_value() {
        let values = vec!["claude".to_string(), "copilot".to_string()];
        let result = parse_engine_list(&values);
        assert_eq!(result, Ok(libaipm::EngineSet::CLAUDE | libaipm::EngineSet::COPILOT));
    }

    #[test]
    fn parse_engine_list_trims_whitespace() {
        let values = vec!["  claude  ".to_string()];
        let result = parse_engine_list(&values);
        assert_eq!(result, Ok(libaipm::EngineSet::CLAUDE));
    }

    #[test]
    fn parse_engine_list_empty_string_errors() {
        let values = vec![String::new()];
        let err = parse_engine_list(&values).expect_err("empty string should error");
        assert!(err.contains("must not be empty"), "unexpected error: {err}");
    }

    #[test]
    fn parse_engine_list_whitespace_only_errors() {
        let values = vec!["   ".to_string()];
        let err = parse_engine_list(&values).expect_err("whitespace-only should error");
        assert!(err.contains("must not be empty"), "unexpected error: {err}");
    }

    #[test]
    fn parse_engine_list_unknown_engine_errors() {
        let values = vec!["gemini".to_string()];
        let err = parse_engine_list(&values).expect_err("unknown engine should error");
        assert!(err.contains("unknown engine 'gemini'"), "unexpected error: {err}");
        assert!(err.contains("known engines:"), "error should list known engines: {err}");
        assert!(err.contains("claude"), "error should mention claude: {err}");
        assert!(err.contains("copilot"), "error should mention copilot: {err}");
    }

    #[test]
    fn parse_engine_list_legacy_copilot_cli_form_rejected() {
        // The legacy "copilot-cli" name was renamed to "copilot" in
        // feature 1; the parser must not accept it.
        let values = vec!["copilot-cli".to_string()];
        let err = parse_engine_list(&values).expect_err("legacy form should error");
        assert!(err.contains("unknown engine 'copilot-cli'"), "unexpected error: {err}");
    }

    #[test]
    fn parse_engine_list_mixed_known_and_unknown_errors_on_first_unknown() {
        let values = vec!["claude".to_string(), "gemini".to_string()];
        let err = parse_engine_list(&values).expect_err("mixed list with unknown should error");
        assert!(err.contains("unknown engine 'gemini'"), "unexpected error: {err}");
    }

    #[test]
    fn format_steps_multi_select_shows_markers() {
        // Covers the PromptKind::MultiSelect arm in format_steps(), including both the
        // True branch (default=true → " *" marker) and the False branch (default=false).
        let steps = vec![PromptStep {
            label: "Choose features",
            kind: PromptKind::MultiSelect {
                options: vec!["Skills", "Agents", "MCP"],
                defaults: vec![true, false, true],
                min_selections: 0,
            },
            help: None,
        }];
        let output = format_steps(&steps);
        assert!(output.contains("Kind: MultiSelect"), "expected MultiSelect kind label");
        assert!(output.contains(" *[0] Skills"), "index 0 should be pre-selected");
        assert!(output.contains("  [1] Agents"), "index 1 should not be pre-selected");
        assert!(output.contains(" *[2] MCP"), "index 2 should be pre-selected");
    }

    // =========================================================================
    // format_steps — min_selections branch coverage
    // =========================================================================

    /// Covers the `if *min_selections > 0` True branch in `format_steps`.
    ///
    /// The existing `format_steps_multi_select_shows_markers` test uses
    /// `min_selections: 0`, exercising only the False branch.  This test
    /// passes `min_selections: 1` so the "Min selections: N" line is emitted,
    /// covering the True branch.
    #[test]
    fn format_steps_multi_select_shows_min_selections_when_nonzero() {
        let steps = vec![PromptStep {
            label: "Choose engines",
            kind: PromptKind::MultiSelect {
                options: vec!["Claude", "Copilot"],
                defaults: vec![true, false],
                min_selections: 1,
            },
            help: None,
        }];
        let output = format_steps(&steps);
        assert!(
            output.contains("Min selections: 1"),
            "min_selections > 0 should produce 'Min selections: N' line; got:\n{output}"
        );
    }

    // =========================================================================
    // decode_engine_multi_select — defensive branch coverage
    // =========================================================================

    #[test]
    fn decode_engine_multi_select_none_returns_empty_set() {
        // Covers the false branch of `if let Some(PromptAnswer::MultiSelected(...)) = answer`
        // when `answer` is `None` — the helper must return an empty EngineSet.
        let result = decode_engine_multi_select(None);
        assert!(result.is_empty(), "None answer should yield empty EngineSet");
    }

    #[test]
    fn decode_engine_multi_select_out_of_range_index_silently_ignored() {
        // Covers the false branch of `if let Some((_, engine)) = ENGINE_OPTIONS_INIT.get(*idx)`
        // when an index exceeds the bounds of ENGINE_OPTIONS_INIT.
        let out_of_range = ENGINE_OPTIONS_INIT.len(); // one past the last valid index
        let answer = PromptAnswer::MultiSelected(vec![out_of_range]);
        let result = decode_engine_multi_select(Some(&answer));
        assert!(result.is_empty(), "out-of-range index should be silently ignored → empty set");
    }
}
