//! Agent detector: scans `.claude/agents/` for `.md` agent definitions.

use std::path::{Path, PathBuf};

use crate::fs::Fs;

use super::detector::Detector;
use super::{strip_yaml_quotes, Artifact, ArtifactKind, ArtifactMetadata, Error};

/// Scans `.claude/agents/` for `.md` files (subagent definitions).
pub struct AgentDetector;

impl Detector for AgentDetector {
    fn name(&self) -> &'static str {
        "agent"
    }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        let agents_dir = source_dir.join("agents");
        if !fs.exists(&agents_dir) {
            return Ok(Vec::new());
        }

        let entries = fs.read_dir(&agents_dir)?;
        let mut artifacts = Vec::new();

        for entry in entries {
            if entry.is_dir {
                continue;
            }
            if !Path::new(&entry.name).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            {
                continue;
            }

            let agent_path = agents_dir.join(&entry.name);
            let content = fs.read_to_string(&agent_path)?;
            let metadata = parse_agent_frontmatter(&content, &agent_path)?;

            let name = metadata.name.clone().unwrap_or_else(|| {
                Path::new(&entry.name)
                    .file_stem()
                    .map_or_else(|| entry.name.clone(), |s| s.to_string_lossy().into_owned())
            });

            artifacts.push(Artifact {
                kind: ArtifactKind::Agent,
                name,
                source_path: agent_path,
                files: vec![PathBuf::from(&entry.name)],
                referenced_scripts: Vec::new(),
                metadata,
            });
        }

        Ok(artifacts)
    }
}

/// Parse YAML frontmatter from an agent `.md` file.
///
/// Extracts `name` and `description` fields only. All other agent-specific
/// fields (tools, model, etc.) are preserved in the raw `.md` content.
fn parse_agent_frontmatter(content: &str, path: &Path) -> Result<ArtifactMetadata, Error> {
    let mut metadata = ArtifactMetadata::default();

    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Ok(metadata);
    }

    let after_first = &trimmed[3..];
    let rest = after_first.trim_start_matches(['\r', '\n']);
    let closing = rest.find("\n---");
    let yaml_block = match closing {
        Some(pos) => &rest[..pos],
        None => {
            return Err(Error::FrontmatterParse {
                path: path.to_path_buf(),
                reason: "missing closing --- delimiter".to_string(),
            });
        },
    };

    for line in yaml_block.lines() {
        let trimmed_line = line.trim();
        if let Some(value) = trimmed_line.strip_prefix("name:") {
            metadata.name = Some(strip_yaml_quotes(value.trim()).to_string());
        } else if let Some(value) = trimmed_line.strip_prefix("description:") {
            metadata.description = Some(strip_yaml_quotes(value.trim()).to_string());
        }
    }

    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
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
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("reviewer.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/agents/reviewer.md"),
            "---\nname: security-reviewer\ndescription: Reviews code for security\n---\nYou are a security reviewer."
                .to_string(),
        );

        let detector = AgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| &a.kind), Some(&ArtifactKind::Agent));
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("security-reviewer"));
        assert_eq!(
            artifacts.first().and_then(|a| a.metadata.description.as_deref()),
            Some("Reviews code for security")
        );
    }

    #[test]
    fn detect_agent_no_agents_dir() {
        let fs = MockFs::new();
        let detector = AgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_agent_empty_dir() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(PathBuf::from("/src/agents"), Vec::new());

        let detector = AgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_agent_skips_non_md_files() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("config.yaml", false)]);

        let detector = AgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_agent_skips_directories() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("subdir", true)]);

        let detector = AgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_agent_malformed_frontmatter() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("bad.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/agents/bad.md"),
            "---\nname: bad\nno closing delimiter".to_string(),
        );

        let detector = AgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_err());
    }

    #[test]
    fn detect_agent_no_frontmatter_uses_filename() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("reviewer.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/agents/reviewer.md"),
            "You are a code reviewer.".to_string(),
        );

        let detector = AgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("reviewer"));
    }

    #[test]
    fn detect_multiple_agents() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(
            PathBuf::from("/src/agents"),
            vec![de("reviewer.md", false), de("writer.md", false)],
        );
        fs.files.insert(
            PathBuf::from("/src/agents/reviewer.md"),
            "---\nname: reviewer\n---\nReview.".to_string(),
        );
        fs.files.insert(
            PathBuf::from("/src/agents/writer.md"),
            "---\nname: writer\n---\nWrite.".to_string(),
        );

        let detector = AgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 2);
    }

    #[test]
    fn frontmatter_name_fallback_to_filename_stem() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("my-agent.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/agents/my-agent.md"),
            "---\ndescription: No name field\n---\nBody.".to_string(),
        );

        let detector = AgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("my-agent"));
    }

    #[test]
    fn detect_agent_strips_quoted_description() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/agents"));
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("reviewer.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/agents/reviewer.md"),
            "---\nname: \"reviewer\"\ndescription: \"Reviews code\"\n---\nBody.".to_string(),
        );

        let detector = AgentDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("reviewer"));
        assert_eq!(
            artifacts.first().and_then(|a| a.metadata.description.as_deref()),
            Some("Reviews code")
        );
    }
}
