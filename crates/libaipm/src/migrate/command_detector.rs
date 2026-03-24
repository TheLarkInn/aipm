//! Command detector: scans `.claude/commands/` for legacy `.md` command files.

use std::path::{Path, PathBuf};

use crate::fs::Fs;

use super::detector::Detector;
use super::skill_detector::extract_script_references;
use super::{Artifact, ArtifactKind, ArtifactMetadata, Error};

/// Scans `.claude/commands/` for `.md` files (legacy command format).
pub struct CommandDetector;

impl Detector for CommandDetector {
    fn name(&self) -> &'static str {
        "command"
    }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        let commands_dir = source_dir.join("commands");
        if !fs.exists(&commands_dir) {
            return Ok(Vec::new());
        }

        let entries = fs.read_dir(&commands_dir)?;
        let mut artifacts = Vec::new();

        for entry in entries {
            if entry.is_dir {
                continue;
            }
            if !std::path::Path::new(&entry.name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            {
                continue;
            }

            let cmd_path = commands_dir.join(&entry.name);
            let content = fs.read_to_string(&cmd_path)?;

            // Use file_stem to derive name (consistent with case-insensitive extension check)
            let name = std::path::Path::new(&entry.name)
                .file_stem()
                .map_or_else(|| entry.name.clone(), |s| s.to_string_lossy().into_owned());

            let mut metadata = parse_command_frontmatter(&content);
            metadata.model_invocation_disabled = true;

            let referenced_scripts = extract_script_references(&content);

            artifacts.push(Artifact {
                kind: ArtifactKind::Command,
                name,
                source_path: cmd_path,
                files: vec![PathBuf::from(&entry.name)],
                referenced_scripts,
                metadata,
            });
        }

        Ok(artifacts)
    }
}

/// Parse optional frontmatter from a command `.md` file.
fn parse_command_frontmatter(content: &str) -> ArtifactMetadata {
    let mut metadata = ArtifactMetadata::default();

    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return metadata;
    }

    let after_first = &trimmed[3..];
    let rest = after_first.trim_start_matches(['\r', '\n']);
    let closing = rest.find("\n---");
    let yaml_block = match closing {
        Some(pos) => &rest[..pos],
        None => return metadata,
    };

    for line in yaml_block.lines() {
        let trimmed_line = line.trim();
        if let Some(value) = trimmed_line.strip_prefix("name:") {
            metadata.name = Some(value.trim().to_string());
        } else if let Some(value) = trimmed_line.strip_prefix("description:") {
            metadata.description = Some(value.trim().to_string());
        }
    }

    metadata
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
            if let Ok(mut w) = self.written.lock() {
                w.insert(path.to_path_buf(), content.to_vec());
            }
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
    fn detect_command_md_file() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/commands"));
        fs.dirs.insert(PathBuf::from("/src/commands"), vec![de("review.md", false)]);
        fs.files.insert(PathBuf::from("/src/commands/review.md"), "Review the code".to_string());

        let detector = CommandDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| &a.kind), Some(&ArtifactKind::Command));
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("review"));
    }

    #[test]
    fn detect_command_adds_disable_model_invocation() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/commands"));
        fs.dirs.insert(PathBuf::from("/src/commands"), vec![de("review.md", false)]);
        fs.files.insert(PathBuf::from("/src/commands/review.md"), "Review the code".to_string());

        let detector = CommandDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        let artifacts = result.ok().unwrap_or_default();
        assert!(artifacts.first().is_some_and(|a| a.metadata.model_invocation_disabled));
    }

    #[test]
    fn detect_command_with_frontmatter() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/commands"));
        fs.dirs.insert(PathBuf::from("/src/commands"), vec![de("review.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/commands/review.md"),
            "---\nname: review\ndescription: Code review\n---\nReview body".to_string(),
        );

        let detector = CommandDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(
            artifacts.first().and_then(|a| a.metadata.description.as_deref()),
            Some("Code review")
        );
    }

    #[test]
    fn detect_command_without_frontmatter() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/commands"));
        fs.dirs.insert(PathBuf::from("/src/commands"), vec![de("review.md", false)]);
        fs.files
            .insert(PathBuf::from("/src/commands/review.md"), "Just plain markdown".to_string());

        let detector = CommandDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert!(artifacts.first().is_some_and(|a| a.metadata.model_invocation_disabled));
        assert!(artifacts.first().is_some_and(|a| a.metadata.name.is_none()));
    }

    #[test]
    fn detect_command_ignores_non_md() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/commands"));
        fs.dirs.insert(PathBuf::from("/src/commands"), vec![de("readme.txt", false)]);

        let detector = CommandDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_command_ignores_directories() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/commands"));
        fs.dirs.insert(PathBuf::from("/src/commands"), vec![de("subdir", true)]);

        let detector = CommandDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_no_commands_dir() {
        let fs = MockFs::new();
        let detector = CommandDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }
}
