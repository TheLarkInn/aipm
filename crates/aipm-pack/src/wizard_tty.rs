//! TTY bridge for `aipm-pack init` wizard.
//!
//! This module contains **only** the terminal-dependent code that calls
//! `inquire::*.prompt()`. It is excluded from the coverage gate because
//! it requires a real TTY and cannot run in CI.
//!
//! All logic (prompt definitions, answer resolution, validation, theming)
//! lives in [`super::wizard`] and is fully tested (snapshot + unit tests).

use std::path::Path;

use libaipm::manifest::types::PluginType;

use super::wizard::{
    package_prompt_steps, resolve_package_answers, styled_render_config, PromptAnswer, PromptKind,
    PromptStep,
};

/// Resolved wizard output: `(name, plugin_type)`.
type WizardResult = (Option<String>, Option<PluginType>);

/// Resolve package init options, launching the interactive wizard if needed.
///
/// When `interactive` is `true`, sets the global render config, prompts the
/// user for any values not provided via flags, and returns the resolved tuple.
/// When `false`, returns the flag values as-is (today's behavior).
pub fn resolve(
    interactive: bool,
    dir: &Path,
    flag_name: Option<String>,
    flag_type: Option<PluginType>,
) -> Result<WizardResult, Box<dyn std::error::Error>> {
    if interactive {
        inquire::set_global_render_config(styled_render_config());
        let steps = package_prompt_steps(dir, flag_name.as_deref(), flag_type);
        let answers = execute_prompts(&steps)?;
        Ok(resolve_package_answers(&answers, flag_name.as_deref(), flag_type))
    } else {
        Ok((flag_name, flag_type))
    }
}

/// Execute prompt steps against the real terminal via `inquire`.
///
/// Returns one [`PromptAnswer`] per step, in order.
fn execute_prompts(steps: &[PromptStep]) -> Result<Vec<PromptAnswer>, Box<dyn std::error::Error>> {
    let mut answers = Vec::with_capacity(steps.len());

    for step in steps {
        let answer = match &step.kind {
            PromptKind::Text { placeholder, validate } => {
                let mut prompt = inquire::Text::new(step.label).with_placeholder(placeholder);
                if let Some(help) = step.help {
                    prompt = prompt.with_help_message(help);
                }
                if *validate {
                    prompt = prompt.with_validator(|input: &str| {
                        match libaipm::manifest::validate::check_name(
                            input,
                            libaipm::manifest::validate::ValidationMode::Interactive,
                        ) {
                            Ok(()) => Ok(inquire::validator::Validation::Valid),
                            Err(msg) => Ok(inquire::validator::Validation::Invalid(msg.into())),
                        }
                    });
                }
                let result = prompt.prompt()?;
                PromptAnswer::Text(result)
            },
            PromptKind::Confirm { default } => {
                let mut prompt = inquire::Confirm::new(step.label).with_default(*default);
                if let Some(help) = step.help {
                    prompt = prompt.with_help_message(help);
                }
                let result = prompt.prompt()?;
                PromptAnswer::Bool(result)
            },
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
        };
        answers.push(answer);
    }

    Ok(answers)
}
