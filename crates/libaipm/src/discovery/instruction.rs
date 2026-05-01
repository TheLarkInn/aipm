//! Instruction-file classifier for the unified discovery module.
//!
//! Recognizes three filename shapes (case-insensitively):
//!
//! 1. **Exact name in [`INSTRUCTION_FILENAMES`]** — `claude.md`, `agents.md`,
//!    `copilot.md`, `instructions.md`, `gemini.md` (today's behavior preserved
//!    from `discovery_legacy::INSTRUCTION_FILENAMES`).
//! 2. **`.instructions.md` suffix** — anything matching `*.instructions.md`
//!    (today's behavior preserved from
//!    `discovery_legacy::classify_feature_kind`).
//! 3. **`<engine>-instructions.md`** — closes the issue #725 silent-drop gap:
//!    `copilot-instructions.md`, `claude-instructions.md`, `agents-instructions.md`,
//!    `gemini-instructions.md` are all recognized.
//!
//! Implementation note: the spec text suggested a `regex` crate `Lazy` static
//! to handle case (3). Since `regex` is not a direct dependency of `libaipm`
//! and the pattern is fixed (one of four prefixes plus a literal suffix),
//! plain `str::strip_suffix` + `matches!` keeps the deps minimal AND avoids
//! the `.unwrap()` that the spec flagged as a concern in `Lazy<Regex>`
//! constructors.

use std::path::Path;

use super::types::{DiscoveredFeature, Engine, FeatureKind, Layout};

/// Lowercase filenames that are unconditionally classified as instruction files.
///
/// Preserved verbatim from `discovery_legacy.rs:185-186`.
pub const INSTRUCTION_FILENAMES: &[&str] =
    &["claude.md", "agents.md", "copilot.md", "instructions.md", "gemini.md"];

/// Engine prefixes accepted in the `<engine>-instructions.md` shape.
const ENGINE_INSTRUCTION_PREFIXES: &[&str] = &["copilot", "claude", "agents", "gemini"];

/// Try to classify `path` as an instruction file based on its filename.
///
/// Returns `Some(DiscoveredFeature)` with `kind = FeatureKind::Instructions`,
/// `layout = Layout::Canonical`, and `feature_dir = None` (instruction files
/// have no enclosing per-feature directory).
///
/// The match is case-insensitive across all three shapes. The `engine` and
/// `source_root` are taken from the caller verbatim — typically the dispatcher
/// calls this with the engine inferred via [`crate::discovery::infer_engine_root`].
#[must_use]
pub fn classify(
    file_name: &str,
    path: &Path,
    engine: Engine,
    source_root: &Path,
) -> Option<DiscoveredFeature> {
    let lower = file_name.to_ascii_lowercase();
    if !is_instruction_filename(&lower) {
        return None;
    }
    Some(DiscoveredFeature {
        kind: FeatureKind::Instructions,
        engine,
        layout: Layout::Canonical,
        source_root: source_root.to_path_buf(),
        feature_dir: None,
        path: path.to_path_buf(),
    })
}

/// Check whether a lowercase filename matches any of the three instruction shapes.
fn is_instruction_filename(file_name_lower: &str) -> bool {
    INSTRUCTION_FILENAMES.contains(&file_name_lower)
        || file_name_lower.ends_with(".instructions.md")
        || matches_engine_instructions(file_name_lower)
}

/// Match `<engine>-instructions.md` where `<engine>` is one of the recognized
/// prefixes. Input must already be lowercase.
fn matches_engine_instructions(file_name_lower: &str) -> bool {
    let Some(prefix) = file_name_lower.strip_suffix("-instructions.md") else {
        return false;
    };
    ENGINE_INSTRUCTION_PREFIXES.contains(&prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn classify_with(name: &str) -> Option<DiscoveredFeature> {
        let root = PathBuf::from("/repo/.github");
        let path = PathBuf::from(format!("/repo/.github/{name}"));
        classify(name, &path, Engine::Copilot, &root)
    }

    // --- Case A: exact filenames in INSTRUCTION_FILENAMES ---

    #[test]
    fn claude_md_matches() {
        let feat = classify_with("claude.md").expect("should match");
        assert_eq!(feat.kind, FeatureKind::Instructions);
        assert_eq!(feat.layout, Layout::Canonical);
        assert_eq!(feat.feature_dir, None);
    }

    #[test]
    fn agents_md_matches() {
        assert!(classify_with("agents.md").is_some());
    }

    #[test]
    fn copilot_md_matches() {
        assert!(classify_with("copilot.md").is_some());
    }

    #[test]
    fn instructions_md_matches() {
        assert!(classify_with("instructions.md").is_some());
    }

    #[test]
    fn gemini_md_matches() {
        assert!(classify_with("gemini.md").is_some());
    }

    #[test]
    fn case_insensitive_for_table_match() {
        // CLAUDE.md (uppercase) — common spelling.
        assert!(classify_with("CLAUDE.md").is_some());
        assert!(classify_with("AGENTS.md").is_some());
    }

    // --- Case B: .instructions.md suffix ---

    #[test]
    fn arbitrary_instructions_md_suffix_matches() {
        assert!(classify_with("my-thing.instructions.md").is_some());
        assert!(classify_with("foo.instructions.md").is_some());
        assert!(classify_with("very.long.name.instructions.md").is_some());
    }

    #[test]
    fn instructions_md_suffix_case_insensitive() {
        assert!(classify_with("MyThing.Instructions.md").is_some());
    }

    // --- Case C: <engine>-instructions.md (the #725 fix) ---

    #[test]
    fn copilot_instructions_md_matches_issue_725() {
        // The exact second silent-drop case from issue #725.
        let feat = classify_with("copilot-instructions.md").expect("should match");
        assert_eq!(feat.kind, FeatureKind::Instructions);
    }

    #[test]
    fn claude_instructions_md_matches() {
        assert!(classify_with("claude-instructions.md").is_some());
    }

    #[test]
    fn agents_instructions_md_matches() {
        assert!(classify_with("agents-instructions.md").is_some());
    }

    #[test]
    fn gemini_instructions_md_matches() {
        assert!(classify_with("gemini-instructions.md").is_some());
    }

    #[test]
    fn engine_instructions_md_case_insensitive() {
        assert!(classify_with("COPILOT-INSTRUCTIONS.md").is_some());
        assert!(classify_with("Copilot-Instructions.MD").is_some());
    }

    // --- negative cases ---

    #[test]
    fn random_md_does_not_match() {
        assert!(classify_with("random.md").is_none());
    }

    #[test]
    fn instructions_copilot_md_wrong_order_no_match() {
        // Reversed order — not the same as `copilot-instructions.md`.
        assert!(classify_with("instructions-copilot.md").is_none());
    }

    #[test]
    fn unknown_engine_prefix_no_match() {
        // `cursor-instructions.md` — `cursor` is not in
        // ENGINE_INSTRUCTION_PREFIXES (out of scope per NG1 of the spec).
        assert!(classify_with("cursor-instructions.md").is_none());
    }

    #[test]
    fn copilot_tools_md_does_not_match() {
        // Random file with copilot prefix but wrong suffix.
        assert!(classify_with("copilot-tools.md").is_none());
    }

    #[test]
    fn copilot_instructions_md_with_extra_suffix_no_match() {
        // `.bak` extension breaks the match — the file no longer ends with `.md`.
        assert!(classify_with("copilot-instructions.md.bak").is_none());
    }

    #[test]
    fn empty_filename_no_match() {
        assert!(classify_with("").is_none());
    }

    #[test]
    fn just_dash_instructions_md_no_match() {
        // `-instructions.md` with no engine prefix.
        assert!(classify_with("-instructions.md").is_none());
    }

    // --- structural fields ---

    #[test]
    fn classify_returns_path_unchanged() {
        let root = PathBuf::from("/repo/.github");
        let path = PathBuf::from("/repo/.github/copilot/copilot-instructions.md");
        let feat = classify("copilot-instructions.md", &path, Engine::Copilot, &root)
            .expect("should match");
        assert_eq!(feat.path, path);
        assert_eq!(feat.source_root, root);
        assert_eq!(feat.engine, Engine::Copilot);
        assert!(feat.feature_dir.is_none());
    }

    #[test]
    fn classify_passes_engine_from_caller() {
        let root = PathBuf::from("/repo/.claude");
        let path = PathBuf::from("/repo/.claude/CLAUDE.md");
        let feat = classify("CLAUDE.md", &path, Engine::Claude, &root).expect("should match");
        assert_eq!(feat.engine, Engine::Claude);
    }
}
