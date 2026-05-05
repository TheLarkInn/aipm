//! Runtime helpers over the generated tables.
//!
//! These functions wrap the const tables emitted by `build.rs` (the
//! `Engine` enum, `EngineSet`, `HOOK_EVENTS_BY_ENGINE`,
//! `TOOL_COMPATIBILITY`, and `paths::*`) with idiomatic lookup APIs that
//! the rest of the workspace consumes.

use crate::generated::{paths, Engine, EngineSet, HOOK_EVENTS_BY_ENGINE, TOOL_COMPATIBILITY};

/// A non-engine "marketplace host" directory.
///
/// `.ai/` is not an engine itself but a convention for multi-engine
/// marketplace plugins. It needs to be discoverable in the same code
/// paths that resolve `.claude/` / `.github/` to engines, hence a
/// dedicated enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarketplaceHost {
    /// The `.ai/` directory.
    Ai,
}

/// Diagnostic detail attached to a `valid_tool_name` failure.
///
/// `supported_by` is the [`EngineSet`] of engines that *do* implement
/// the tool; `declared` is the plugin's `[engines]` set as supplied by
/// the caller. Their disjoint relationship is the reason the check
/// failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolNameViolation {
    pub supported_by: EngineSet,
    pub declared: EngineSet,
}

/// Map a plugin-root directory component (e.g. `.claude`, `.github`) to
/// its [`Engine`].
///
/// `.ai` is intentionally not handled here — pair this with
/// [`marketplace_host_for_root_dir`] when the caller also needs to
/// recognise marketplace-host directories.
#[must_use]
pub fn engine_for_root_dir(name: &str) -> Option<Engine> {
    if name == paths::CLAUDE_DOT {
        Some(Engine::Claude)
    } else if name == paths::GITHUB_DOT {
        Some(Engine::Copilot)
    } else {
        None
    }
}

/// Map a plugin-root directory component to its [`MarketplaceHost`].
///
/// Today only `.ai` is recognised.
#[must_use]
pub fn marketplace_host_for_root_dir(name: &str) -> Option<MarketplaceHost> {
    if name == paths::AI_DOT {
        Some(MarketplaceHost::Ai)
    } else {
        None
    }
}

/// Returns true iff `event` is a recognised hook event name (canonical
/// or alias) for the given `engine`.
#[must_use]
pub fn is_valid_event(event: &str, engine: Engine) -> bool {
    let Some((_, events)) = HOOK_EVENTS_BY_ENGINE.iter().find(|(e, _)| *e == engine) else {
        return false;
    };
    events.iter().any(|he| he.name == event || he.aliases.contains(&event))
}

/// If `event` is a deprecated/legacy alias of an `engine`'s canonical
/// hook event name, return the canonical name; otherwise `None`.
///
/// Returns `None` when `event` is already canonical or unknown.
#[must_use]
pub fn suggest_canonical(event: &str, engine: Engine) -> Option<&'static str> {
    let (_, events) = HOOK_EVENTS_BY_ENGINE.iter().find(|(e, _)| *e == engine)?;
    events.iter().find(|he| he.aliases.contains(&event)).map(|he| he.name)
}

/// Validate that a tool name is compatible with the plugin's declared
/// engine set.
///
/// Semantics:
///   * Unknown tools return `Ok(())` — out of scope for this check.
///   * Shared tools (`EngineSet::ALL`) return `Ok(())`.
///   * If `declared` is empty, an engine-exclusive tool returns `Err`.
///   * If `declared` intersects the tool's `supported_by`, `Ok(())`.
///   * Otherwise `Err` with the supported / declared sets attached.
pub fn valid_tool_name_check(tool: &str, declared: EngineSet) -> Result<(), ToolNameViolation> {
    let Some((_, supported)) = TOOL_COMPATIBILITY.iter().find(|(n, _)| *n == tool) else {
        return Ok(());
    };
    let supported = *supported;
    if supported == EngineSet::ALL {
        return Ok(());
    }
    if declared.is_empty() || !declared.intersects(supported) {
        return Err(ToolNameViolation { supported_by: supported, declared });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_for_root_dir_known_engines() {
        assert_eq!(engine_for_root_dir(".claude"), Some(Engine::Claude));
        assert_eq!(engine_for_root_dir(".github"), Some(Engine::Copilot));
    }

    #[test]
    fn engine_for_root_dir_excludes_marketplace_host() {
        assert_eq!(engine_for_root_dir(".ai"), None);
    }

    #[test]
    fn engine_for_root_dir_unknown_returns_none() {
        assert_eq!(engine_for_root_dir("unknown"), None);
        assert_eq!(engine_for_root_dir(""), None);
    }

    #[test]
    fn marketplace_host_for_root_dir_only_ai() {
        assert_eq!(marketplace_host_for_root_dir(".ai"), Some(MarketplaceHost::Ai));
        assert_eq!(marketplace_host_for_root_dir(".claude"), None);
        assert_eq!(marketplace_host_for_root_dir(".github"), None);
        assert_eq!(marketplace_host_for_root_dir(""), None);
    }

    #[test]
    fn is_valid_event_claude_canonical_pascal_case() {
        assert!(is_valid_event("PreToolUse", Engine::Claude));
        assert!(is_valid_event("PostToolUse", Engine::Claude));
        assert!(is_valid_event("FileChanged", Engine::Claude));
    }

    #[test]
    fn is_valid_event_claude_rejects_camel_case() {
        assert!(!is_valid_event("preToolUse", Engine::Claude));
        assert!(!is_valid_event("postToolUse", Engine::Claude));
    }

    #[test]
    fn is_valid_event_copilot_canonical_camel_case() {
        assert!(is_valid_event("preToolUse", Engine::Copilot));
        assert!(is_valid_event("agentStop", Engine::Copilot));
    }

    #[test]
    fn is_valid_event_copilot_accepts_legacy_pascal_case_aliases() {
        // PreToolUse is an alias for preToolUse on copilot
        assert!(is_valid_event("PreToolUse", Engine::Copilot));
        // Stop is an alias for agentStop on copilot
        assert!(is_valid_event("Stop", Engine::Copilot));
    }

    #[test]
    fn is_valid_event_unknown_event_rejected() {
        assert!(!is_valid_event("NotAnEvent", Engine::Claude));
        assert!(!is_valid_event("NotAnEvent", Engine::Copilot));
    }

    #[test]
    fn suggest_canonical_pascal_case_to_camel_case_for_copilot() {
        assert_eq!(suggest_canonical("PreToolUse", Engine::Copilot), Some("preToolUse"));
        assert_eq!(suggest_canonical("PostToolUse", Engine::Copilot), Some("postToolUse"));
        assert_eq!(suggest_canonical("Stop", Engine::Copilot), Some("agentStop"));
        assert_eq!(
            suggest_canonical("UserPromptSubmit", Engine::Copilot),
            Some("userPromptSubmitted")
        );
        // PostToolUseFailure is mapped to errorOccurred — preserves the legacy
        // known_events::COPILOT_LEGACY_MAP behaviour after that module's
        // deletion in the engine-api-schema source-of-truth refactor.
        assert_eq!(suggest_canonical("PostToolUseFailure", Engine::Copilot), Some("errorOccurred"));
    }

    #[test]
    fn suggest_canonical_for_canonical_input_returns_none() {
        // Canonical names aren't aliases of themselves.
        assert_eq!(suggest_canonical("preToolUse", Engine::Copilot), None);
        assert_eq!(suggest_canonical("PreToolUse", Engine::Claude), None);
    }

    #[test]
    fn suggest_canonical_unknown_event_returns_none() {
        assert_eq!(suggest_canonical("Unknown", Engine::Claude), None);
        assert_eq!(suggest_canonical("Unknown", Engine::Copilot), None);
    }

    #[test]
    fn valid_tool_name_check_unknown_tool_passes() {
        // Out of scope for this lint — the unknown_tool_name lint catches typos.
        assert!(valid_tool_name_check("totally_made_up_tool_name", EngineSet::empty()).is_ok());
        assert!(valid_tool_name_check("totally_made_up_tool_name", EngineSet::CLAUDE).is_ok());
    }

    #[test]
    fn valid_tool_name_check_shared_tool_always_ok() {
        for declared in [EngineSet::empty(), EngineSet::CLAUDE, EngineSet::COPILOT, EngineSet::ALL]
        {
            assert!(
                valid_tool_name_check("bash", declared).is_ok(),
                "bash failed for {declared:?}"
            );
        }
    }

    #[test]
    fn valid_tool_name_check_undeclared_engine_exclusive_tool_errors() {
        let result = valid_tool_name_check("Task", EngineSet::empty());
        let v = result.expect_err("expected ToolNameViolation");
        assert_eq!(v.supported_by, EngineSet::CLAUDE);
        assert_eq!(v.declared, EngineSet::empty());
    }

    #[test]
    fn valid_tool_name_check_declared_engine_supports_tool_passes() {
        assert!(valid_tool_name_check("Task", EngineSet::CLAUDE).is_ok());
        assert!(valid_tool_name_check("Task", EngineSet::ALL).is_ok());
        assert!(valid_tool_name_check("browser_navigate", EngineSet::COPILOT).is_ok());
        assert!(valid_tool_name_check("browser_navigate", EngineSet::ALL).is_ok());
    }

    #[test]
    fn valid_tool_name_check_declared_engine_does_not_support_tool_errors() {
        let v = valid_tool_name_check("Task", EngineSet::COPILOT)
            .expect_err("Task on copilot-only plugin should error");
        assert_eq!(v.supported_by, EngineSet::CLAUDE);
        assert_eq!(v.declared, EngineSet::COPILOT);

        let v2 = valid_tool_name_check("browser_navigate", EngineSet::CLAUDE)
            .expect_err("browser_navigate on claude-only plugin should error");
        assert_eq!(v2.supported_by, EngineSet::COPILOT);
    }
}
