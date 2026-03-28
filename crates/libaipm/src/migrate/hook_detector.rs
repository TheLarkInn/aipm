//! Hook detector: extracts hooks from `.claude/settings.json` into a standalone hook artifact.

use std::path::{Path, PathBuf};

use crate::fs::Fs;

use super::detector::Detector;
use super::{Artifact, ArtifactKind, ArtifactMetadata, Error};

/// Extracts hooks from `.claude/settings.json` into a standalone hook artifact.
pub struct HookDetector;

impl Detector for HookDetector {
    fn name(&self) -> &'static str {
        "hook"
    }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        let settings_path = source_dir.join("settings.json");
        if !fs.exists(&settings_path) {
            return Ok(Vec::new());
        }

        let content = fs.read_to_string(&settings_path)?;
        let json: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| Error::ConfigParse {
                path: settings_path.clone(),
                reason: format!("invalid JSON in settings.json: {e}"),
            })?;

        let hooks_value = match json.get("hooks") {
            Some(v) if v.is_object() && !v.as_object().is_some_and(serde_json::Map::is_empty) => v,
            _ => return Ok(Vec::new()),
        };

        // Build plugin hooks.json format: { "hooks": { ... } }
        let hooks_json = serde_json::json!({ "hooks": hooks_value });
        let hooks_content =
            serde_json::to_string_pretty(&hooks_json).unwrap_or_else(|_| "{}".to_string());

        // Extract script references from command hooks
        let referenced_scripts = extract_hook_script_references(hooks_value, source_dir);

        Ok(vec![Artifact {
            kind: ArtifactKind::Hook,
            name: "project-hooks".to_string(),
            source_path: settings_path,
            files: Vec::new(), // no files to copy — content is in raw_content
            referenced_scripts,
            metadata: ArtifactMetadata {
                name: Some("project-hooks".to_string()),
                description: Some("Hooks extracted from .claude/settings.json".to_string()),
                raw_content: Some(hooks_content),
                ..ArtifactMetadata::default()
            },
        }])
    }
}

/// Walk hooks JSON recursively to find `"type": "command"` handlers
/// and extract their `"command"` values as script references when they
/// appear to be relative paths.
fn extract_hook_script_references(
    hooks_value: &serde_json::Value,
    source_dir: &Path,
) -> Vec<PathBuf> {
    let mut scripts = Vec::new();
    collect_command_scripts(hooks_value, source_dir, &mut scripts);
    scripts
}

fn collect_command_scripts(
    value: &serde_json::Value,
    source_dir: &Path,
    scripts: &mut Vec<PathBuf>,
) {
    match value {
        serde_json::Value::Object(map) => {
            // Check if this object is a command handler: { "type": "command", "command": "..." }
            let is_command_type =
                map.get("type").and_then(|v| v.as_str()).is_some_and(|t| t == "command");
            if is_command_type {
                if let Some(cmd) = map.get("command").and_then(|v| v.as_str()) {
                    let cmd_trimmed = cmd.trim();
                    // Extract the first token (the executable/script path)
                    let script_path = cmd_trimmed.split_whitespace().next().unwrap_or(cmd_trimmed);
                    if script_path.starts_with("./") || is_relative_script(script_path, source_dir)
                    {
                        scripts.push(PathBuf::from(script_path));
                    }
                }
            }
            // Recurse into all values
            for v in map.values() {
                collect_command_scripts(v, source_dir, scripts);
            }
        },
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_command_scripts(v, source_dir, scripts);
            }
        },
        _ => {},
    }
}

/// Check if a path looks like a relative script (not an absolute path, not a bare command).
pub(super) fn is_relative_script(path: &str, _source_dir: &Path) -> bool {
    if path.is_empty() {
        return false;
    }
    let path_obj = Path::new(path);
    // Reject absolute paths: std check + manual Windows drive letter check
    // (Path::is_absolute on Linux doesn't recognize C:\... as absolute)
    if path_obj.is_absolute() || path_obj.has_root() || has_windows_drive_prefix(path) {
        return false;
    }
    // Has directory separators — it's a path, not a bare command
    if path.contains('/') || path.contains('\\') {
        return true;
    }
    // Check for known script extensions (case-insensitive)
    path_obj.extension().is_some_and(|ext| {
        ext.eq_ignore_ascii_case("sh")
            || ext.eq_ignore_ascii_case("py")
            || ext.eq_ignore_ascii_case("js")
    })
}

/// Check for Windows-style drive letter prefix (e.g., `C:\`, `D:/`).
/// Works on all platforms — `Path::is_absolute` on Linux doesn't detect these.
fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes.first().is_some_and(u8::is_ascii_alphabetic)
        && bytes.get(1) == Some(&b':')
        && (bytes.get(2) == Some(&b'\\') || bytes.get(2) == Some(&b'/'))
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
    fn detect_hooks_from_settings() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.claude/settings.json"));
        fs.files.insert(
            PathBuf::from("/project/.claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[{"type":"command","command":"echo check"}]}}"#.to_string(),
        );

        let detector = HookDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| &a.kind), Some(&ArtifactKind::Hook));
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("project-hooks"));
        assert!(artifacts.first().and_then(|a| a.metadata.raw_content.as_ref()).is_some());
    }

    #[test]
    fn detect_no_settings_json() {
        let fs = MockFs::new();
        let detector = HookDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_no_hooks_key() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.claude/settings.json"));
        fs.files.insert(
            PathBuf::from("/project/.claude/settings.json"),
            r#"{"permissions":["allow"]}"#.to_string(),
        );

        let detector = HookDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_empty_hooks_object() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.claude/settings.json"));
        fs.files
            .insert(PathBuf::from("/project/.claude/settings.json"), r#"{"hooks":{}}"#.to_string());

        let detector = HookDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_malformed_json() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.claude/settings.json"));
        fs.files
            .insert(PathBuf::from("/project/.claude/settings.json"), "not valid json".to_string());

        let detector = HookDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_err());
        assert!(result.err().is_some_and(|e| matches!(e, Error::ConfigParse { .. })));
    }

    #[test]
    fn detect_script_references_from_command_hooks() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.claude/settings.json"));
        fs.files.insert(
            PathBuf::from("/project/.claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[{"type":"command","command":"./scripts/validate.sh --strict"}]}}"#.to_string(),
        );

        let detector = HookDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.referenced_scripts.len()), Some(1));
        assert_eq!(
            artifacts
                .first()
                .and_then(|a| a.referenced_scripts.first())
                .map(|p| p.to_string_lossy().into_owned()),
            Some("./scripts/validate.sh".to_string())
        );
    }

    #[test]
    fn detect_ignores_bare_command_names() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.claude/settings.json"));
        fs.files.insert(
            PathBuf::from("/project/.claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[{"type":"command","command":"echo hello"}]}}"#.to_string(),
        );

        let detector = HookDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        // "echo" is a bare command, not a script reference
        assert_eq!(artifacts.first().map(|a| a.referenced_scripts.len()), Some(0));
    }

    #[test]
    fn detect_hooks_value_not_object() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.claude/settings.json"));
        fs.files.insert(
            PathBuf::from("/project/.claude/settings.json"),
            r#"{"hooks":"not-an-object"}"#.to_string(),
        );

        let detector = HookDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn detect_nested_script_in_array_handler() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.claude/settings.json"));
        fs.files.insert(
            PathBuf::from("/project/.claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[{"type":"command","command":"scripts/check.sh --verbose"},{"type":"command","command":"./run.sh"}]}}"#.to_string(),
        );

        let detector = HookDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.referenced_scripts.len()), Some(2));
    }

    #[test]
    fn detect_script_with_extension_in_command() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.claude/settings.json"));
        fs.files.insert(
            PathBuf::from("/project/.claude/settings.json"),
            r#"{"hooks":{"PostToolUse":[{"type":"command","command":"validate.py arg1"}]}}"#
                .to_string(),
        );

        let detector = HookDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.referenced_scripts.len()), Some(1));
    }

    #[test]
    fn detect_non_command_type_ignored() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.claude/settings.json"));
        fs.files.insert(
            PathBuf::from("/project/.claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[{"type":"interceptor","pattern":"*"}]}}"#.to_string(),
        );

        let detector = HookDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.referenced_scripts.len()), Some(0));
    }

    #[test]
    fn is_relative_script_detects_paths() {
        use std::path::Path;
        // Has directory separators
        assert!(is_relative_script("scripts/check.sh", Path::new(".")));
        // Starts with ./
        assert!(is_relative_script("./run.sh", Path::new(".")));
        // Known script extensions
        assert!(is_relative_script("check.sh", Path::new(".")));
        assert!(is_relative_script("validate.py", Path::new(".")));
        assert!(is_relative_script("lint.js", Path::new(".")));
        // Bare commands (not scripts)
        assert!(!is_relative_script("echo", Path::new(".")));
        assert!(!is_relative_script("npx", Path::new(".")));
        // Absolute paths (Unix)
        assert!(!is_relative_script("/usr/bin/env", Path::new(".")));
        // Absolute paths (Windows)
        assert!(!is_relative_script("C:\\tools\\check.sh", Path::new(".")));
        // Empty
        assert!(!is_relative_script("", Path::new(".")));
    }

    #[test]
    fn detect_absolute_path_command_not_extracted() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.claude/settings.json"));
        fs.files.insert(
            PathBuf::from("/project/.claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[{"type":"command","command":"/usr/bin/check --flag"}]}}"#
                .to_string(),
        );

        let detector = HookDetector;
        let result = detector.detect(Path::new("/project/.claude"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.referenced_scripts.len()), Some(0));
    }

    #[test]
    fn has_windows_drive_prefix_detects_drive_letters() {
        assert!(has_windows_drive_prefix("C:\\tools\\check.sh"));
        assert!(has_windows_drive_prefix("D:/scripts/run.sh"));
        assert!(has_windows_drive_prefix("Z:\\file"));
        assert!(!has_windows_drive_prefix("/usr/bin/env"));
        assert!(!has_windows_drive_prefix("./scripts/run.sh"));
        assert!(!has_windows_drive_prefix("scripts/run.sh"));
        assert!(!has_windows_drive_prefix(""));
        assert!(!has_windows_drive_prefix("C:"));
        assert!(!has_windows_drive_prefix("CC:\\bad"));
    }
}
