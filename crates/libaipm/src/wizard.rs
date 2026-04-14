//! Shared wizard types and theming for interactive CLI prompts.
//!
//! Gated behind the `wizard` feature flag because it depends on `inquire`.
//! Both the `aipm` and `aipm-pack` binaries enable this feature and import
//! these types instead of defining their own.

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

/// Build a styled `RenderConfig` for a modern prompt appearance.
pub fn styled_render_config() -> inquire::ui::RenderConfig<'static> {
    use inquire::ui::{Color, RenderConfig, StyleSheet, Styled};

    let mut config = RenderConfig::default_colored();
    config.prompt_prefix = Styled::new("?").with_fg(Color::LightCyan);
    config.answered_prompt_prefix = Styled::new("\u{2713}").with_fg(Color::LightGreen);
    config.placeholder = StyleSheet::new().with_fg(Color::DarkGrey);
    config
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
        assert!(format!("{select:?}").contains("Select"));
        assert!(format!("{confirm:?}").contains("Confirm"));
        assert!(format!("{text:?}").contains("Text"));
    }

    #[test]
    fn prompt_answer_equality() {
        assert_eq!(PromptAnswer::Selected(0), PromptAnswer::Selected(0));
        assert_eq!(PromptAnswer::Bool(true), PromptAnswer::Bool(true));
        assert_eq!(PromptAnswer::Text("a".into()), PromptAnswer::Text("a".into()));
        assert_ne!(PromptAnswer::Selected(0), PromptAnswer::Selected(1));
    }

    #[test]
    fn styled_render_config_returns_config() {
        let config = styled_render_config();
        // Just verify it doesn't panic and returns a config
        let _ = format!("{:?}", config.prompt_prefix);
    }
}
