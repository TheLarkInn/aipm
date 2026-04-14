//! TTY bridge for `aipm` wizard flows (init, migrate, make plugin).
//!
//! This module wires up interactive wizard flows by calling
//! [`libaipm::wizard::execute_prompts()`] for TTY prompt execution.
//! It is excluded from the coverage gate because it requires a real TTY.
//!
//! All logic (prompt definitions, answer resolution, theming) lives in
//! [`super::wizard`] and is fully tested (snapshot + unit tests).

use super::wizard;
use super::wizard::{
    migrate_cleanup_prompt_steps, resolve_defaults, resolve_migrate_cleanup_answer,
    resolve_workspace_answers, styled_render_config, workspace_prompt_steps, PromptAnswer,
    PromptKind, PromptStep,
};

/// Resolved wizard output: `(workspace, marketplace, no_starter, marketplace_name)`.
type WizardResult = (bool, bool, bool, String);

/// Resolved make-plugin output: `(name, engine, features)`.
type MakePluginResult = (String, String, Vec<String>);

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
        let answers = libaipm::wizard::execute_prompts(&steps)?;
        Ok(resolve_workspace_answers(&answers, workspace, marketplace, no_starter, flag_name))
    } else {
        Ok(resolve_defaults(workspace, marketplace, no_starter, flag_name))
    }
}

/// Prompt the user about removing migrated source files.
///
/// Returns `true` if the user confirmed cleanup, `false` otherwise.
/// When `interactive` is `false`, returns `Ok(false)` immediately.
pub fn resolve_migrate_cleanup(
    interactive: bool,
    outcome: &libaipm::migrate::Outcome,
) -> Result<bool, Box<dyn std::error::Error>> {
    if !interactive {
        return Ok(false);
    }

    let migrated_count = outcome.migrated_sources().len();
    let steps = migrate_cleanup_prompt_steps(migrated_count);

    if steps.is_empty() {
        return Ok(false);
    }

    inquire::set_global_render_config(styled_render_config());
    let answers = libaipm::wizard::execute_prompts(&steps)?;
    Ok(resolve_migrate_cleanup_answer(&answers))
}

/// Resolve `aipm make plugin` wizard values.
///
/// In interactive mode, runs a two-phase wizard:
/// 1. Prompt for name and engine (if not set via flags)
/// 2. Resolve engine, compute engine-filtered feature options, prompt for features
///
/// In non-interactive mode, validates that required flags are present
/// (`--name` and `--feature`) and defaults engine to `"claude"`.
pub fn resolve_make_plugin(
    interactive: bool,
    flag_name: Option<&str>,
    flag_engine: Option<&str>,
    flag_features: &[String],
) -> Result<MakePluginResult, Box<dyn std::error::Error>> {
    if !interactive {
        let name = flag_name
            .map(str::to_string)
            .ok_or_else(|| Box::new(libaipm::make::Error::MissingFlag("name".to_string())))?;
        if flag_features.is_empty() {
            return Err(Box::new(libaipm::make::Error::MissingFlag("feature".to_string())));
        }
        let engine = flag_engine.map_or_else(|| "claude".to_string(), str::to_string);
        return Ok((name, engine, flag_features.to_vec()));
    }

    inquire::set_global_render_config(styled_render_config());

    // Phase 1: Name + Engine
    let mut phase1_steps: Vec<PromptStep> = Vec::new();
    if flag_name.is_none() {
        phase1_steps.push(PromptStep {
            label: "Plugin name",
            kind: PromptKind::Text { placeholder: "my-plugin".to_string(), validate: true },
            help: Some("Lowercase, hyphens allowed"),
        });
    }
    if flag_engine.is_none() {
        phase1_steps.push(PromptStep {
            label: "Target engine",
            kind: PromptKind::Select { options: wizard::ENGINE_OPTIONS.to_vec(), default_index: 0 },
            help: Some("Which AI coding tool will this plugin target?"),
        });
    }
    let phase1_answers = libaipm::wizard::execute_prompts(&phase1_steps)?;

    // Resolve name + engine from phase 1 answers
    let mut idx = 0;
    let name = flag_name.map_or_else(
        || {
            let r = match phase1_answers.get(idx) {
                Some(PromptAnswer::Text(t)) => t.clone(),
                _ => String::new(),
            };
            idx += 1;
            r
        },
        str::to_string,
    );

    let engine = flag_engine.map_or_else(
        || match phase1_answers.get(idx) {
            Some(PromptAnswer::Selected(i)) => wizard::engine_from_index(*i).to_string(),
            _ => "claude".to_string(),
        },
        str::to_string,
    );

    // Validate the resolved engine before using it to filter features
    match engine.as_str() {
        "claude" | "copilot" | "both" => {},
        _ => return Err(Box::new(libaipm::make::Error::InvalidEngine(engine.clone()))),
    }

    // Phase 2: Features (filtered by resolved engine)
    if flag_features.is_empty() {
        let available = libaipm::make::engine_features::features_for_engine(&engine);
        let labels: Vec<&'static str> =
            available.iter().map(libaipm::make::Feature::label).collect();
        let defaults: Vec<bool> = labels.iter().map(|_| false).collect();
        let cli_names: Vec<&str> = available.iter().map(libaipm::make::Feature::cli_name).collect();

        let feature_step = PromptStep {
            label: "AI features to include",
            kind: PromptKind::MultiSelect { options: labels, defaults },
            help: Some("Select the features for your plugin"),
        };
        let feature_answers = libaipm::wizard::execute_prompts(&[feature_step])?;

        let features: Vec<String> = match feature_answers.first() {
            Some(PromptAnswer::MultiSelected(indices)) => {
                indices.iter().filter_map(|&i| cli_names.get(i).map(|s| (*s).to_string())).collect()
            },
            _ => Vec::new(),
        };
        if features.is_empty() {
            return Err(Box::new(libaipm::make::Error::MissingFlag("feature".to_string())));
        }
        Ok((name, engine, features))
    } else {
        Ok((name, engine, flag_features.to_vec()))
    }
}
