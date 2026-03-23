//! TTY bridge for `aipm-pack init` wizard.
//!
//! This module contains **only** the terminal-dependent code that calls
//! `inquire::*.prompt()`. It is excluded from the coverage gate because
//! it requires a real TTY and cannot run in CI.
//!
//! All logic (prompt definitions, answer resolution, validation, theming)
//! lives in [`super::wizard`] and is fully snapshot-tested.

use std::path::Path;

use libaipm::manifest::types::PluginType;

use super::wizard::{
    package_prompt_steps, resolve_package_answers, styled_render_config, validate_package_name,
    PromptAnswer, PromptKind, PromptStep,
};

/// Resolved wizard output: `(name, plugin_type)`.
type WizardResult = (Option<String>, Option<PluginType>);

/// Run the interactive package init wizard against a real terminal.
///
/// Sets the global render config, collects user input via `inquire` prompts,
/// and returns the resolved `(name, plugin_type)` tuple.
pub fn run(
    dir: &Path,
    flag_name: Option<&str>,
    flag_type: Option<PluginType>,
) -> Result<WizardResult, Box<dyn std::error::Error>> {
    inquire::set_global_render_config(styled_render_config());
    let steps = package_prompt_steps(dir, flag_name, flag_type);
    let answers = execute_prompts(&steps)?;
    Ok(resolve_package_answers(&answers, flag_name, flag_type))
}

/// Execute prompt steps against the real terminal via `inquire`.
///
/// Returns one [`PromptAnswer`] per step, in order.
fn execute_prompts(steps: &[PromptStep]) -> Result<Vec<PromptAnswer>, Box<dyn std::error::Error>> {
    let mut answers = Vec::with_capacity(steps.len());

    for step in steps {
        let answer = match &step.kind {
            PromptKind::Text { placeholder } => {
                let mut prompt = inquire::Text::new(step.label).with_placeholder(placeholder);
                if let Some(help) = step.help {
                    prompt = prompt.with_help_message(help);
                }
                // Apply validator only for the package name prompt
                if step.label == "Package name:" {
                    prompt =
                        prompt.with_validator(|input: &str| match validate_package_name(input) {
                            Ok(()) => Ok(inquire::validator::Validation::Valid),
                            Err(msg) => Ok(inquire::validator::Validation::Invalid(msg.into())),
                        });
                }
                let result = prompt.prompt()?;
                PromptAnswer::Text(result)
            },
            PromptKind::Select { options, default_index } => {
                let mut prompt = inquire::Select::new(step.label, options.clone())
                    .with_starting_cursor(*default_index);
                if let Some(help) = step.help {
                    prompt = prompt.with_help_message(help);
                }
                let choice = prompt.prompt()?;
                let index = options.iter().position(|o| *o == choice).unwrap_or(0);
                PromptAnswer::Selected(index)
            },
        };
        answers.push(answer);
    }

    Ok(answers)
}
