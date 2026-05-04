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

pub use generated::{Engine, EngineSet, ENGINES};
pub use types::{EngineApiSchemaFile, EngineSpec, META_SCHEMA_VERSION};

#[cfg(test)]
mod smoke_tests {
    use super::{Engine, EngineSet, ENGINES, META_SCHEMA_VERSION};

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
    fn meta_schema_version_is_semver_like() {
        let parts: Vec<&str> = META_SCHEMA_VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "expected MAJOR.MINOR.PATCH, got {META_SCHEMA_VERSION}");
        for part in parts {
            assert!(part.chars().all(|c| c.is_ascii_digit()), "non-digit in {part}");
        }
    }
}
