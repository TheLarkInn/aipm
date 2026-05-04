//! Engine API schema source-of-truth for AIPM.
//!
//! This crate hosts the canonical Rust types for the engine API schema and the
//! build-time generated tables (engines, tools, hook events, paths,
//! constraints) consumed by the rest of the workspace.
//!
//! The data file at `data/engine-api-schema.json` is treated as the single
//! source of truth: `build.rs` validates it against `schemas/engine-api.schema.json`,
//! parses it, and emits typed const tables into `OUT_DIR/engine_data.rs`.

pub mod generated;
pub mod helpers;
pub mod types;

pub use generated::{constraints, paths};
pub use generated::{
    Engine, EngineSet, ENGINES, FEATURES_BY_ENGINE, HOOK_EVENTS_BY_ENGINE, TOOL_COMPATIBILITY,
    VALID_TOOLS,
};
pub use types::{
    EngineApiSchemaFile, EngineFeatureSet, EngineSpec, FeatureKind, HookEventStatic,
    META_SCHEMA_VERSION,
};

#[cfg(test)]
mod smoke_tests {
    use super::{
        Engine, EngineFeatureSet, EngineSet, HookEventStatic, ENGINES, FEATURES_BY_ENGINE,
        HOOK_EVENTS_BY_ENGINE, META_SCHEMA_VERSION, TOOL_COMPATIBILITY, VALID_TOOLS,
    };

    #[test]
    fn engine_all_lists_known_variants() {
        let all = Engine::ALL;
        assert!(all.contains(&Engine::Claude));
        assert!(all.contains(&Engine::CopilotCli));
        assert!(all.len() >= 2);
    }

    #[test]
    fn engine_name_round_trips() {
        for &engine in Engine::ALL {
            let name = engine.name();
            assert_eq!(Engine::from_name(name), Some(engine), "name = {name}");
        }
        assert_eq!(Engine::from_name("not-a-real-engine"), None);
    }

    #[test]
    fn engine_set_all_is_union_of_individual_bits() {
        let union = EngineSet::CLAUDE | EngineSet::COPILOT_CLI;
        assert_eq!(EngineSet::ALL, union);
        assert!(EngineSet::ALL.contains(EngineSet::CLAUDE));
        assert!(EngineSet::ALL.contains(EngineSet::COPILOT_CLI));
    }

    #[test]
    fn engines_const_has_at_least_two_entries() {
        assert!(ENGINES.len() >= 2, "expected ≥ 2 engines, got {}", ENGINES.len());
        let names: Vec<&str> = ENGINES.iter().map(|(_, spec)| spec.name).collect();
        assert!(names.contains(&"claude"));
        assert!(names.contains(&"copilot-cli"));
    }

    #[test]
    fn engines_const_marketplace_paths_match_schema_wins_decision() {
        let claude = ENGINES.iter().find(|(e, _)| *e == Engine::Claude).map(|(_, s)| s);
        let copilot = ENGINES.iter().find(|(e, _)| *e == Engine::CopilotCli).map(|(_, s)| s);
        assert_eq!(
            claude.map(|s| s.marketplace_manifest_path),
            Some(".claude-plugin/marketplace.toml")
        );
        assert_eq!(
            copilot.map(|s| s.marketplace_manifest_path),
            Some(".github/plugin/marketplace.json")
        );
    }

    #[test]
    fn valid_tools_contains_known_names_and_aliases() {
        assert!(!VALID_TOOLS.is_empty());
        for expected in &["bash", "Bash", "Edit", "Read", "Write", "browser_navigate"] {
            assert!(VALID_TOOLS.contains(expected), "VALID_TOOLS missing {expected}");
        }
    }

    #[test]
    fn valid_tools_rejects_unknown_names() {
        assert!(!VALID_TOOLS.contains("definitely-not-a-real-tool"));
        assert!(!VALID_TOOLS.contains(""));
    }

    fn tool_compat_lookup(name: &str) -> Option<EngineSet> {
        TOOL_COMPATIBILITY.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
    }

    #[test]
    fn tool_compatibility_shared_tools_map_to_all() {
        assert_eq!(tool_compat_lookup("bash"), Some(EngineSet::ALL));
        assert_eq!(tool_compat_lookup("glob"), Some(EngineSet::ALL));
        assert_eq!(tool_compat_lookup("grep"), Some(EngineSet::ALL));
        assert_eq!(tool_compat_lookup("web_fetch"), Some(EngineSet::ALL));
    }

    #[test]
    fn tool_compatibility_claude_exclusive_tools_map_to_claude_only() {
        assert_eq!(tool_compat_lookup("Task"), Some(EngineSet::CLAUDE));
        assert_eq!(tool_compat_lookup("Edit"), Some(EngineSet::CLAUDE));
        let task = tool_compat_lookup("Task").expect("Task missing");
        assert!(task.contains(EngineSet::CLAUDE));
        assert!(!task.contains(EngineSet::COPILOT_CLI));
    }

    #[test]
    fn tool_compatibility_copilot_exclusive_tools_map_to_copilot_only() {
        assert_eq!(tool_compat_lookup("browser_navigate"), Some(EngineSet::COPILOT_CLI));
        assert_eq!(tool_compat_lookup("get_pull_request"), Some(EngineSet::COPILOT_CLI));
        let nav = tool_compat_lookup("browser_navigate").expect("browser_navigate missing");
        assert!(nav.contains(EngineSet::COPILOT_CLI));
        assert!(!nav.contains(EngineSet::CLAUDE));
    }

    fn hook_events_for(engine: Engine) -> &'static [HookEventStatic] {
        HOOK_EVENTS_BY_ENGINE.iter().find(|(e, _)| *e == engine).map_or(&[], |(_, evs)| *evs)
    }

    #[test]
    fn hook_events_by_engine_has_one_entry_per_variant() {
        for &engine in Engine::ALL {
            let events = hook_events_for(engine);
            assert!(!events.is_empty(), "{engine:?} has no hook events");
        }
    }

    #[test]
    fn hook_events_claude_count_matches_known_baseline() {
        assert_eq!(hook_events_for(Engine::Claude).len(), 27);
    }

    #[test]
    fn hook_events_copilot_count_matches_known_baseline() {
        assert_eq!(hook_events_for(Engine::CopilotCli).len(), 10);
    }

    #[test]
    fn hook_events_copilot_pre_tool_use_carries_pascal_case_alias() {
        let pre = hook_events_for(Engine::CopilotCli)
            .iter()
            .find(|e| e.name == "preToolUse")
            .expect("preToolUse missing from copilot hook events");
        assert!(
            pre.aliases.contains(&"PreToolUse"),
            "expected legacy alias PreToolUse, got {:?}",
            pre.aliases
        );
    }

    fn features_for(engine: Engine) -> EngineFeatureSet {
        FEATURES_BY_ENGINE
            .iter()
            .find(|(e, _)| *e == engine)
            .map_or(EngineFeatureSet::empty(), |(_, s)| *s)
    }

    #[test]
    fn features_claude_carries_skill_agent_mcp_hook_output_style() {
        let f = features_for(Engine::Claude);
        for bit in [
            EngineFeatureSet::SKILL,
            EngineFeatureSet::AGENT,
            EngineFeatureSet::MCP,
            EngineFeatureSet::HOOK,
            EngineFeatureSet::OUTPUT_STYLE,
        ] {
            assert!(f.contains(bit), "claude missing {bit:?}");
        }
        assert!(!f.contains(EngineFeatureSet::LSP));
        assert!(!f.contains(EngineFeatureSet::EXTENSION));
    }

    #[test]
    fn features_copilot_carries_skill_agent_mcp_hook_lsp_extension() {
        let f = features_for(Engine::CopilotCli);
        for bit in [
            EngineFeatureSet::SKILL,
            EngineFeatureSet::AGENT,
            EngineFeatureSet::MCP,
            EngineFeatureSet::HOOK,
            EngineFeatureSet::LSP,
            EngineFeatureSet::EXTENSION,
        ] {
            assert!(f.contains(bit), "copilot missing {bit:?}");
        }
        assert!(!f.contains(EngineFeatureSet::OUTPUT_STYLE));
    }

    #[test]
    fn paths_module_constants_have_expected_values() {
        use super::paths;
        assert_eq!(paths::CLAUDE_PLUGIN_DIR, ".claude-plugin");
        assert_eq!(paths::GITHUB_PLUGIN_DIR, ".github/plugin");
        assert_eq!(paths::MARKETPLACE_JSON, "marketplace.json");
        assert_eq!(paths::MARKETPLACE_TOML, "marketplace.toml");
        assert_eq!(paths::PLUGIN_JSON, "plugin.json");
        assert_eq!(paths::PLUGIN_TOML, "plugin.toml");
        assert_eq!(paths::AIPM_TOML, "aipm.toml");
        assert_eq!(paths::SETTINGS_JSON, "settings.json");
        assert_eq!(paths::SETTINGS_LOCAL_JSON, "settings.local.json");
        assert_eq!(paths::CLAUDE_DOT, ".claude");
        assert_eq!(paths::GITHUB_DOT, ".github");
        assert_eq!(paths::AI_DOT, ".ai");
    }

    #[test]
    fn constraints_constants_match_schema_values() {
        use super::constraints;
        assert_eq!(constraints::PLUGIN_NAME_MAX_LEN, 64);
        assert_eq!(constraints::DESCRIPTION_MAX_LEN, 1024);
        assert_eq!(constraints::POST_INSTALL_MSG_MAX_LEN, 2048);
        assert_eq!(constraints::PLUGIN_NAME_REGEX, "^[a-zA-Z0-9-]+$");
    }

    #[test]
    fn meta_schema_version_is_semver_like() {
        let parts: Vec<&str> = META_SCHEMA_VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "expected MAJOR.MINOR.PATCH, got {META_SCHEMA_VERSION}");
        for part in parts {
            assert!(part.chars().all(|c| c.is_ascii_digit()), "non-digit in {part}");
        }
    }
}
