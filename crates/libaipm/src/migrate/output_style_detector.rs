//! Output style detector: scans `.claude/output-styles/` for `.md` files.

use std::path::{Path, PathBuf};

use crate::fs::Fs;

use super::detector::Detector;
use super::{Artifact, ArtifactKind, ArtifactMetadata, Error};

/// Scans `.claude/output-styles/` for `.md` files.
pub struct OutputStyleDetector;

impl Detector for OutputStyleDetector {
    fn name(&self) -> &'static str {
        "output-style"
    }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        let styles_dir = source_dir.join("output-styles");
        if !fs.exists(&styles_dir) {
            return Ok(Vec::new());
        }

        let entries = fs.read_dir(&styles_dir)?;
        let mut artifacts = Vec::new();

        for entry in entries {
            if entry.is_dir {
                continue;
            }
            if !Path::new(&entry.name).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            {
                continue;
            }

            let style_path = styles_dir.join(&entry.name);
            let content = fs.read_to_string(&style_path)?;
            let metadata = parse_output_style_frontmatter(&content);

            let name = metadata.name.clone().unwrap_or_else(|| {
                Path::new(&entry.name)
                    .file_stem()
                    .map_or_else(|| entry.name.clone(), |s| s.to_string_lossy().into_owned())
            });

            artifacts.push(Artifact {
                kind: ArtifactKind::OutputStyle,
                name,
                source_path: style_path,
                files: vec![PathBuf::from(&entry.name)],
                referenced_scripts: Vec::new(),
                metadata,
            });
        }

        Ok(artifacts)
    }
}

/// Parse optional YAML frontmatter from an output style `.md` file.
///
/// Extracts `name` and `description` fields only. The `keep-coding-instructions`
/// field and the entire `.md` body are preserved verbatim when copied.
fn parse_output_style_frontmatter(content: &str) -> ArtifactMetadata {
    let mut metadata = ArtifactMetadata::default();

    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return metadata;
    }

    let after_first = &trimmed[3..];
    let rest = after_first.trim_start_matches(['\r', '\n']);
    let yaml_block = match rest.find("\n---") {
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
    fn detect_output_style_md_file() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/output-styles"));
        fs.dirs.insert(PathBuf::from("/src/output-styles"), vec![de("concise.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/output-styles/concise.md"),
            "---\nname: concise\ndescription: Short outputs\n---\nBe concise.".to_string(),
        );

        let detector = OutputStyleDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| &a.kind), Some(&ArtifactKind::OutputStyle));
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("concise"));
        assert_eq!(
            artifacts.first().and_then(|a| a.metadata.description.as_deref()),
            Some("Short outputs")
        );
    }

    #[test]
    fn detect_no_output_styles_dir() {
        let fs = MockFs::new();
        let detector = OutputStyleDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_empty_output_styles_dir() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/output-styles"));
        fs.dirs.insert(PathBuf::from("/src/output-styles"), Vec::new());

        let detector = OutputStyleDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_multiple_output_styles() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/output-styles"));
        fs.dirs.insert(
            PathBuf::from("/src/output-styles"),
            vec![de("concise.md", false), de("verbose.md", false)],
        );
        fs.files.insert(
            PathBuf::from("/src/output-styles/concise.md"),
            "---\nname: concise\n---\nBe concise.".to_string(),
        );
        fs.files.insert(
            PathBuf::from("/src/output-styles/verbose.md"),
            "---\nname: verbose\n---\nBe verbose.".to_string(),
        );

        let detector = OutputStyleDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 2);
    }

    #[test]
    fn detect_name_fallback_to_filename() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/output-styles"));
        fs.dirs.insert(PathBuf::from("/src/output-styles"), vec![de("my-style.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/output-styles/my-style.md"),
            "No frontmatter at all.".to_string(),
        );

        let detector = OutputStyleDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("my-style"));
    }

    #[test]
    fn detect_skips_non_md_files() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/output-styles"));
        fs.dirs.insert(PathBuf::from("/src/output-styles"), vec![de("config.json", false)]);

        let detector = OutputStyleDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_skips_directories() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/output-styles"));
        fs.dirs.insert(PathBuf::from("/src/output-styles"), vec![de("subdir", true)]);

        let detector = OutputStyleDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn frontmatter_with_no_name_uses_description_only() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/output-styles"));
        fs.dirs.insert(PathBuf::from("/src/output-styles"), vec![de("fancy.md", false)]);
        fs.files.insert(
            PathBuf::from("/src/output-styles/fancy.md"),
            "---\ndescription: A fancy style\n---\nFancy content.".to_string(),
        );

        let detector = OutputStyleDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("fancy"));
        assert_eq!(
            artifacts.first().and_then(|a| a.metadata.description.as_deref()),
            Some("A fancy style")
        );
    }
}
