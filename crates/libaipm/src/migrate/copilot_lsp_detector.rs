//! Copilot LSP detector: reads `lsp.json` for LSP server configurations.
//!
//! Future-proofing — Copilot v1.0.12 has the schema but no runtime support.

use std::path::Path;

use crate::fs::Fs;

use super::detector::Detector;
use super::{Artifact, ArtifactKind, ArtifactMetadata, Error};

/// Reads `lsp.json` at the source directory or project root `.github/lsp.json`.
pub struct CopilotLspDetector;

impl Detector for CopilotLspDetector {
    fn name(&self) -> &'static str {
        "copilot-lsp"
    }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        // Check <source_dir>/lsp.json first
        let direct_path = source_dir.join("lsp.json");

        // Then check project root .github/lsp.json as fallback
        let fallback_path = source_dir.parent().map(|root| root.join(".github").join("lsp.json"));

        let lsp_path = if fs.exists(&direct_path) {
            direct_path
        } else if let Some(ref fb) = fallback_path {
            if fs.exists(fb) {
                fb.clone()
            } else {
                return Ok(Vec::new());
            }
        } else {
            return Ok(Vec::new());
        };

        let content = fs.read_to_string(&lsp_path)?;

        let json: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| Error::ConfigParse {
                path: lsp_path.clone(),
                reason: format!("invalid JSON in lsp.json: {e}"),
            })?;

        // Validate it contains LSP server definitions (non-empty object)
        let Some(obj) = json.as_object() else {
            return Ok(Vec::new());
        };

        if obj.is_empty() {
            return Ok(Vec::new());
        }

        let server_count = obj.len();
        let description = format!("{server_count} LSP server(s) from lsp.json");

        Ok(vec![Artifact {
            kind: ArtifactKind::LspServer,
            name: "copilot-lsp-servers".to_string(),
            source_path: lsp_path,
            files: Vec::new(),
            referenced_scripts: Vec::new(),
            metadata: ArtifactMetadata {
                name: Some("copilot-lsp-servers".to_string()),
                description: Some(description),
                raw_content: Some(content),
                ..ArtifactMetadata::default()
            },
        }])
    }
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

    #[test]
    fn detect_valid_lsp_json_at_source_dir() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/lsp.json"));
        fs.files.insert(
            PathBuf::from("/project/.github/lsp.json"),
            r#"{"typescript-lsp":{"command":"typescript-language-server","args":["--stdio"]}}"#
                .to_string(),
        );

        let detector = CopilotLspDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("copilot-lsp-servers"));
        assert_eq!(artifacts.first().map(|a| &a.kind), Some(&ArtifactKind::LspServer));
        assert!(artifacts
            .first()
            .and_then(|a| a.metadata.description.as_deref())
            .is_some_and(|d| d.contains("1 LSP server(s)")));
    }

    #[test]
    fn detect_github_lsp_json_fallback() {
        let mut fs = MockFs::new();
        // Not at source_dir directly, but at project_root/.github/lsp.json
        fs.exists.insert(PathBuf::from("/project/.github/lsp.json"));
        fs.files.insert(
            PathBuf::from("/project/.github/lsp.json"),
            r#"{"rust-analyzer":{"command":"rust-analyzer"}}"#.to_string(),
        );

        // source_dir is /project/.copilot (not .github), so fallback checks .github/lsp.json
        let detector = CopilotLspDetector;
        let result = detector.detect(Path::new("/project/.copilot"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
    }

    #[test]
    fn no_lsp_json_returns_empty() {
        let fs = MockFs::new();
        let detector = CopilotLspDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn malformed_json_returns_config_parse_error() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/lsp.json"));
        fs.files.insert(PathBuf::from("/project/.github/lsp.json"), "not valid json".to_string());

        let detector = CopilotLspDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_err());
        assert!(result.err().is_some_and(|e| matches!(e, Error::ConfigParse { .. })));
    }

    #[test]
    fn empty_object_returns_empty() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/lsp.json"));
        fs.files.insert(PathBuf::from("/project/.github/lsp.json"), r#"{}"#.to_string());

        let detector = CopilotLspDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn root_source_dir_no_parent_returns_empty() {
        let fs = MockFs::new();
        let detector = CopilotLspDetector;
        let result = detector.detect(Path::new("/"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn non_object_json_returns_empty() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/lsp.json"));
        fs.files.insert(PathBuf::from("/project/.github/lsp.json"), r#"[1,2,3]"#.to_string());

        let detector = CopilotLspDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn multiple_lsp_servers_counted() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/lsp.json"));
        fs.files.insert(
            PathBuf::from("/project/.github/lsp.json"),
            r#"{"ts":{"command":"tsc"},"rust":{"command":"rust-analyzer"},"python":{"command":"pylsp"}}"#.to_string(),
        );

        let detector = CopilotLspDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert!(artifacts
            .first()
            .and_then(|a| a.metadata.description.as_deref())
            .is_some_and(|d| d.contains("3 LSP server(s)")));
    }
}
