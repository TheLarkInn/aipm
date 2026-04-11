//! Interactive wizard for `aipm init`.
//!
//! Split into two layers for testability:
//! 1. **Prompt definitions** (pure functions) — build prompt configs, answer mapping.
//! 2. **Prompt execution** (thin bridge) — calls `inquire::*.prompt()`.

// =============================================================================
// Types
// =============================================================================

/// Describes a single prompt step in the wizard.
#[derive(Debug)]
pub struct PromptStep {
    /// Human-readable label shown to the user.
    pub label: &'static str,
    /// The kind of prompt.
    pub kind: PromptKind,
    /// Optional help message shown below the prompt.
    pub help: Option<&'static str>,
}

/// The kind of interactive prompt.
#[derive(Debug)]
pub enum PromptKind {
    /// Single-choice list.
    Select {
        /// Option labels.
        options: Vec<&'static str>,
        /// Index of the default selection.
        default_index: usize,
    },
    /// Yes/no confirmation.
    Confirm {
        /// Default value (true = yes).
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

/// Raw answer collected from a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptAnswer {
    /// Index of the selected option.
    Selected(usize),
    /// Boolean confirmation.
    Bool(bool),
    /// Text input.
    Text(String),
}

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
// Validation
// =============================================================================

/// Validate a marketplace name.
///
/// Empty string is valid (means "use default").
/// Otherwise must be lowercase alphanumeric with hyphens, optionally `@org/name`.
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
// Theming
// =============================================================================

/// Build a styled `RenderConfig` for a modern prompt appearance.
pub fn styled_render_config() -> inquire::ui::RenderConfig<'static> {
    use inquire::ui::{Color, RenderConfig, StyleSheet, Styled};

    let mut config = RenderConfig::default_colored();
    config.prompt_prefix = Styled::new("?").with_fg(Color::LightCyan);
    config.answered_prompt_prefix = Styled::new("\u{2713}").with_fg(Color::LightGreen);
    config.placeholder = StyleSheet::new().with_fg(Color::DarkGrey);
    config
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
    // validate_marketplace_name
    // =========================================================================

    #[test]
    fn validate_marketplace_name_accepts_lowercase() {
        assert!(validate_marketplace_name("my-plugins").is_ok());
    }

    #[test]
    fn validate_marketplace_name_accepts_scoped() {
        assert!(validate_marketplace_name("@org/plugins").is_ok());
    }

    #[test]
    fn validate_marketplace_name_accepts_empty_for_default() {
        assert!(validate_marketplace_name("").is_ok());
    }

    #[test]
    fn validate_marketplace_name_accepts_digits() {
        assert!(validate_marketplace_name("123abc").is_ok());
    }

    #[test]
    fn validate_marketplace_name_rejects_uppercase() {
        assert!(validate_marketplace_name("MyPlugins").is_err());
    }

    #[test]
    fn validate_marketplace_name_rejects_spaces() {
        assert!(validate_marketplace_name("my plugins").is_err());
    }

    #[test]
    fn validate_marketplace_name_rejects_underscores() {
        assert!(validate_marketplace_name("my_plugins").is_err());
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
}
