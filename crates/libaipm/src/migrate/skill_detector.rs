//! Skill detector: scans `.claude/skills/` for directories containing `SKILL.md`.

use std::path::Path;

use crate::fs::Fs;

use super::detector::Detector;
use super::skill_common;
use super::{Artifact, ArtifactKind, Error};

/// Scans `.claude/skills/` for directories containing `SKILL.md`.
pub struct SkillDetector;

impl Detector for SkillDetector {
    fn name(&self) -> &'static str {
        "skill"
    }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        let skills_dir = source_dir.join("skills");
        if !fs.exists(&skills_dir) {
            return Ok(Vec::new());
        }

        let entries = fs.read_dir(&skills_dir)?;
        let mut artifacts = Vec::new();

        for entry in entries {
            if !entry.is_dir {
                continue;
            }

            let entry_dir = skills_dir.join(&entry.name);
            let skill_md = entry_dir.join("SKILL.md");

            if !fs.exists(&skill_md) {
                continue;
            }

            let content = fs.read_to_string(&skill_md)?;
            let metadata = skill_common::parse_skill_frontmatter(&content, &skill_md)?;
            let files = skill_common::collect_files_recursive(&entry_dir, &entry_dir, fs)?;
            let referenced_scripts =
                skill_common::extract_script_references(&content, "${CLAUDE_SKILL_DIR}/");

            let name = metadata.name.clone().unwrap_or_else(|| entry.name.clone());

            artifacts.push(Artifact {
                kind: ArtifactKind::Skill,
                name,
                source_path: entry_dir,
                files,
                referenced_scripts,
                metadata,
            });
        }

        Ok(artifacts)
    }
}

/// Extract script references from SKILL.md content using the Claude skill dir prefix.
///
/// This is a convenience wrapper around `skill_common::extract_script_references`
/// that uses the `${CLAUDE_SKILL_DIR}/` prefix. Kept public for `command_detector`.
pub fn extract_script_references(content: &str) -> Vec<std::path::PathBuf> {
    skill_common::extract_script_references(content, "${CLAUDE_SKILL_DIR}/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill_common::parse_skill_frontmatter;
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;
    use std::sync::Mutex;

    struct MockFs {
        exists: HashSet<PathBuf>,
        dirs: HashMap<PathBuf, Vec<crate::fs::DirEntry>>,
        files: HashMap<PathBuf, String>,
        written: Mutex<HashMap<PathBuf, Vec<u8>>>,
    }

    impl MockFs {
        fn new() -> Self {
            Self {
                exists: HashSet::new(),
                dirs: HashMap::new(),
                files: HashMap::new(),
                written: Mutex::new(HashMap::new()),
            }
        }
    }

    impl crate::fs::Fs for MockFs {
        fn exists(&self, path: &Path) -> bool {
            self.exists.contains(path)
        }

        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            Ok(())
        }

        fn write_file(&self, path: &Path, content: &[u8]) -> std::io::Result<()> {
            self.written
                .lock()
                .expect("MockFs::write_file: mutex poisoned")
                .insert(path.to_path_buf(), content.to_vec());
            Ok(())
        }

        fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
            self.files.get(path).cloned().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("not found: {}", path.display()),
                )
            })
        }

        fn read_dir(&self, path: &Path) -> std::io::Result<Vec<crate::fs::DirEntry>> {
            self.dirs.get(path).cloned().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("dir not found: {}", path.display()),
                )
            })
        }
    }

    fn de(name: &str, is_dir: bool) -> crate::fs::DirEntry {
        crate::fs::DirEntry { name: name.to_string(), is_dir }
    }

    #[test]
    fn detect_skill_with_skill_md() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/skills"));
        fs.exists.insert(PathBuf::from("/src/skills/deploy/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/src/skills"), vec![de("deploy", true)]);
        fs.dirs.insert(PathBuf::from("/src/skills/deploy"), vec![de("SKILL.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/skills/deploy/SKILL.md"),
            "---\nname: deploy\n---\nDeploy stuff".to_string(),
        );

        let detector = SkillDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("deploy"));
        assert_eq!(artifacts.first().map(|a| &a.kind), Some(&ArtifactKind::Skill));
    }

    #[test]
    fn detect_skill_without_skill_md() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/skills"));
        // deploy dir exists but no SKILL.md
        fs.dirs.insert(PathBuf::from("/src/skills"), vec![de("deploy", true)]);
        fs.dirs.insert(PathBuf::from("/src/skills/deploy"), vec![de("readme.md", false)]);

        let detector = SkillDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_skill_with_scripts() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/skills"));
        fs.exists.insert(PathBuf::from("/src/skills/deploy/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/src/skills"), vec![de("deploy", true)]);
        fs.dirs.insert(PathBuf::from("/src/skills/deploy"), vec![de("SKILL.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/skills/deploy/SKILL.md"),
            "---\nname: deploy\n---\nRun `${CLAUDE_SKILL_DIR}/scripts/deploy.sh`".to_string(),
        );

        let detector = SkillDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| a.referenced_scripts.len()), Some(1));
    }

    #[test]
    fn detect_skill_extracts_frontmatter() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/skills"));
        fs.exists.insert(PathBuf::from("/src/skills/deploy/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/src/skills"), vec![de("deploy", true)]);
        fs.dirs.insert(PathBuf::from("/src/skills/deploy"), vec![de("SKILL.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/skills/deploy/SKILL.md"),
            "---\nname: my-deploy\ndescription: Deploy app\nhooks:\n  PreToolUse: check\n---\nBody"
                .to_string(),
        );

        let detector = SkillDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("my-deploy"));
        assert_eq!(
            artifacts.first().and_then(|a| a.metadata.description.as_deref()),
            Some("Deploy app")
        );
        assert!(artifacts.first().and_then(|a| a.metadata.hooks.as_ref()).is_some());
    }

    #[test]
    fn detect_skill_no_skills_dir() {
        let fs = MockFs::new();
        // .claude/ exists but no skills/ subdirectory
        let detector = SkillDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_skill_empty_skills_dir() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/skills"));
        fs.dirs.insert(PathBuf::from("/src/skills"), Vec::new());

        let detector = SkillDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_multiple_skills() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/skills"));
        fs.exists.insert(PathBuf::from("/src/skills/deploy/SKILL.md"));
        fs.exists.insert(PathBuf::from("/src/skills/lint/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/src/skills"), vec![de("deploy", true), de("lint", true)]);
        fs.dirs.insert(PathBuf::from("/src/skills/deploy"), vec![de("SKILL.md", false)]);
        fs.dirs.insert(PathBuf::from("/src/skills/lint"), vec![de("SKILL.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/skills/deploy/SKILL.md"),
            "---\nname: deploy\n---\nDeploy".to_string(),
        );
        fs.files.insert(
            PathBuf::from("/src/skills/lint/SKILL.md"),
            "---\nname: lint\n---\nLint".to_string(),
        );

        let detector = SkillDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 2);
    }

    #[test]
    fn detect_skill_nested_files() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/skills"));
        fs.exists.insert(PathBuf::from("/src/skills/deploy/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/src/skills"), vec![de("deploy", true)]);
        fs.dirs.insert(
            PathBuf::from("/src/skills/deploy"),
            vec![de("SKILL.md", false), de("reference.md", false), de("examples", true)],
        );
        fs.dirs.insert(PathBuf::from("/src/skills/deploy/examples"), vec![de("demo.sh", false)]);
        fs.files.insert(
            PathBuf::from("/src/skills/deploy/SKILL.md"),
            "---\nname: deploy\n---\nContent".to_string(),
        );

        let detector = SkillDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        // Should have 3 files: SKILL.md, reference.md, examples/demo.sh
        assert_eq!(artifacts.first().map(|a| a.files.len()), Some(3));
    }

    #[test]
    fn parse_frontmatter_no_frontmatter() {
        let result = parse_skill_frontmatter("just plain text", Path::new("test"));
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert!(meta.name.is_none());
    }

    #[test]
    fn parse_frontmatter_no_closing() {
        let result = parse_skill_frontmatter("---\nname: test\nno closing", Path::new("test"));
        assert!(result.is_err());
    }

    #[test]
    fn parse_frontmatter_with_disable_model_invocation() {
        let result = parse_skill_frontmatter(
            "---\nname: test\ndisable-model-invocation: true\n---\nbody",
            Path::new("test"),
        );
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert!(meta.model_invocation_disabled);
    }

    #[test]
    fn parse_frontmatter_hooks_with_inline_value() {
        let result =
            parse_skill_frontmatter("---\nhooks: inline-hook-value\n---\nbody", Path::new("test"));
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert!(meta.hooks.is_some());
    }

    #[test]
    fn extract_scripts_none() {
        let scripts = extract_script_references("no script references");
        assert!(scripts.is_empty());
    }

    #[test]
    fn extract_scripts_multiple() {
        let content = "Run ${CLAUDE_SKILL_DIR}/scripts/a.sh and ${CLAUDE_SKILL_DIR}/scripts/b.sh";
        let scripts = extract_script_references(content);
        assert_eq!(scripts.len(), 2);
    }

    #[test]
    fn extract_scripts_non_script_path() {
        let content = "Use ${CLAUDE_SKILL_DIR}/readme.md";
        let scripts = extract_script_references(content);
        assert!(scripts.is_empty());
    }

    #[test]
    fn extract_scripts_with_quotes() {
        let content = r#"Run "${CLAUDE_SKILL_DIR}/scripts/deploy.sh" now"#;
        let scripts = extract_script_references(content);
        assert_eq!(scripts.len(), 1);
    }

    #[test]
    fn detect_skill_uses_dir_name_when_no_frontmatter_name() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/skills"));
        fs.exists.insert(PathBuf::from("/src/skills/my-skill/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/src/skills"), vec![de("my-skill", true)]);
        fs.dirs.insert(PathBuf::from("/src/skills/my-skill"), vec![de("SKILL.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/skills/my-skill/SKILL.md"),
            "---\ndescription: no name field\n---\ncontent".to_string(),
        );

        let detector = SkillDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("my-skill"));
    }

    #[test]
    fn parse_frontmatter_with_hooks_multiline() {
        let result = parse_skill_frontmatter(
            "---\nhooks:\n  PreToolUse: check\n  PostToolUse: log\n---\nbody",
            Path::new("test"),
        );
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert!(meta.hooks.is_some());
        if let Some(hooks) = meta.hooks {
            assert!(hooks.contains("PreToolUse"));
        }
    }

    #[test]
    fn parse_frontmatter_empty_name() {
        let result =
            parse_skill_frontmatter("---\nname:\ndescription: test\n---\nbody", Path::new("test"));
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        // Empty name value should be Some("")
        assert!(meta.name.is_some());
    }

    #[test]
    fn parse_frontmatter_disable_model_invocation_false() {
        let result = parse_skill_frontmatter(
            "---\ndisable-model-invocation: false\n---\nbody",
            Path::new("test"),
        );
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert!(!meta.model_invocation_disabled);
    }

    #[test]
    fn extract_scripts_backtick_terminated() {
        let content = "Run `${CLAUDE_SKILL_DIR}/scripts/deploy.sh`";
        let scripts = extract_script_references(content);
        assert_eq!(scripts.len(), 1);
    }

    #[test]
    fn extract_scripts_paren_terminated() {
        let content = "$(${CLAUDE_SKILL_DIR}/scripts/deploy.sh)";
        let scripts = extract_script_references(content);
        assert_eq!(scripts.len(), 1);
    }

    #[test]
    fn parse_frontmatter_hooks_with_tab_indent() {
        let result = parse_skill_frontmatter(
            "---\nhooks:\n\tPreToolUse: check\n---\nbody",
            Path::new("test"),
        );
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert!(meta.hooks.is_some());
    }

    #[test]
    fn parse_frontmatter_unknown_key() {
        let result = parse_skill_frontmatter(
            "---\nunknown-key: some-value\nname: test\n---\nbody",
            Path::new("test"),
        );
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert_eq!(meta.name.as_deref(), Some("test"));
    }

    #[test]
    fn extract_scripts_end_of_line() {
        // Script reference at end of line (no terminator character)
        let content = "Run ${CLAUDE_SKILL_DIR}/scripts/deploy.sh";
        let scripts = extract_script_references(content);
        assert_eq!(scripts.len(), 1);
    }

    #[test]
    fn extract_scripts_single_quote_terminated() {
        let content = "'${CLAUDE_SKILL_DIR}/scripts/deploy.sh'";
        let scripts = extract_script_references(content);
        assert_eq!(scripts.len(), 1);
    }

    #[test]
    fn detect_skill_skips_non_dir_entries() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/skills"));
        fs.dirs.insert(PathBuf::from("/src/skills"), vec![de("readme.txt", false)]);

        let detector = SkillDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_skill_strips_quoted_description() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/skills"));
        fs.exists.insert(PathBuf::from("/src/skills/deploy/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/src/skills"), vec![de("deploy", true)]);
        fs.dirs.insert(PathBuf::from("/src/skills/deploy"), vec![de("SKILL.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/skills/deploy/SKILL.md"),
            "---\nname: \"my-deploy\"\ndescription: \"Deploy app\"\n---\nBody".to_string(),
        );

        let detector = SkillDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("my-deploy"));
        assert_eq!(
            artifacts.first().and_then(|a| a.metadata.description.as_deref()),
            Some("Deploy app")
        );
    }
}
