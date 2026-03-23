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
}

/// Raw answer collected from a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptAnswer {
    /// Index of the selected option.
    Selected(usize),
    /// Boolean confirmation.
    Bool(bool),
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

/// Map raw wizard answers to final `(workspace, marketplace, no_starter)` values.
///
/// `answers` correspond 1:1 with the steps returned by [`workspace_prompt_steps`].
pub fn resolve_workspace_answers(
    answers: &[PromptAnswer],
    flag_workspace: bool,
    flag_marketplace: bool,
    flag_no_starter: bool,
) -> (bool, bool, bool) {
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

    // Resolve no_starter
    let marketplace_possible = flag_marketplace || needs_setup_prompt;
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

    (do_workspace, do_marketplace, no_starter)
}

// =============================================================================
// Non-interactive defaults
// =============================================================================

/// Apply today's defaulting logic for the non-interactive path.
///
/// If neither `--workspace` nor `--marketplace` is set, default to marketplace only.
pub const fn resolve_defaults(
    workspace: bool,
    marketplace: bool,
    no_starter: bool,
) -> (bool, bool, bool) {
    let (w, m) = if !workspace && !marketplace { (false, true) } else { (workspace, marketplace) };
    (w, m, no_starter)
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
            }
            if let Some(help) = step.help {
                out.push_str(&format!("  Help: {}\n", help));
            }
            out.push('\n');
        }
        out
    }

    // =========================================================================
    // Prompt step snapshots — all 6 flag combinations
    // =========================================================================

    #[test]
    fn workspace_prompts_no_flags_snapshot() {
        let steps = workspace_prompt_steps(false, false, false);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_workspace_flag_snapshot() {
        let steps = workspace_prompt_steps(true, false, false);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_marketplace_flag_snapshot() {
        let steps = workspace_prompt_steps(false, true, false);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_both_flags_snapshot() {
        let steps = workspace_prompt_steps(true, true, false);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_no_starter_flag_snapshot() {
        let steps = workspace_prompt_steps(false, true, true);
        insta::assert_snapshot!(format_steps(&steps));
    }

    #[test]
    fn workspace_prompts_all_flags_snapshot() {
        let steps = workspace_prompt_steps(true, true, true);
        insta::assert_snapshot!(format_steps(&steps));
    }

    // =========================================================================
    // Answer resolution snapshots
    // =========================================================================

    #[test]
    fn resolve_workspace_marketplace_only_snapshot() {
        let answers = vec![PromptAnswer::Selected(0), PromptAnswer::Bool(true)];
        let result = resolve_workspace_answers(&answers, false, false, false);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_manifest_only_snapshot() {
        // Selecting "Workspace only" — no confirm prompt follows because marketplace=false
        let answers = vec![PromptAnswer::Selected(1), PromptAnswer::Bool(true)];
        let result = resolve_workspace_answers(&answers, false, false, false);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_both_snapshot() {
        let answers = vec![PromptAnswer::Selected(2), PromptAnswer::Bool(true)];
        let result = resolve_workspace_answers(&answers, false, false, false);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_decline_starter_snapshot() {
        let answers = vec![PromptAnswer::Selected(0), PromptAnswer::Bool(false)];
        let result = resolve_workspace_answers(&answers, false, false, false);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_flags_bypass_snapshot() {
        // Both flags set — confirm prompt is the only one, and it's for starter
        let answers = vec![PromptAnswer::Bool(true)];
        let result = resolve_workspace_answers(&answers, true, true, false);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_workspace_all_flags_no_prompts_snapshot() {
        let answers: Vec<PromptAnswer> = vec![];
        let result = resolve_workspace_answers(&answers, true, true, true);
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
        // Neither flag → marketplace only
        assert_eq!(resolve_defaults(false, false, false), (false, true, false));
    }

    #[test]
    fn resolve_defaults_workspace_only() {
        assert_eq!(resolve_defaults(true, false, false), (true, false, false));
    }

    #[test]
    fn resolve_defaults_both_flags() {
        assert_eq!(resolve_defaults(true, true, false), (true, true, false));
    }

    #[test]
    fn resolve_defaults_no_starter() {
        assert_eq!(resolve_defaults(false, false, true), (false, true, true));
    }
}
