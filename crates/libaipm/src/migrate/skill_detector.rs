//! Skill detector: scans `.claude/skills/` for directories containing `SKILL.md`.

use std::path::{Path, PathBuf};

use crate::fs::Fs;

use super::detector::Detector;
use super::{Artifact, ArtifactKind, ArtifactMetadata, Error};

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
            let metadata = parse_skill_frontmatter(&content, &skill_md)?;
            let files = collect_files_recursive(&entry_dir, &entry_dir, fs)?;
            let referenced_scripts = extract_script_references(&content);

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

/// Parse YAML frontmatter from a SKILL.md file.
///
/// Frontmatter is delimited by `---` lines. Extracts `name`, `description`,
/// and `hooks` fields using simple line-by-line parsing (no YAML parser).
fn parse_skill_frontmatter(content: &str, path: &Path) -> Result<ArtifactMetadata, Error> {
    let mut metadata = ArtifactMetadata::default();

    // Find frontmatter between --- delimiters
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Ok(metadata);
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    let rest = after_first.trim_start_matches(['\r', '\n']);
    let closing = rest.find("\n---");
    let yaml_block = match closing {
        Some(pos) => &rest[..pos],
        None => {
            return Err(Error::FrontmatterParse {
                path: path.to_path_buf(),
                reason: "no closing --- delimiter found".to_string(),
            });
        },
    };

    // Parse line by line
    let mut hooks_lines: Vec<&str> = Vec::new();
    let mut in_hooks = false;

    for line in yaml_block.lines() {
        let trimmed_line = line.trim();

        // Check if we're in a hooks block (indented continuation)
        if in_hooks {
            if line.starts_with(' ') || line.starts_with('\t') {
                hooks_lines.push(line);
                continue;
            }
            in_hooks = false;
        }

        if let Some(value) = trimmed_line.strip_prefix("name:") {
            metadata.name = Some(value.trim().to_string());
        } else if let Some(value) = trimmed_line.strip_prefix("description:") {
            metadata.description = Some(value.trim().to_string());
        } else if trimmed_line.starts_with("hooks:") {
            in_hooks = true;
            let value = trimmed_line.strip_prefix("hooks:").unwrap_or_default().trim();
            if !value.is_empty() {
                hooks_lines.push(value);
            }
        } else if let Some(value) = trimmed_line.strip_prefix("disable-model-invocation:") {
            if value.trim() == "true" {
                metadata.model_invocation_disabled = true;
            }
        }
    }

    if !hooks_lines.is_empty() {
        metadata.hooks = Some(hooks_lines.join("\n"));
    }

    Ok(metadata)
}

/// Extract script references from SKILL.md content.
///
/// Looks for `${CLAUDE_SKILL_DIR}/scripts/<path>` and
/// `${CLAUDE_SKILL_DIR}/<path>` patterns.
pub fn extract_script_references(content: &str) -> Vec<PathBuf> {
    let mut scripts = Vec::new();
    let marker = "${CLAUDE_SKILL_DIR}/";

    for line in content.lines() {
        let mut search = line;
        while let Some(pos) = search.find(marker) {
            let after = &search[pos + marker.len()..];
            // Extract the path until whitespace, quote, backtick, or end of line
            let end = after
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '`' || c == ')')
                .unwrap_or(after.len());
            let path_str = &after[..end];
            if path_str.starts_with("scripts/") {
                scripts.push(PathBuf::from(path_str));
            }
            search = &search[pos + marker.len() + end..];
        }
    }

    scripts
}

/// Recursively collect all files in a directory, returning paths relative to `base`.
fn collect_files_recursive(dir: &Path, base: &Path, fs: &dyn Fs) -> Result<Vec<PathBuf>, Error> {
    let mut files = Vec::new();
    let entries = fs.read_dir(dir)?;

    for entry in entries {
        let full_path = dir.join(&entry.name);
        if entry.is_dir {
            let sub_files = collect_files_recursive(&full_path, base, fs)?;
            files.extend(sub_files);
        } else if let Ok(relative) = full_path.strip_prefix(base) {
            files.push(relative.to_path_buf());
        }
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};

    struct MockFs {
        exists: HashSet<PathBuf>,
        dirs: HashMap<PathBuf, Vec<crate::fs::DirEntry>>,
        files: HashMap<PathBuf, String>,
        written: RefCell<HashMap<PathBuf, Vec<u8>>>,
    }

    impl MockFs {
        fn new() -> Self {
            Self {
                exists: HashSet::new(),
                dirs: HashMap::new(),
                files: HashMap::new(),
                written: RefCell::new(HashMap::new()),
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
            self.written.borrow_mut().insert(path.to_path_buf(), content.to_vec());
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
}
