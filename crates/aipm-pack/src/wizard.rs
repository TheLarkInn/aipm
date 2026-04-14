//! Interactive wizard for `aipm-pack init`.
//!
//! Split into two layers for testability:
//! 1. **Prompt definitions** (pure functions) — build prompt configs, validators, answer mapping.
//! 2. **Prompt execution** (thin bridge) — calls `inquire::*.prompt()`.

use std::path::Path;

use libaipm::manifest::types::PluginType;

pub use libaipm::wizard::{styled_render_config, PromptAnswer, PromptKind, PromptStep};

// =============================================================================
// Prompt definitions — fully testable, no terminal dependency
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
                        out.push_str("  Validate: package-name\n");
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
    // Prompt step snapshots — all flag combinations
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
            _ => {
                // wrong prompt kind — fail the test
                assert!(false, "expected Text prompt for package name");
            },
        }
    }

    // =========================================================================
    // Answer resolution snapshots
    // =========================================================================

    #[test]
    fn resolve_package_defaults_snapshot() {
        let answers = vec![
            PromptAnswer::Text(String::new()), // empty = use placeholder
            PromptAnswer::Text(String::new()), // empty description
            PromptAnswer::Selected(0),         // composite
        ];
        let result = resolve_package_answers(&answers, None, None);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_package_custom_name_snapshot() {
        let answers = vec![
            PromptAnswer::Text("my-plugin".to_string()),
            PromptAnswer::Text("A cool plugin".to_string()),
            PromptAnswer::Selected(1), // skill
        ];
        let result = resolve_package_answers(&answers, None, None);
        insta::assert_snapshot!(format!("{:?}", result));
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
            out.push_str(&format!("index {} -> {:?} (expected {})\n", i, pt, label));
        }
        insta::assert_snapshot!(out);
    }

    #[test]
    fn resolve_package_with_name_flag_snapshot() {
        // Only description + type prompts shown (name skipped)
        let answers = vec![
            PromptAnswer::Text(String::new()), // description
            PromptAnswer::Selected(2),         // agent
        ];
        let result = resolve_package_answers(&answers, Some("preset-name"), None);
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_package_with_type_flag_snapshot() {
        // Name + description prompts shown (type skipped)
        let answers =
            vec![PromptAnswer::Text("custom".to_string()), PromptAnswer::Text(String::new())];
        let result = resolve_package_answers(&answers, None, Some(PluginType::Agent));
        insta::assert_snapshot!(format!("{:?}", result));
    }

    #[test]
    fn resolve_package_with_both_flags_snapshot() {
        // Only description prompt shown
        let answers = vec![PromptAnswer::Text("desc".to_string())];
        let result = resolve_package_answers(&answers, Some("preset"), Some(PluginType::Hook));
        insta::assert_snapshot!(format!("{:?}", result));
    }

    // =========================================================================
    // Validator unit tests (now delegates to shared validator)
    // =========================================================================

    fn validate_name_interactive(input: &str) -> Result<(), String> {
        libaipm::manifest::validate::check_name(
            input,
            libaipm::manifest::validate::ValidationMode::Interactive,
        )
    }

    #[test]
    fn validate_package_name_accepts_lowercase() {
        assert!(validate_name_interactive("my-plugin").is_ok());
    }

    #[test]
    fn validate_package_name_accepts_scoped() {
        assert!(validate_name_interactive("@org/my-plugin").is_ok());
    }

    #[test]
    fn validate_package_name_accepts_empty_for_default() {
        assert!(validate_name_interactive("").is_ok());
    }

    #[test]
    fn validate_package_name_accepts_digits() {
        assert!(validate_name_interactive("123abc").is_ok());
    }

    #[test]
    fn validate_package_name_rejects_uppercase() {
        assert!(validate_name_interactive("MyPlugin").is_err());
    }

    #[test]
    fn validate_package_name_rejects_spaces() {
        assert!(validate_name_interactive("my plugin").is_err());
    }

    #[test]
    fn validate_package_name_rejects_special_chars() {
        assert!(validate_name_interactive("my_plugin!").is_err());
    }

    #[test]
    fn validate_package_name_rejects_underscores() {
        assert!(validate_name_interactive("my_plugin").is_err());
    }

    // =========================================================================
    // Theming snapshot
    // =========================================================================

    #[test]
    fn styled_render_config_snapshot() {
        let config = styled_render_config();
        let summary = format!(
            "prompt_prefix: {:?}\nanswered_prefix: {:?}\nplaceholder: {:?}",
            config.prompt_prefix, config.answered_prompt_prefix, config.placeholder,
        );
        insta::assert_snapshot!(summary);
    }

    // =========================================================================
    // plugin_type_from_index coverage
    // =========================================================================

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
    // format_steps edge-case: empty step list
    // =========================================================================

    #[test]
    fn format_steps_empty_input_returns_no_prompts_label() {
        // Covers the `if steps.is_empty()` True branch in the format_steps helper.
        let result = format_steps(&[]);
        assert_eq!(result, "(no prompts)\n");
    }
}
