//! TTY bridge for `aipm init` wizard.
//!
//! This module contains **only** the terminal-dependent code that calls
//! `inquire::*.prompt()`. It is excluded from the coverage gate because
//! it requires a real TTY and cannot run in CI.
//!
//! All logic (prompt definitions, answer resolution, theming) lives in
//! [`super::wizard`] and is fully snapshot-tested.

use super::wizard::{
    resolve_workspace_answers, styled_render_config, workspace_prompt_steps, PromptAnswer,
    PromptKind, PromptStep,
};

/// Run the interactive workspace init wizard against a real terminal.
///
/// Sets the global render config, collects user input via `inquire` prompts,
/// and returns the resolved `(workspace, marketplace, no_starter)` tuple.
pub fn run(
    workspace: bool,
    marketplace: bool,
    no_starter: bool,
) -> Result<(bool, bool, bool), Box<dyn std::error::Error>> {
    inquire::set_global_render_config(styled_render_config());
    let steps = workspace_prompt_steps(workspace, marketplace, no_starter);
    let answers = execute_prompts(&steps)?;
    Ok(resolve_workspace_answers(&answers, workspace, marketplace, no_starter))
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
                let index = options.iter().position(|o| *o == choice).unwrap_or(0);
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
        };
        answers.push(answer);
    }

    Ok(answers)
}
