//! TTY bridge for `aipm init` wizard.
//!
//! This module contains **only** the terminal-dependent code that calls
//! `inquire::*.prompt()`. It is excluded from the coverage gate because
//! it requires a real TTY and cannot run in CI.
//!
//! All logic (prompt definitions, answer resolution, theming) lives in
//! [`super::wizard`] and is fully tested (snapshot + unit tests).

use super::wizard::{
    resolve_defaults, resolve_workspace_answers, styled_render_config, validate_marketplace_name,
    workspace_prompt_steps, PromptAnswer, PromptKind, PromptStep,
};

/// Resolved wizard output: `(workspace, marketplace, no_starter, marketplace_name)`.
type WizardResult = (bool, bool, bool, String);

/// Resolve workspace init options, launching the interactive wizard if needed.
///
/// When `interactive` is `true`, sets the global render config, prompts the
/// user for any values not provided via flags, and returns the resolved tuple.
/// When `false`, applies today's defaulting logic (marketplace only if no flags).
///
/// `flags` is `(workspace, marketplace, no_starter)` from CLI args.
pub fn resolve(
    interactive: bool,
    flags: (bool, bool, bool),
    flag_name: Option<&str>,
) -> Result<WizardResult, Box<dyn std::error::Error>> {
    let (workspace, marketplace, no_starter) = flags;
    if interactive {
        inquire::set_global_render_config(styled_render_config());
        let steps = workspace_prompt_steps(workspace, marketplace, no_starter, flag_name);
        let answers = execute_prompts(&steps)?;
        Ok(resolve_workspace_answers(&answers, workspace, marketplace, no_starter, flag_name))
    } else {
        Ok(resolve_defaults(workspace, marketplace, no_starter, flag_name))
    }
}

/// Execute prompt steps against the real terminal via `inquire`.
///
/// Returns one [`PromptAnswer`] per step, in order.
fn execute_prompts(steps: &[PromptStep]) -> Result<Vec<PromptAnswer>, Box<dyn std::error::Error>> {
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
                        match validate_marketplace_name(input) {
                            Ok(()) => Ok(inquire::validator::Validation::Valid),
                            Err(msg) => Ok(inquire::validator::Validation::Invalid(msg.into())),
                        }
                    });
                }
                let result = prompt.prompt()?;
                PromptAnswer::Text(result)
            },
        };
        answers.push(answer);
    }

    Ok(answers)
}
