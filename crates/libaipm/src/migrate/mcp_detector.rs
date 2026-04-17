//! MCP detector: reads `.mcp.json` at the project root and emits all MCP servers as a single
//! artifact.

use std::path::{Path, PathBuf};

use crate::fs::Fs;

use super::detector::Detector;
use super::{Artifact, ArtifactKind, ArtifactMetadata, Error};

/// Reads `.mcp.json` at the project root and emits all MCP servers as a single artifact.
pub struct McpDetector;

impl Detector for McpDetector {
    fn name(&self) -> &'static str {
        "mcp"
    }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        // source_dir is .claude/ — derive project root by going up one level
        let Some(project_root) = source_dir.parent() else {
            return Ok(Vec::new());
        };

        let mcp_path = project_root.join(".mcp.json");
        if !fs.exists(&mcp_path) {
            return Ok(Vec::new());
        }

        let content = fs.read_to_string(&mcp_path)?;

        // Validate it's parseable JSON with mcpServers key
        let json: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| Error::ConfigParse {
                path: mcp_path.clone(),
                reason: format!("invalid JSON in .mcp.json: {e}"),
            })?;

        let servers = json.get("mcpServers").and_then(|v| v.as_object());
        if servers.is_none_or(serde_json::Map::is_empty) {
            return Ok(Vec::new());
        }

        let server_count = servers.map_or(0, serde_json::Map::len);
        let description = format!("{server_count} MCP server(s) from .mcp.json");

        Ok(vec![Artifact {
            kind: ArtifactKind::McpServer,
            name: "project-mcp-servers".to_string(),
            source_path: mcp_path,
            files: vec![PathBuf::from(".mcp.json")],
            referenced_scripts: Vec::new(),
            metadata: ArtifactMetadata {
                name: Some("project-mcp-servers".to_string()),
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
    fn detect_valid_mcp_json() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.mcp.json"));
        fs.files.insert(
            PathBuf::from("/project/.mcp.json"),
            r#"{"mcpServers":{"slack":{"command":"npx","args":["slack-mcp"]},"github":{"command":"npx","args":["github-mcp"]}}}"#.to_string(),
        );

        let detector = McpDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| &a.kind), Some(&ArtifactKind::McpServer));
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("project-mcp-servers"));
        assert!(artifacts
            .first()
            .and_then(|a| a.metadata.description.as_deref())
            .is_some_and(|d| d.contains("2 MCP server(s)")));
        assert!(artifacts.first().and_then(|a| a.metadata.raw_content.as_ref()).is_some());
    }

    #[test]
    fn detect_no_mcp_json() {
        let fs = MockFs::new();
        let detector = McpDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_empty_mcp_servers() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.mcp.json"));
        fs.files.insert(PathBuf::from("/project/.mcp.json"), r#"{"mcpServers":{}}"#.to_string());

        let detector = McpDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_malformed_json() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.mcp.json"));
        fs.files.insert(PathBuf::from("/project/.mcp.json"), "not valid json".to_string());

        let detector = McpDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_err());
        assert!(result.err().is_some_and(|e| matches!(e, Error::ConfigParse { .. })));
    }

    #[test]
    fn detect_no_mcp_servers_key() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.mcp.json"));
        fs.files.insert(PathBuf::from("/project/.mcp.json"), r#"{"otherKey":"value"}"#.to_string());

        let detector = McpDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_project_root_derivation() {
        // source_dir is /a/b/.claude, project root should be /a/b
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/a/b/.mcp.json"));
        fs.files.insert(
            PathBuf::from("/a/b/.mcp.json"),
            r#"{"mcpServers":{"s1":{"command":"test"}}}"#.to_string(),
        );

        let detector = McpDetector;
        let result = detector.detect(Path::new("/a/b/.claude"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(
            artifacts.first().map(|a| a.source_path.clone()),
            Some(PathBuf::from("/a/b/.mcp.json"))
        );
    }

    #[test]
    fn detect_root_source_dir_no_parent() {
        let fs = MockFs::new();
        let detector = McpDetector;
        // On Unix, "/" has no parent; on Windows this edge case is harder to trigger
        // but Path::new("/").parent() returns Some("") on Unix.
        // This test just ensures no panic.
        let result = detector.detect(Path::new("/"), &fs);
        assert!(result.is_ok());
    }

    #[test]
    fn detect_io_error_on_read() {
        // .mcp.json exists according to fs.exists but has no content in fs.files,
        // so read_to_string returns an IO error — covers the `?` error branch on
        // the read call inside `detect`.
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.mcp.json"));
        // Intentionally NOT adding content to fs.files

        let detector = McpDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_err());
        assert!(result.err().is_some_and(|e| matches!(e, Error::Io(_))));
    }
}
