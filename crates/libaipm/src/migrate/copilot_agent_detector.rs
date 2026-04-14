//! Copilot agent detector: scans `.github/agents/` for `.md` and `.agent.md` files
//! with dedup precedence for `.agent.md`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::fs::Fs;

use super::detector::Detector;
use super::skill_common::extract_script_references;
use super::{Artifact, ArtifactKind, ArtifactMetadata, Error};

/// Scans `.github/agents/` for `.md` and `.agent.md` files.
/// When both `foo.md` and `foo.agent.md` exist, `.agent.md` takes precedence.
pub struct CopilotAgentDetector;

impl Detector for CopilotAgentDetector {
    fn name(&self) -> &'static str {
        "copilot-agent"
    }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        let agents_dir = source_dir.join("agents");
        if !fs.exists(&agents_dir) {
            return Ok(Vec::new());
        }

        let entries = fs.read_dir(&agents_dir)?;

        // Dedup: .agent.md takes precedence over .md
        let mut by_name: HashMap<String, (PathBuf, String)> = HashMap::new();

        for entry in entries {
            if entry.is_dir {
                continue;
            }
            if !Path::new(&entry.name).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            {
                continue;
            }

            let is_agent_md = has_agent_md_suffix(&entry.name);
            let stem = if is_agent_md {
                entry.name[..entry.name.len() - ".agent.md".len()].to_string()
            } else {
                Path::new(&entry.name)
                    .file_stem()
                    .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
            };
            if stem.is_empty() {
                continue;
            }

            // .agent.md takes precedence
            if is_agent_md || !by_name.contains_key(&stem) {
                by_name.insert(stem, (agents_dir.join(&entry.name), entry.name.clone()));
            }
        }

        let mut artifacts = Vec::new();
        let mut sorted_names: Vec<String> = by_name.keys().cloned().collect();
        sorted_names.sort();

        for name_key in sorted_names {
            let Some((agent_path, _filename)) = by_name.get(&name_key) else {
                continue;
            };

            let content = fs.read_to_string(agent_path)?;
            let metadata = super::skill_common::parse_frontmatter(&content, agent_path)?;

            let name = metadata.name.clone().unwrap_or_else(|| name_key.clone());

            artifacts.push(Artifact {
                kind: ArtifactKind::Agent,
                name,
                source_path: agent_path.clone(),
                files: Vec::new(),
                referenced_scripts: extract_script_references(&content, "${COPILOT_AGENT_DIR}/"),
                metadata: ArtifactMetadata {
                    name: metadata.name,
                    description: metadata.description,
                    raw_content: Some(content),
                    ..ArtifactMetadata::default()
                },
            });
        }

        Ok(artifacts)
    }
}

/// Check if a filename ends with `.agent.md` (case-insensitive).
fn has_agent_md_suffix(name: &str) -> bool {
    name.len() > ".agent.md".len()
        && name[name.len() - ".agent.md".len()..].eq_ignore_ascii_case(".agent.md")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
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
    fn detect_agent_md_file() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("reviewer.agent.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/agents/reviewer.agent.md"),
            "---\nname: security-reviewer\ndescription: Reviews security\n---\nYou review code."
                .to_string(),
        );

        let detector = CopilotAgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("security-reviewer"));
        assert_eq!(artifacts.first().map(|a| &a.kind), Some(&ArtifactKind::Agent));
    }

    #[test]
    fn detect_plain_md_when_no_agent_md() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("writer.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/agents/writer.md"),
            "---\nname: writer\n---\nYou write.".to_string(),
        );

        let detector = CopilotAgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("writer"));
    }

    #[test]
    fn agent_md_takes_precedence_over_plain_md() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(
            PathBuf::from("/src/agents"),
            vec![de("foo.md", false), de("foo.agent.md", false)],
        );
        fs.files.insert(
            PathBuf::from("/src/agents/foo.md"),
            "---\nname: plain-foo\n---\nPlain.".to_string(),
        );
        fs.files.insert(
            PathBuf::from("/src/agents/foo.agent.md"),
            "---\nname: agent-foo\n---\nAgent.".to_string(),
        );

        let detector = CopilotAgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("agent-foo"));
    }

    #[test]
    fn no_agents_dir_returns_empty() {
        let fs = MockFs::new();
        let detector = CopilotAgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn empty_agents_dir_returns_empty() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(PathBuf::from("/src/agents"), Vec::new());

        let detector = CopilotAgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn skips_directories_and_non_md_files() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(
            PathBuf::from("/src/agents"),
            vec![de("subdir", true), de("config.yaml", false)],
        );

        let detector = CopilotAgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn malformed_frontmatter_returns_error() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("bad.agent.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/agents/bad.agent.md"),
            "---\nname: bad\nno closing delimiter".to_string(),
        );

        let detector = CopilotAgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_err());
    }

    #[test]
    fn no_frontmatter_uses_filename_stem() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("helper.md", false)]);
        fs.files.insert(PathBuf::from("/src/agents/helper.md"), "You are a helper.".to_string());

        let detector = CopilotAgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("helper"));
    }

    #[test]
    fn multiple_agents_detected() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs
            .insert(PathBuf::from("/src/agents"), vec![de("a.md", false), de("b.agent.md", false)]);
        fs.files
            .insert(PathBuf::from("/src/agents/a.md"), "---\nname: agent-a\n---\nA.".to_string());
        fs.files.insert(
            PathBuf::from("/src/agents/b.agent.md"),
            "---\nname: agent-b\n---\nB.".to_string(),
        );

        let detector = CopilotAgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 2);
    }

    #[test]
    fn quoted_frontmatter_values_stripped() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("r.agent.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/agents/r.agent.md"),
            "---\nname: \"reviewer\"\ndescription: 'Reviews code'\n---\nBody.".to_string(),
        );

        let detector = CopilotAgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("reviewer"));
        assert_eq!(
            artifacts.first().and_then(|a| a.metadata.description.as_deref()),
            Some("Reviews code")
        );
    }

    #[test]
    fn has_agent_md_suffix_edge_cases() {
        assert!(has_agent_md_suffix("foo.agent.md"));
        assert!(!has_agent_md_suffix(".agent.md")); // name would be empty, too short
        assert!(!has_agent_md_suffix("foo.md"));
        assert!(!has_agent_md_suffix(""));
    }

    #[test]
    fn detect_agent_stores_raw_content() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("test.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/agents/test.md"),
            "---\nname: test\ntools:\n  - Read\n---\nBody content".to_string(),
        );

        let detector = CopilotAgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert!(artifacts.first().and_then(|a| a.metadata.raw_content.as_ref()).is_some());
    }

    #[test]
    fn agent_md_first_in_listing_skips_plain_md() {
        // When `foo.agent.md` appears before `foo.md` in the directory listing,
        // `by_name` already contains the stem when `foo.md` is processed.
        // The condition `is_agent_md || !by_name.contains_key(&stem)` evaluates to
        // `false || !true` = false, so the plain `.md` entry is skipped.
        // This covers the False branch at the `!by_name.contains_key` sub-expression.
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(
            PathBuf::from("/src/agents"),
            vec![de("foo.agent.md", false), de("foo.md", false)],
        );
        fs.files.insert(
            PathBuf::from("/src/agents/foo.agent.md"),
            "---\nname: agent-foo\n---\nAgent.".to_string(),
        );
        fs.files.insert(
            PathBuf::from("/src/agents/foo.md"),
            "---\nname: plain-foo\n---\nPlain.".to_string(),
        );

        let detector = CopilotAgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        // Only one artifact — the .agent.md takes precedence
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("agent-foo"));
    }
}
