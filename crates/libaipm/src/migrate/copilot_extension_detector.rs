//! Copilot extension detector: scans `.github/extensions/` subdirectories.

use std::path::Path;

use crate::fs::Fs;

use super::detector::Detector;
use super::skill_common;
use super::{Artifact, ArtifactKind, ArtifactMetadata, Error};

/// Scans `.github/extensions/` for subdirectories, treating each as an extension.
pub struct CopilotExtensionDetector;

impl Detector for CopilotExtensionDetector {
    fn name(&self) -> &'static str {
        "copilot-extension"
    }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        let extensions_dir = source_dir.join("extensions");
        if !fs.exists(&extensions_dir) {
            return Ok(Vec::new());
        }

        let entries = fs.read_dir(&extensions_dir)?;
        let mut artifacts = Vec::new();

        for entry in entries {
            if !entry.is_dir {
                continue;
            }

            let ext_dir = extensions_dir.join(&entry.name);
            let files = skill_common::collect_files_recursive(&ext_dir, &ext_dir, fs)?;

            // Try to read any config file for raw_content
            let raw_content = try_read_config(&ext_dir, fs);

            artifacts.push(Artifact {
                kind: ArtifactKind::Extension,
                name: entry.name.clone(),
                source_path: ext_dir,
                files,
                referenced_scripts: Vec::new(),
                metadata: ArtifactMetadata {
                    name: Some(entry.name.clone()),
                    description: Some(format!("Extension: {}", entry.name)),
                    raw_content,
                    ..ArtifactMetadata::default()
                },
            });
        }

        Ok(artifacts)
    }
}

/// Try to read a config file from an extension directory.
/// Checks for config.json, extension.json, or manifest.json.
fn try_read_config(ext_dir: &Path, fs: &dyn Fs) -> Option<String> {
    let candidates = [
        "config.json",
        "extension.json",
        "manifest.json",
        "config.yaml",
        "config.yml",
        "extension.yaml",
        "extension.yml",
        "manifest.yaml",
        "manifest.yml",
    ];
    for name in &candidates {
        let path = ext_dir.join(name);
        if let Ok(content) = fs.read_to_string(&path) {
            tracing::debug!(path = %path.display(), "found config file for copilot extension");
            return Some(content);
        }
    }
    tracing::debug!(dir = %ext_dir.display(), "no config file found among candidates");
    None
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn detect_extension_subdirectory_with_config() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/extensions"));
        fs.dirs.insert(PathBuf::from("/src/extensions"), vec![de("my-ext", true)]);
        fs.dirs.insert(
            PathBuf::from("/src/extensions/my-ext"),
            vec![de("config.json", false), de("index.js", false)],
        );
        fs.files.insert(
            PathBuf::from("/src/extensions/my-ext/config.json"),
            r#"{"name":"my-ext"}"#.to_string(),
        );

        let detector = CopilotExtensionDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("my-ext"));
        assert_eq!(artifacts.first().map(|a| &a.kind), Some(&ArtifactKind::Extension));
        assert!(artifacts.first().and_then(|a| a.metadata.raw_content.as_ref()).is_some());
    }

    #[test]
    fn detect_multiple_extensions() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/extensions"));
        fs.dirs
            .insert(PathBuf::from("/src/extensions"), vec![de("ext-a", true), de("ext-b", true)]);
        fs.dirs.insert(PathBuf::from("/src/extensions/ext-a"), vec![de("index.js", false)]);
        fs.dirs.insert(PathBuf::from("/src/extensions/ext-b"), vec![de("main.py", false)]);

        let detector = CopilotExtensionDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 2);
    }

    #[test]
    fn empty_extensions_dir_returns_empty() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/extensions"));
        fs.dirs.insert(PathBuf::from("/src/extensions"), Vec::new());

        let detector = CopilotExtensionDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn no_extensions_dir_returns_empty() {
        let fs = MockFs::new();
        let detector = CopilotExtensionDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn skips_non_directory_entries() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/src/extensions"));
        fs.dirs.insert(PathBuf::from("/src/extensions"), vec![de("readme.txt", false)]);

        let detector = CopilotExtensionDetector;
        let result = detector.detect(Path::new("/src"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }
}
