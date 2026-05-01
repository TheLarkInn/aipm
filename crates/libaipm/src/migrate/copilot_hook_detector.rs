//! Copilot hook detector: reads standalone `hooks.json` with legacy event name normalization.

use std::path::{Path, PathBuf};

use crate::fs::Fs;

use super::detector::Detector;
use super::{Artifact, ArtifactKind, ArtifactMetadata, Error};

/// Reads standalone `hooks.json` files from Copilot CLI source directories.
pub struct CopilotHookDetector;

impl Detector for CopilotHookDetector {
    fn name(&self) -> &'static str {
        "copilot-hook"
    }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        // Check <source_dir>/hooks.json first, then <source_dir>/hooks/hooks.json
        let direct_path = source_dir.join("hooks.json");
        let subdir_path = source_dir.join("hooks").join("hooks.json");

        let hooks_path = if fs.exists(&direct_path) {
            direct_path
        } else if fs.exists(&subdir_path) {
            subdir_path
        } else {
            return Ok(Vec::new());
        };

        let content = fs.read_to_string(&hooks_path)?;

        let json: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| Error::ConfigParse {
                path: hooks_path.clone(),
                reason: format!("invalid JSON in hooks.json: {e}"),
            })?;

        let Some(obj) = json.as_object() else {
            return Ok(Vec::new());
        };

        if obj.is_empty() {
            return Ok(Vec::new());
        }

        // Normalize legacy event names
        let normalized = normalize_hook_events(&json);
        let hooks_content =
            serde_json::to_string_pretty(&normalized).unwrap_or_else(|_| "{}".to_string());

        // Extract script references from command hooks
        let referenced_scripts = extract_hook_script_references(&normalized);

        Ok(vec![Artifact {
            kind: ArtifactKind::Hook,
            name: "copilot-hooks".to_string(),
            source_path: hooks_path,
            files: Vec::new(),
            referenced_scripts,
            metadata: ArtifactMetadata {
                name: Some("copilot-hooks".to_string()),
                description: Some("Hooks from Copilot CLI hooks.json".to_string()),
                raw_content: Some(hooks_content),
                ..ArtifactMetadata::default()
            },
        }])
    }
}

/// Normalize legacy Copilot hook event names to canonical camelCase names.
pub(crate) fn normalize_hook_event_name(name: &str) -> &str {
    match name {
        "SessionStart" => "sessionStart",
        "SessionEnd" => "sessionEnd",
        "UserPromptSubmit" => "userPromptSubmitted",
        "PreToolUse" => "preToolUse",
        "PostToolUse" => "postToolUse",
        "PostToolUseFailure" | "ErrorOccurred" => "errorOccurred",
        "Stop" => "agentStop",
        "SubagentStop" => "subagentStop",
        "PreCompact" => "preCompact",
        other => other, // already canonical or unknown — pass through
    }
}

/// Normalize all top-level keys in a hooks JSON object.
/// Merges arrays when both legacy and canonical keys map to the same canonical name.
pub(crate) fn normalize_hook_events(json: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = json.as_object() else {
        return json.clone();
    };

    let mut normalized = serde_json::Map::new();

    for (key, value) in obj {
        let canonical = normalize_hook_event_name(key);

        if let Some(existing) = normalized.get_mut(canonical) {
            // Merge arrays if both map to the same canonical name
            if let (Some(existing_arr), Some(new_arr)) = (existing.as_array_mut(), value.as_array())
            {
                existing_arr.extend(new_arr.iter().cloned());
            }
        } else {
            normalized.insert(canonical.to_string(), value.clone());
        }
    }

    serde_json::Value::Object(normalized)
}

/// Extract script references from command hooks in the normalized JSON.
pub(crate) fn extract_hook_script_references(json: &serde_json::Value) -> Vec<PathBuf> {
    let mut scripts = Vec::new();
    collect_command_scripts(json, &mut scripts);
    scripts
}

pub(crate) fn collect_command_scripts(value: &serde_json::Value, scripts: &mut Vec<PathBuf>) {
    match value {
        serde_json::Value::Object(map) => {
            let is_command_type =
                map.get("type").and_then(|v| v.as_str()).is_some_and(|t| t == "command");
            if is_command_type {
                if let Some(cmd) = map.get("command").and_then(|v| v.as_str()) {
                    let cmd_trimmed = cmd.trim();
                    let script_path = cmd_trimmed.split_whitespace().next().unwrap_or(cmd_trimmed);
                    if script_path.starts_with("./")
                        || super::hook_detector::is_relative_script(
                            script_path,
                            std::path::Path::new("."),
                        )
                    {
                        scripts.push(PathBuf::from(script_path));
                    }
                }
            }
            for v in map.values() {
                collect_command_scripts(v, scripts);
            }
        },
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_command_scripts(v, scripts);
            }
        },
        _ => {},
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
    fn detect_standalone_hooks_json_at_root() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/hooks.json"));
        fs.files.insert(
            PathBuf::from("/project/.github/hooks.json"),
            r#"{"PreToolUse":[{"type":"command","command":"echo check"}]}"#.to_string(),
        );

        let detector = CopilotHookDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("copilot-hooks"));
        assert_eq!(artifacts.first().map(|a| &a.kind), Some(&ArtifactKind::Hook));
    }

    #[test]
    fn detect_hooks_in_subdirectory() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/hooks/hooks.json"));
        fs.files.insert(
            PathBuf::from("/project/.github/hooks/hooks.json"),
            r#"{"postToolUse":[{"type":"command","command":"echo done"}]}"#.to_string(),
        );

        let detector = CopilotHookDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
    }

    #[test]
    fn hooks_json_at_root_takes_priority() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/hooks.json"));
        fs.exists.insert(PathBuf::from("/project/.github/hooks/hooks.json"));
        fs.files.insert(
            PathBuf::from("/project/.github/hooks.json"),
            r#"{"preToolUse":[{"type":"command","command":"echo root"}]}"#.to_string(),
        );
        fs.files.insert(
            PathBuf::from("/project/.github/hooks/hooks.json"),
            r#"{"preToolUse":[{"type":"command","command":"echo subdir"}]}"#.to_string(),
        );

        let detector = CopilotHookDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        // The root hooks.json should be used
        assert_eq!(
            artifacts.first().map(|a| a.source_path.clone()),
            Some(PathBuf::from("/project/.github/hooks.json"))
        );
    }

    #[test]
    fn legacy_session_start_normalized() {
        let result = normalize_hook_event_name("SessionStart");
        assert_eq!(result, "sessionStart");
    }

    #[test]
    fn legacy_user_prompt_submit_normalized() {
        let result = normalize_hook_event_name("UserPromptSubmit");
        assert_eq!(result, "userPromptSubmitted");
    }

    #[test]
    fn legacy_stop_normalized() {
        let result = normalize_hook_event_name("Stop");
        assert_eq!(result, "agentStop");
    }

    #[test]
    fn canonical_names_pass_through() {
        assert_eq!(normalize_hook_event_name("sessionStart"), "sessionStart");
        assert_eq!(normalize_hook_event_name("preToolUse"), "preToolUse");
        assert_eq!(normalize_hook_event_name("errorOccurred"), "errorOccurred");
    }

    #[test]
    fn no_hooks_files_returns_empty() {
        let fs = MockFs::new();
        let detector = CopilotHookDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn malformed_json_returns_error() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/hooks.json"));
        fs.files.insert(PathBuf::from("/project/.github/hooks.json"), "not valid json".to_string());

        let detector = CopilotHookDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_err());
    }

    #[test]
    fn script_references_extracted_from_command_hooks() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/hooks.json"));
        fs.files.insert(
            PathBuf::from("/project/.github/hooks.json"),
            r#"{"PreToolUse":[{"type":"command","command":"./scripts/validate.sh --strict"}]}"#
                .to_string(),
        );

        let detector = CopilotHookDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.referenced_scripts.len()), Some(1));
    }

    #[test]
    fn empty_hooks_object_returns_empty() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/hooks.json"));
        fs.files.insert(PathBuf::from("/project/.github/hooks.json"), r#"{}"#.to_string());

        let detector = CopilotHookDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn normalize_remaining_legacy_names() {
        assert_eq!(normalize_hook_event_name("SessionEnd"), "sessionEnd");
        assert_eq!(normalize_hook_event_name("PostToolUse"), "postToolUse");
        assert_eq!(normalize_hook_event_name("PostToolUseFailure"), "errorOccurred");
        assert_eq!(normalize_hook_event_name("ErrorOccurred"), "errorOccurred");
        assert_eq!(normalize_hook_event_name("SubagentStop"), "subagentStop");
        assert_eq!(normalize_hook_event_name("PreCompact"), "preCompact");
    }

    #[test]
    fn normalize_merges_duplicate_canonical_keys() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"PreToolUse":[{"type":"command","command":"echo a"}],"preToolUse":[{"type":"command","command":"echo b"}]}"#,
        ).ok().unwrap_or_default();
        let normalized = normalize_hook_events(&json);
        let arr = normalized.get("preToolUse").and_then(|v| v.as_array());
        assert_eq!(arr.map(Vec::len), Some(2));
    }

    #[test]
    fn normalize_non_object_passes_through() {
        let json = serde_json::Value::String("not an object".to_string());
        let result = normalize_hook_events(&json);
        assert_eq!(result, json);
    }

    #[test]
    fn detect_hooks_json_is_array_returns_empty() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/hooks.json"));
        fs.files.insert(PathBuf::from("/project/.github/hooks.json"), r#"[1,2,3]"#.to_string());

        let detector = CopilotHookDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn normalize_skips_merge_when_existing_is_not_array() {
        // "PreToolUse" normalizes to canonical "preToolUse".
        // If two keys normalize to the same canonical but the already-stored
        // value is not an array, the tuple destructure in the merge branch
        // fails → covers the False branch of
        // `if let (Some(existing_arr), Some(new_arr)) = (...)`.
        let json: serde_json::Value = serde_json::from_str(
            r#"{"PreToolUse":"not-an-array","preToolUse":[{"type":"command","command":"echo b"}]}"#,
        )
        .ok()
        .unwrap_or_default();
        let normalized = normalize_hook_events(&json);
        assert!(normalized.get("preToolUse").is_some());
    }

    #[test]
    fn command_type_without_command_key_yields_no_script_refs() {
        // A hook entry declares itself as type "command" but omits the
        // "command" key → covers the False branch of
        // `if let Some(cmd) = map.get("command").and_then(|v| v.as_str())`.
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/hooks.json"));
        fs.files.insert(
            PathBuf::from("/project/.github/hooks.json"),
            r#"{"PreToolUse":[{"type":"command"}]}"#.to_string(),
        );

        let detector = CopilotHookDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.referenced_scripts.len()), Some(0));
    }

    #[test]
    fn relative_script_without_dot_slash_prefix_is_extracted() {
        // A command script that does NOT start with "./" but has a .sh
        // extension is still recognised as a relative script via
        // `is_relative_script` → covers the True branch of that second
        // condition in `if script_path.starts_with("./") || is_relative_script(...)`.
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/hooks.json"));
        fs.files.insert(
            PathBuf::from("/project/.github/hooks.json"),
            r#"{"PreToolUse":[{"type":"command","command":"run.sh --arg"}]}"#.to_string(),
        );

        let detector = CopilotHookDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.referenced_scripts.len()), Some(1));
    }
}
