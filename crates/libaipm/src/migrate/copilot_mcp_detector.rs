//! Copilot MCP detector: reads `.copilot/mcp-config.json` with transport passthrough.
//!
//! Only checks the Copilot-specific path (`.copilot/mcp-config.json`).
//! The shared `.mcp.json` is handled by the existing `McpDetector`.

use std::path::Path;

use crate::fs::Fs;

use super::detector::Detector;
use super::{Artifact, ArtifactKind, ArtifactMetadata, Error};

/// Reads `.copilot/mcp-config.json` and emits MCP server artifacts.
pub struct CopilotMcpDetector;

impl Detector for CopilotMcpDetector {
    fn name(&self) -> &'static str {
        "copilot-mcp"
    }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        // source_dir is .github/ — derive project root by going up one level
        let Some(project_root) = source_dir.parent() else {
            return Ok(Vec::new());
        };

        let mcp_path = project_root.join(".copilot").join("mcp-config.json");
        if !fs.exists(&mcp_path) {
            return Ok(Vec::new());
        }

        let content = fs.read_to_string(&mcp_path)?;

        let json: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| Error::ConfigParse {
                path: mcp_path.clone(),
                reason: format!("invalid JSON in .copilot/mcp-config.json: {e}"),
            })?;

        let servers = json.get("mcpServers").and_then(|v| v.as_object());
        if servers.is_none_or(serde_json::Map::is_empty) {
            return Ok(Vec::new());
        }

        let server_count = servers.map_or(0, serde_json::Map::len);
        let description = format!("{server_count} MCP server(s) from .copilot/mcp-config.json");

        Ok(vec![Artifact {
            kind: ArtifactKind::McpServer,
            name: "copilot-mcp-servers".to_string(),
            source_path: mcp_path,
            files: Vec::new(),
            referenced_scripts: Vec::new(),
            metadata: ArtifactMetadata {
                name: Some("copilot-mcp-servers".to_string()),
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
    fn detect_valid_copilot_mcp_config_local_transport() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.copilot/mcp-config.json"));
        fs.files.insert(
            PathBuf::from("/project/.copilot/mcp-config.json"),
            r#"{"mcpServers":{"my-server":{"transport":"local","command":"node","args":["server.js"]}}}"#.to_string(),
        );

        let detector = CopilotMcpDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("copilot-mcp-servers"));
        assert_eq!(artifacts.first().map(|a| &a.kind), Some(&ArtifactKind::McpServer));
        assert!(artifacts
            .first()
            .and_then(|a| a.metadata.description.as_deref())
            .is_some_and(|d| d.contains("1 MCP server(s)")));
    }

    #[test]
    fn detect_valid_copilot_mcp_config_stdio_transport() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.copilot/mcp-config.json"));
        fs.files.insert(
            PathBuf::from("/project/.copilot/mcp-config.json"),
            r#"{"mcpServers":{"s1":{"transport":"stdio","command":"test"},"s2":{"transport":"stdio","command":"test2"}}}"#.to_string(),
        );

        let detector = CopilotMcpDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert!(artifacts
            .first()
            .and_then(|a| a.metadata.description.as_deref())
            .is_some_and(|d| d.contains("2 MCP server(s)")));
    }

    #[test]
    fn no_copilot_mcp_config_returns_empty() {
        let fs = MockFs::new();
        let detector = CopilotMcpDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn empty_mcp_servers_returns_empty() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.copilot/mcp-config.json"));
        fs.files.insert(
            PathBuf::from("/project/.copilot/mcp-config.json"),
            r#"{"mcpServers":{}}"#.to_string(),
        );

        let detector = CopilotMcpDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn malformed_json_returns_error() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.copilot/mcp-config.json"));
        fs.files.insert(
            PathBuf::from("/project/.copilot/mcp-config.json"),
            "not valid json".to_string(),
        );

        let detector = CopilotMcpDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_err());
        assert!(result.err().is_some_and(|e| matches!(e, Error::ConfigParse { .. })));
    }

    #[test]
    fn no_mcp_servers_key_returns_empty() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.copilot/mcp-config.json"));
        fs.files.insert(
            PathBuf::from("/project/.copilot/mcp-config.json"),
            r#"{"otherKey":"value"}"#.to_string(),
        );

        let detector = CopilotMcpDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn project_root_derivation() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/a/b/.copilot/mcp-config.json"));
        fs.files.insert(
            PathBuf::from("/a/b/.copilot/mcp-config.json"),
            r#"{"mcpServers":{"s1":{"command":"test"}}}"#.to_string(),
        );

        let detector = CopilotMcpDetector;
        let result = detector.detect(Path::new("/a/b/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(
            artifacts.first().map(|a| a.source_path.clone()),
            Some(PathBuf::from("/a/b/.copilot/mcp-config.json"))
        );
    }
}
