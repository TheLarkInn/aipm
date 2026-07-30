//! Rule: `skill/description-invalid-chars` — description contains `<` or `>`.
//!
//! Copilot CLI injects skill descriptions into an XML-tagged block in the
//! system prompt, so a `<` or `>` in the description corrupts that markup.
//! This is **not** a general YAML restriction and Claude Code does not impose
//! it, so the rule is scoped to skills that can reach Copilot:
//!
//! * skills under `.github/` → always checked (Copilot's own source root);
//! * skills under `.claude/` → never checked;
//! * everything else (`.ai/` plugins) → checked unless the nearest `aipm.toml`
//!   declares an `engines` list that excludes `copilot`.

use std::path::Path;

use libaipm_engine_spec::EngineSet;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

/// Checks that skill descriptions consumed by Copilot contain no angle brackets.
pub struct DescriptionInvalidChars;

impl Rule for DescriptionInvalidChars {
    fn id(&self) -> &'static str {
        "skill/description-invalid-chars"
    }

    fn name(&self) -> &'static str {
        "skill description invalid characters"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn help_url(&self) -> Option<&'static str> {
        Some(
            "https://github.com/TheLarkInn/aipm/blob/main/docs/rules/skill/description-invalid-chars.md",
        )
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("remove `<` and `>` from the description, or exclude copilot from engines")
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        self.check_file_in(file_path, Path::new(""), fs)
    }

    fn check_file_in(
        &self,
        file_path: &Path,
        lint_dir: &Path,
        fs: &dyn Fs,
    ) -> Result<Vec<Diagnostic>, Error> {
        let Some((source_type, skill)) = super::read_skill_preamble(file_path, fs) else {
            return Ok(vec![]);
        };
        if !targets_copilot(file_path, lint_dir, fs) {
            return Ok(vec![]);
        }
        let Some(desc) = skill.field("description") else { return Ok(vec![]) };
        let found: Vec<char> = ['<', '>'].into_iter().filter(|c| desc.contains(*c)).collect();
        if found.is_empty() {
            return Ok(vec![]);
        }
        let desc_line =
            skill.frontmatter.as_ref().and_then(|fm| fm.field_lines.get("description").copied());
        let (col, end_col) = desc_line
            .and_then(|n| skill.content.lines().nth(n.saturating_sub(1)))
            .and_then(|line| crate::frontmatter::field_value_range(line, "description"))
            .unzip();
        let listed = found.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(" and ");
        Ok(vec![Diagnostic {
            rule_id: self.id().to_string(),
            severity: self.default_severity(),
            message: format!(
                "skill description contains {listed}, which Copilot CLI does not allow"
            ),
            file_path: skill.path,
            line: desc_line,
            col,
            end_line: desc_line,
            end_col,
            source_type,
            help_text: None,
            help_url: None,
        }])
    }
}

/// Decide whether the skill at `file_path` can be loaded by Copilot CLI.
fn targets_copilot(file_path: &Path, lint_dir: &Path, fs: &dyn Fs) -> bool {
    match super::scan::source_type_from_path(file_path) {
        libaipm_engine_spec::paths::CLAUDE_DOT => false,
        libaipm_engine_spec::paths::GITHUB_DOT => true,
        _ => {
            let declared =
                super::valid_tool_name::nearest_declared_engines(file_path, lint_dir, fs);
            declared.is_empty() || declared.contains(EngineSet::COPILOT)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::test_helpers::MockFs;
    use std::path::PathBuf;

    const ANGLED: &str = "---\nname: s\ndescription: Use <tool> here\n---\nbody";

    fn check_at(path: &str, content: &str, manifest: Option<(&str, &str)>) -> Vec<Diagnostic> {
        let mut fs = MockFs::new();
        let path = PathBuf::from(path);
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), content.to_string());
        if let Some((manifest_path, manifest_body)) = manifest {
            let manifest_path = PathBuf::from(manifest_path);
            fs.exists.insert(manifest_path.clone());
            fs.files.insert(manifest_path, manifest_body.to_string());
        }
        DescriptionInvalidChars.check_file(&path, &fs).ok().unwrap_or_default()
    }

    fn check(content: &str) -> Vec<Diagnostic> {
        check_at(".ai/p/skills/s/SKILL.md", content, None)
    }

    #[test]
    fn no_file_returns_empty() {
        let fs = MockFs::new();
        let diags = DescriptionInvalidChars
            .check_file(Path::new(".ai/p/skills/s/SKILL.md"), &fs)
            .ok()
            .unwrap_or_default();
        assert!(diags.is_empty());
    }

    #[test]
    fn clean_description_passes() {
        assert!(check("---\nname: s\ndescription: All good\n---\nbody").is_empty());
    }

    #[test]
    fn angle_brackets_are_reported() {
        let diags = check(ANGLED);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/description-invalid-chars");
        assert!(diags[0].message.contains('<'));
        assert_eq!(diags[0].line, Some(3));
        assert_eq!(diags[0].col, Some(14));
    }

    #[test]
    fn only_less_than_is_reported_singly() {
        let diags = check("---\nname: s\ndescription: a < b\n---\nbody");
        assert_eq!(diags.len(), 1);
        assert!(!diags[0].message.contains(" and "));
    }

    #[test]
    fn folded_description_is_checked_after_parsing() {
        let diags = check("---\nname: s\ndescription: >\n  keep <this>\n  out\n---\nbody");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn github_source_is_always_checked() {
        assert_eq!(check_at(".github/skills/s/SKILL.md", ANGLED, None).len(), 1);
    }

    #[test]
    fn claude_source_is_never_checked() {
        assert!(check_at(".claude/skills/s/SKILL.md", ANGLED, None).is_empty());
    }

    #[test]
    fn claude_only_plugin_is_not_checked() {
        let manifest = "[package]\nname = \"p\"\nversion = \"1.0.0\"\nengines = [\"claude\"]\n";
        assert!(check_at(".ai/p/skills/s/SKILL.md", ANGLED, Some((".ai/p/aipm.toml", manifest)))
            .is_empty());
    }

    #[test]
    fn copilot_plugin_is_checked() {
        let manifest = "[package]\nname = \"p\"\nversion = \"1.0.0\"\nengines = [\"copilot\"]\n";
        assert_eq!(
            check_at(".ai/p/skills/s/SKILL.md", ANGLED, Some((".ai/p/aipm.toml", manifest))).len(),
            1
        );
    }

    #[test]
    fn plugin_without_declared_engines_is_checked() {
        let manifest = "[package]\nname = \"p\"\nversion = \"1.0.0\"\n";
        assert_eq!(
            check_at(".ai/p/skills/s/SKILL.md", ANGLED, Some((".ai/p/aipm.toml", manifest))).len(),
            1
        );
    }

    #[test]
    fn missing_description_is_ignored() {
        assert!(check("---\nname: s\n---\nbody").is_empty());
    }

    #[test]
    fn no_frontmatter_is_ignored() {
        assert!(check("no frontmatter").is_empty());
    }

    #[test]
    fn rule_metadata() {
        assert_eq!(DescriptionInvalidChars.name(), "skill description invalid characters");
        assert_eq!(DescriptionInvalidChars.default_severity(), Severity::Warning);
        assert!(DescriptionInvalidChars.help_url().is_some());
        assert!(DescriptionInvalidChars.help_text().is_some());
    }
}
