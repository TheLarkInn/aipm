//! Instruction-file classifier for the unified discovery module.
//!
//! Recognizes two filename shapes (case-insensitively):
//!
//! 1. **Exact name in [`INSTRUCTION_FILENAMES`]** — `claude.md`, `agents.md`,
//!    `copilot.md`, `instructions.md`, `gemini.md`, plus the real GitHub
//!    Copilot repository-instructions filename `copilot-instructions.md`.
//! 2. **`.instructions.md` suffix** — anything matching `*.instructions.md`
//!    (today's behavior preserved from
//!    `discovery_legacy::classify_feature_kind`).
//!
//! A previous third shape `<engine>-instructions.md` covering all of
//! `claude-/agents-/gemini-/copilot-instructions.md` was withdrawn after
//! engine-documentation verification — see
//! `specs/2026-05-02-engine-instructions-md-pattern-removal.md`. No engine
//! reads files literally named `claude-instructions.md` /
//! `agents-instructions.md` / `gemini-instructions.md`. The lone real name
//! `copilot-instructions.md` (GitHub Copilot's repository-level instructions
//! file at `.github/copilot-instructions.md`) is preserved here in
//! [`INSTRUCTION_FILENAMES`] because Copilot does read it; the engine-prefix
//! family was the wrong abstraction, but the bare filename is correct.

use std::path::Path;

use std::sync::OnceLock;

use libaipm_engine_spec::ENGINES;

use super::types::{DiscoveredFeature, DiscoverySource, FeatureKind, Layout};

/// Filenames that are unconditionally classified as instruction files.
///
/// Aggregated lazily from `libaipm_engine_spec::ENGINES`'s `convention_files`
/// (so adding a new engine's convention names in the schema picks up here
/// automatically) plus two legacy names (`copilot.md`, `instructions.md`)
/// that historical installers wrote but the schema doesn't track.
///
/// The schema entries retain their original case (e.g. `CLAUDE.md`,
/// `AGENTS.md`); matching is case-insensitive via [`str::eq_ignore_ascii_case`].
fn instruction_filenames() -> &'static [&'static str] {
    static FILENAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    FILENAMES
        .get_or_init(|| {
            let mut names: Vec<&'static str> = ENGINES
                .iter()
                .flat_map(|(_, spec)| spec.convention_files.iter().map(|(f, _)| *f))
                .collect();
            // Legacy heuristic names not tracked in the schema's
            // convention_files but still recognised by historical
            // installers.
            names.extend_from_slice(&["copilot.md", "instructions.md"]);
            names.sort_unstable();
            names.dedup();
            names
        })
        .as_slice()
}

/// Try to classify `path` as an instruction file based on its filename.
///
/// Returns `Some(DiscoveredFeature)` with `kind = FeatureKind::Instructions`,
/// `layout = Layout::Canonical`, and `feature_dir = None` (instruction files
/// have no enclosing per-feature directory).
///
/// The match is case-insensitive across both shapes. The `source` and
/// `source_root` are taken from the caller verbatim — typically the dispatcher
/// calls this with the source inferred via [`crate::discovery::infer_engine_root`].
#[must_use]
pub fn classify(
    file_name: &str,
    path: &Path,
    source: DiscoverySource,
    source_root: &Path,
) -> Option<DiscoveredFeature> {
    let lower = file_name.to_ascii_lowercase();
    if !is_instruction_filename(&lower) {
        return None;
    }
    Some(DiscoveredFeature {
        kind: FeatureKind::Instructions,
        source,
        layout: Layout::Canonical,
        source_root: source_root.to_path_buf(),
        feature_dir: None,
        path: path.to_path_buf(),
    })
}

/// Check whether a lowercase filename matches any of the two instruction shapes.
fn is_instruction_filename(file_name_lower: &str) -> bool {
    instruction_filenames().iter().copied().any(|f| f.eq_ignore_ascii_case(file_name_lower))
        || file_name_lower.ends_with(".instructions.md")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn classify_with(name: &str) -> Option<DiscoveredFeature> {
        let root = PathBuf::from("/repo/.github");
        let path = PathBuf::from(format!("/repo/.github/{name}"));
        classify(name, &path, DiscoverySource::COPILOT, &root)
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
    fn copilot_instructions_md_matches_via_table() {
        // GitHub Copilot's documented repository-instructions file. Recognized
        // via INSTRUCTION_FILENAMES (Case A), not via the withdrawn engine-
        // prefix branch.
        let feat = classify_with("copilot-instructions.md").expect("should match");
        assert_eq!(feat.kind, FeatureKind::Instructions);
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

    // --- negative cases ---

    #[test]
    fn random_md_does_not_match() {
        assert!(classify_with("random.md").is_none());
    }

    #[test]
    fn empty_filename_no_match() {
        assert!(classify_with("").is_none());
    }

    // --- structural fields ---

    #[test]
    fn classify_returns_path_unchanged() {
        let root = PathBuf::from("/repo/.github");
        let path = PathBuf::from("/repo/.github/instructions.md");
        let feat = classify("instructions.md", &path, DiscoverySource::COPILOT, &root)
            .expect("should match");
        assert_eq!(feat.path, path);
        assert_eq!(feat.source_root, root);
        assert_eq!(feat.source, DiscoverySource::COPILOT);
        assert!(feat.feature_dir.is_none());
    }

    #[test]
    fn classify_passes_source_from_caller() {
        let root = PathBuf::from("/repo/.claude");
        let path = PathBuf::from("/repo/.claude/CLAUDE.md");
        let feat =
            classify("CLAUDE.md", &path, DiscoverySource::CLAUDE, &root).expect("should match");
        assert_eq!(feat.source, DiscoverySource::CLAUDE);
    }

    // --- regression guard ---

    #[test]
    fn engine_instructions_md_family_not_classified() {
        // Regression guard: see specs/2026-05-02-engine-instructions-md-pattern-removal.md.
        // Engine-documentation verification confirmed no engine reads files
        // literally named claude-/agents-/gemini-instructions.md. The
        // classifier MUST NOT re-introduce a generic `<engine>-instructions.md`
        // family. (Note: `copilot-instructions.md` IS classified — but via
        // INSTRUCTION_FILENAMES, the legitimate Case A path, because Copilot
        // does read it at `.github/copilot-instructions.md`. See
        // `copilot_instructions_md_matches_via_table` above.)
        assert!(classify_with("claude-instructions.md").is_none());
        assert!(classify_with("agents-instructions.md").is_none());
        assert!(classify_with("gemini-instructions.md").is_none());
    }
}
