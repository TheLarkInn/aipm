//! Shared wizard types, theming, and prompt execution for interactive CLI prompts.
//!
//! Gated behind the `wizard` feature flag because it depends on `inquire`.
//! The `aipm` binary enables this feature for interactive wizard flows.

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
        /// Whether to apply name validation to this input.
        validate: bool,
    },
    /// Multi-choice list (zero or more selections).
    MultiSelect {
        /// Option labels shown to the user.
        options: Vec<&'static str>,
        /// Per-option default selection state (`true` = pre-selected).
        defaults: Vec<bool>,
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
    /// Indices of the selected options in a multi-select prompt.
    MultiSelected(Vec<usize>),
}

/// Build a styled `RenderConfig` for a modern prompt appearance.
pub fn styled_render_config() -> inquire::ui::RenderConfig<'static> {
    use inquire::ui::{Color, RenderConfig, StyleSheet, Styled};

    let mut config = RenderConfig::default_colored();
    config.prompt_prefix = Styled::new("?").with_fg(Color::LightCyan);
    config.answered_prompt_prefix = Styled::new("\u{2713}").with_fg(Color::LightGreen);
    config.placeholder = StyleSheet::new().with_fg(Color::DarkGrey);
    config
}

/// Execute a sequence of prompt steps against a real terminal via `inquire`.
///
/// Each [`PromptStep`] is dispatched to the corresponding `inquire` prompt type.
/// Text prompts with `validate: true` use
/// [`crate::manifest::validate::check_name()`] in `Interactive` mode.
/// Returns one [`PromptAnswer`] per step, in order.
pub fn execute_prompts(
    steps: &[PromptStep],
) -> Result<Vec<PromptAnswer>, Box<dyn std::error::Error>> {
    let mut answers = Vec::with_capacity(steps.len());

    for step in steps {
        let answer = match &step.kind {
            PromptKind::Select { options, default_index } => {
                let mut prompt = inquire::Select::new(step.label, options.clone())
                    .with_starting_cursor(*default_index);
                if let Some(help) = step.help {
                    prompt = prompt.with_help_message(help);
                }
                let choice = prompt.prompt()?;
                let index = options.iter().position(|o| *o == choice).ok_or_else(|| {
                    format!(
                        "internal error: selected choice `{choice}` not found in options for prompt `{}`",
                        step.label
                    )
                })?;
                PromptAnswer::Selected(index)
            },
            PromptKind::Confirm { default } => {
                let mut prompt = inquire::Confirm::new(step.label).with_default(*default);
                if let Some(help) = step.help {
                    prompt = prompt.with_help_message(help);
                }
                let result = prompt.prompt()?;
                PromptAnswer::Bool(result)
            },
            PromptKind::Text { placeholder, validate } => {
                let mut prompt = inquire::Text::new(step.label).with_placeholder(placeholder);
                if let Some(help) = step.help {
                    prompt = prompt.with_help_message(help);
                }
                if *validate {
                    prompt = prompt.with_validator(|input: &str| {
                        match crate::manifest::validate::check_name(
                            input,
                            crate::manifest::validate::ValidationMode::Interactive,
                        ) {
                            Ok(()) => Ok(inquire::validator::Validation::Valid),
                            Err(msg) => Ok(inquire::validator::Validation::Invalid(msg.into())),
                        }
                    });
                }
                let result = prompt.prompt()?;
                PromptAnswer::Text(result)
            },
            PromptKind::MultiSelect { options, defaults } => {
                let mut prompt = inquire::MultiSelect::new(step.label, options.clone());
                let default_indices: Vec<usize> = defaults
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &d)| if d { Some(i) } else { None })
                    .collect();
                prompt = prompt.with_default(&default_indices);
                if let Some(help) = step.help {
                    prompt = prompt.with_help_message(help);
                }
                let selected = prompt.prompt()?;
                let indices: Vec<usize> =
                    selected.iter().filter_map(|s| options.iter().position(|o| o == s)).collect();
                PromptAnswer::MultiSelected(indices)
            },
        };
        answers.push(answer);
    }

    Ok(answers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_step_debug_format() {
        let step = PromptStep {
            label: "test",
            kind: PromptKind::Text { placeholder: "ph".to_string(), validate: false },
            help: Some("help text"),
        };
        let debug = format!("{step:?}");
        assert!(debug.contains("test"));
    }

    #[test]
    fn prompt_kind_variants() {
        let select = PromptKind::Select { options: vec!["a"], default_index: 0 };
        let confirm = PromptKind::Confirm { default: true };
        let text = PromptKind::Text { placeholder: String::new(), validate: false };
        let multi =
            PromptKind::MultiSelect { options: vec!["x", "y"], defaults: vec![true, false] };
        assert!(format!("{select:?}").contains("Select"));
        assert!(format!("{confirm:?}").contains("Confirm"));
        assert!(format!("{text:?}").contains("Text"));
        assert!(format!("{multi:?}").contains("MultiSelect"));
    }

    #[test]
    fn prompt_answer_equality() {
        assert_eq!(PromptAnswer::Selected(0), PromptAnswer::Selected(0));
        assert_eq!(PromptAnswer::Bool(true), PromptAnswer::Bool(true));
        assert_eq!(PromptAnswer::Text("a".into()), PromptAnswer::Text("a".into()));
        assert_ne!(PromptAnswer::Selected(0), PromptAnswer::Selected(1));
        assert_eq!(
            PromptAnswer::MultiSelected(vec![0, 2]),
            PromptAnswer::MultiSelected(vec![0, 2])
        );
        assert_ne!(PromptAnswer::MultiSelected(vec![0]), PromptAnswer::MultiSelected(vec![1]));
    }

    #[test]
    fn styled_render_config_returns_config() {
        let config = styled_render_config();
        // Just verify it doesn't panic and returns a config
        let _ = format!("{:?}", config.prompt_prefix);
    }

    #[test]
    fn execute_prompts_empty_steps_returns_empty_vec() {
        let result = execute_prompts(&[]);
        assert!(result.is_ok());
        let answers = result.unwrap_or_default();
        assert!(answers.is_empty());
    }
}
