//! Hard-coded hook event lists per tool, derived from binary analysis.
//!
//! **Claude Code v2.1.87**: 27 events (`PascalCase`)
//! **Copilot CLI v1.0.12**: 10 events (`camelCase`) + legacy `PascalCase` mapping
//!
//! Updated per aipm release by re-running binary analysis.
//! Source: [research/docs/2026-03-31-cli-binary-frontmatter-hook-analysis.md]

/// Valid Claude Code hook events (27, `PascalCase`).
pub const CLAUDE_EVENTS: &[&str] = &[
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "Notification",
    "SessionStart",
    "Stop",
    "StopFailure",
    "SubagentStart",
    "SubagentStop",
    "PreCompact",
    "PostCompact",
    "SessionEnd",
    "PermissionRequest",
    "Setup",
    "TeammateIdle",
    "TaskCreated",
    "TaskCompleted",
    "UserPromptSubmit",
    "ToolError",
    "Elicitation",
    "ElicitationResult",
    "ConfigChange",
    "InstructionsLoaded",
    "WorktreeCreate",
    "WorktreeRemove",
    "CwdChanged",
    "FileChanged",
];

/// Valid Copilot CLI hook events (10, `camelCase`).
pub const COPILOT_EVENTS: &[&str] = &[
    "sessionStart",
    "sessionEnd",
    "userPromptSubmitted",
    "preToolUse",
    "postToolUse",
    "errorOccurred",
    "agentStop",
    "subagentStop",
    "subagentStart",
    "preCompact",
];

/// Legacy `PascalCase` -> canonical `camelCase` mapping for Copilot CLI.
///
/// Copilot normalizes these legacy names internally. The `hook/legacy-event-name`
/// rule uses this to suggest canonical names.
pub const COPILOT_LEGACY_MAP: &[(&str, &str)] = &[
    ("SessionStart", "sessionStart"),
    ("SessionEnd", "sessionEnd"),
    ("UserPromptSubmit", "userPromptSubmitted"),
    ("PreToolUse", "preToolUse"),
    ("PostToolUse", "postToolUse"),
    ("PostToolUseFailure", "errorOccurred"),
    ("ErrorOccurred", "errorOccurred"),
    ("Stop", "agentStop"),
    ("SubagentStop", "subagentStop"),
    ("PreCompact", "preCompact"),
];

/// Check if an event name is valid for the given tool.
///
/// `tool` should be `".claude"` or `".github"`.
/// For `.ai/` marketplace plugins, use [`is_valid_for_any_tool`].
pub fn is_valid_event(event: &str, tool: &str) -> bool {
    match tool {
        ".claude" => CLAUDE_EVENTS.contains(&event),
        ".github" => {
            COPILOT_EVENTS.contains(&event)
                || COPILOT_LEGACY_MAP.iter().any(|(legacy, _)| *legacy == event)
        },
        _ => is_valid_for_any_tool(event),
    }
}

/// Check if an event name is valid for any supported tool (union of all sets).
pub fn is_valid_for_any_tool(event: &str) -> bool {
    CLAUDE_EVENTS.contains(&event)
        || COPILOT_EVENTS.contains(&event)
        || COPILOT_LEGACY_MAP.iter().any(|(legacy, _)| *legacy == event)
}

/// If the event is a legacy Copilot name, return the canonical camelCase name.
pub fn suggest_canonical(event: &str) -> Option<&'static str> {
    COPILOT_LEGACY_MAP.iter().find(|(legacy, _)| *legacy == event).map(|(_, canonical)| *canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_events_count() {
        assert_eq!(CLAUDE_EVENTS.len(), 27);
    }

    #[test]
    fn copilot_events_count() {
        assert_eq!(COPILOT_EVENTS.len(), 10);
    }

    #[test]
    fn is_valid_event_claude() {
        assert!(is_valid_event("PreToolUse", ".claude"));
        assert!(is_valid_event("FileChanged", ".claude"));
        assert!(!is_valid_event("sessionStart", ".claude"));
        assert!(!is_valid_event("InvalidEvent", ".claude"));
    }

    #[test]
    fn is_valid_event_copilot() {
        assert!(is_valid_event("sessionStart", ".github"));
        assert!(is_valid_event("preCompact", ".github"));
        // Legacy names are also accepted for copilot
        assert!(is_valid_event("Stop", ".github"));
        assert!(is_valid_event("SessionStart", ".github"));
        assert!(!is_valid_event("InvalidEvent", ".github"));
    }

    #[test]
    fn is_valid_for_any_tool_union() {
        // Claude-only event
        assert!(is_valid_for_any_tool("FileChanged"));
        // Copilot-only event
        assert!(is_valid_for_any_tool("errorOccurred"));
        // Shared event (different case)
        assert!(is_valid_for_any_tool("PreToolUse"));
        assert!(is_valid_for_any_tool("preToolUse"));
        // Invalid
        assert!(!is_valid_for_any_tool("TotallyInvalid"));
    }

    #[test]
    fn suggest_canonical_finds_mapping() {
        assert_eq!(suggest_canonical("Stop"), Some("agentStop"));
        assert_eq!(suggest_canonical("UserPromptSubmit"), Some("userPromptSubmitted"));
        assert_eq!(suggest_canonical("PostToolUseFailure"), Some("errorOccurred"));
    }

    #[test]
    fn suggest_canonical_returns_none_for_non_legacy() {
        assert_eq!(suggest_canonical("preToolUse"), None);
        assert_eq!(suggest_canonical("InvalidEvent"), None);
    }

    #[test]
    fn is_valid_event_unknown_tool_falls_back_to_any() {
        // The `_ =>` arm delegates to `is_valid_for_any_tool`.
        assert!(is_valid_event("PreToolUse", ".ai"));
        assert!(is_valid_event("preToolUse", ".ai"));
        assert!(is_valid_event("Stop", ".ai")); // legacy Copilot name
        assert!(!is_valid_event("TotallyInvalid", ".ai"));
    }
}
