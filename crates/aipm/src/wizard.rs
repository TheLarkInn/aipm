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

/// Build the list of prompts for workspace init, given pre-filled flags.
///
/// Prompts whose corresponding flag is already set are omitted.
pub fn workspace_prompt_steps(
    flag_workspace: bool,
    flag_marketplace: bool,
    flag_no_starter: bool,
    flag_name: Option<&str>,
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

/// Map raw wizard answers to final `(workspace, marketplace, no_starter, marketplace_name)`.
///
/// `answers` correspond 1:1 with the steps returned by [`workspace_prompt_steps`].
pub fn resolve_workspace_answers(
    answers: &[PromptAnswer],
    flag_workspace: bool,
    flag_marketplace: bool,
    flag_no_starter: bool,
    flag_name: Option<&str>,
) -> (bool, bool, bool, String) {
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

    // Resolve marketplace name
    let marketplace_possible = flag_marketplace || needs_setup_prompt;
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
            Some(PromptAnswer::Bool(include)) => {
                if do_marketplace {
                    !include
                } else {
                    false
                }
            },
            _ => false,
        }
    } else {
        flag_no_starter
    };

    (do_workspace, do_marketplace, no_starter, marketplace_name)
}

// =============================================================================
// Non-interactive defaults
// =============================================================================

/// Apply today's defaulting logic for the non-interactive path.
///
/// If neither `--workspace` nor `--marketplace` is set, default to marketplace only.
pub fn resolve_defaults(
    workspace: bool,
    marketplace: bool,
    no_starter: bool,
    name: Option<&str>,
) -> (bool, bool, bool, String) {
    let (w, m) = if !workspace && !marketplace { (false, true) } else { (workspace, marketplace) };
    let marketplace_name =
        name.filter(|s| !s.is_empty()).unwrap_or("local-repo-plugins").to_string();
    (w, m, no_starter, marketplace_name)
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
                PromptKind::MultiSelect { options, defaults } => {
                    out.push_str("  Kind: MultiSelect\n");
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

    // =========================================================================
    // Prompt step snapshots — flag combinations
    // =========================================================================

    #[test]
    fn workspace_prompts_no_flags_snapshot() {
        let steps = workspace_prompt_steps(false, false, false, None);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_workspace_flag_snapshot() {
        let steps = workspace_prompt_steps(true, false, false, None);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_marketplace_flag_snapshot() {
        let steps = workspace_prompt_steps(false, true, false, None);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_both_flags_snapshot() {
        let steps = workspace_prompt_steps(true, true, false, None);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_no_starter_flag_snapshot() {
        let steps = workspace_prompt_steps(false, true, true, None);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_all_flags_snapshot() {
        let steps = workspace_prompt_steps(true, true, true, None);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_name_flag_omits_name_prompt() {
        let steps = workspace_prompt_steps(false, false, false, Some("custom-mkt"));
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_workspace_only_omits_name_prompt() {
        // --workspace alone means no marketplace, so no name prompt
        let steps = workspace_prompt_steps(true, false, false, None);
        insta::assert_snapshot!(format_steps(&steps));
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
        let result = resolve_workspace_answers(&answers, false, false, false, None);
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
        let result = resolve_workspace_answers(&answers, false, false, false, None);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_both_snapshot() {
        let answers = vec![
            PromptAnswer::Selected(2),
            PromptAnswer::Text(String::new()),
            PromptAnswer::Bool(true),
        ];
        let result = resolve_workspace_answers(&answers, false, false, false, None);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_decline_starter_snapshot() {
        let answers = vec![
            PromptAnswer::Selected(0),
            PromptAnswer::Text(String::new()),
            PromptAnswer::Bool(false),
        ];
        let result = resolve_workspace_answers(&answers, false, false, false, None);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_flags_bypass_snapshot() {
        // Both flags set — name + confirm prompts shown (setup skipped)
        let answers = vec![PromptAnswer::Text(String::new()), PromptAnswer::Bool(true)];
        let result = resolve_workspace_answers(&answers, true, true, false, None);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_all_flags_no_prompts_snapshot() {
        let answers: Vec<PromptAnswer> = vec![];
        let result = resolve_workspace_answers(&answers, true, true, true, Some("my-mkt"));
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_workspace_only_skips_marketplace_prompt() {
        // flag_workspace=true, flag_marketplace=false → marketplace_possible=false.
        // Covers the False branches of:
        //   - "if marketplace_possible" (name resolution skipped, uses default)
        //   - "if marketplace_possible && !flag_no_starter" (starter prompt skipped)
        let answers: Vec<PromptAnswer> = vec![];
        let result = resolve_workspace_answers(&answers, true, false, false, None);
        assert_eq!(result, (true, false, false, "local-repo-plugins".to_string()));
    }

    #[test]
    fn resolve_workspace_custom_name_snapshot() {
        let answers = vec![
            PromptAnswer::Selected(0),
            PromptAnswer::Text("my-custom-plugins".to_string()),
            PromptAnswer::Bool(true),
        ];
        let result = resolve_workspace_answers(&answers, false, false, false, None);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_empty_name_uses_default_snapshot() {
        let answers = vec![
            PromptAnswer::Selected(0),
            PromptAnswer::Text(String::new()),
            PromptAnswer::Bool(true),
        ];
        let result = resolve_workspace_answers(&answers, false, false, false, None);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_name_flag_snapshot() {
        // --name flag provided — name prompt skipped, only setup + confirm
        let answers = vec![PromptAnswer::Selected(0), PromptAnswer::Bool(true)];
        let result = resolve_workspace_answers(&answers, false, false, false, Some("preset-mkt"));
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
            resolve_defaults(false, false, false, None),
            (false, true, false, "local-repo-plugins".to_string())
        );
    }

    #[test]
    fn resolve_defaults_workspace_only() {
        assert_eq!(
            resolve_defaults(true, false, false, None),
            (true, false, false, "local-repo-plugins".to_string())
        );
    }

    #[test]
    fn resolve_defaults_both_flags() {
        assert_eq!(
            resolve_defaults(true, true, false, None),
            (true, true, false, "local-repo-plugins".to_string())
        );
    }

    #[test]
    fn resolve_defaults_no_starter() {
        assert_eq!(
            resolve_defaults(false, false, true, None),
            (false, true, true, "local-repo-plugins".to_string())
        );
    }

    #[test]
    fn resolve_defaults_with_name() {
        assert_eq!(
            resolve_defaults(false, false, false, Some("custom-mkt")),
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
            resolve_defaults(false, true, false, None),
            (false, true, false, "local-repo-plugins".to_string())
        );
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
            },
            help: None,
        }];
        let output = format_steps(&steps);
        assert!(output.contains("Kind: MultiSelect"), "expected MultiSelect kind label");
        assert!(output.contains(" *[0] Skills"), "index 0 should be pre-selected");
        assert!(output.contains("  [1] Agents"), "index 1 should not be pre-selected");
        assert!(output.contains(" *[2] MCP"), "index 2 should be pre-selected");
    }
}
