//! Shared skill parsing logic used by both Claude and Copilot skill detectors.

use std::path::{Path, PathBuf};

use crate::fs::Fs;

use super::{strip_yaml_quotes, ArtifactMetadata, Error};

/// Parse YAML frontmatter from a SKILL.md file.
///
/// Frontmatter is delimited by `---` lines. Extracts `name`, `description`,
/// and `hooks` fields using simple line-by-line parsing (no YAML parser).
pub fn parse_skill_frontmatter(content: &str, path: &Path) -> Result<ArtifactMetadata, Error> {
    let mut metadata = ArtifactMetadata::default();

    // Find frontmatter between --- delimiters
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Ok(metadata);
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    let rest = after_first.trim_start_matches(['\r', '\n']);
    let closing = rest.find("\n---");
    let yaml_block = match closing {
        Some(pos) => &rest[..pos],
        None => {
            return Err(Error::FrontmatterParse {
                path: path.to_path_buf(),
                reason: "no closing --- delimiter found".to_string(),
            });
        },
    };

    // Parse line by line
    let mut hooks_lines: Vec<String> = Vec::new();
    let mut in_hooks = false;

    for line in yaml_block.lines() {
        let trimmed_line = line.trim();

        // Check if we're in a hooks block (indented continuation)
        if in_hooks {
            if line.starts_with(' ') || line.starts_with('\t') {
                // Strip one level of indentation so the emitter can parse key: value lines
                let stripped =
                    line.strip_prefix("  ").or_else(|| line.strip_prefix('\t')).unwrap_or(line);
                hooks_lines.push(stripped.to_string());
                continue;
            }
            in_hooks = false;
        }

        if let Some(value) = trimmed_line.strip_prefix("name:") {
            metadata.name = Some(strip_yaml_quotes(value.trim()).to_string());
        } else if let Some(value) = trimmed_line.strip_prefix("description:") {
            metadata.description = Some(strip_yaml_quotes(value.trim()).to_string());
        } else if trimmed_line.starts_with("hooks:") {
            in_hooks = true;
            let value = trimmed_line.strip_prefix("hooks:").unwrap_or_default().trim();
            if !value.is_empty() {
                hooks_lines.push(value.to_string());
            }
        } else if let Some(value) = trimmed_line.strip_prefix("disable-model-invocation:") {
            if value.trim() == "true" {
                metadata.model_invocation_disabled = true;
            }
        }
    }

    if !hooks_lines.is_empty() {
        metadata.hooks = Some(hooks_lines.join("\n"));
    }

    Ok(metadata)
}

/// Extract script references from artifact content.
///
/// Matches three patterns:
/// 1. Variable-prefix: `${CLAUDE_SKILL_DIR}/scripts/helper.sh`
/// 2. Relative paths: `./scripts/helper.sh`, `../utils/lib.sh`
/// 3. Bare script invocations: `bash scripts/deploy.sh`, `python utils/run.py`
///
/// URLs (`https://`, `http://`) are excluded. Results are deduplicated.
pub fn extract_script_references(content: &str, variable_prefix: &str) -> Vec<PathBuf> {
    const INTERPRETERS: &[&str] =
        &["bash ", "sh ", "python ", "python3 ", "node ", "ruby ", "perl "];
    let mut refs = Vec::new();

    for line in content.lines() {
        // Pattern 1: existing ${VARIABLE_PREFIX}/scripts/* matching
        let mut search = line;
        while let Some(pos) = search.find(variable_prefix) {
            let after = &search[pos + variable_prefix.len()..];
            let end = after
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '`' || c == ')')
                .unwrap_or(after.len());
            let path_str = &after[..end];
            if path_str.starts_with("scripts/") {
                refs.push(PathBuf::from(path_str));
            }
            search = &search[pos + variable_prefix.len() + end..];
        }

        // Pattern 2: relative path references starting with ./ or ../
        let mut search = line;
        while let Some(idx) = search.find("./") {
            // Skip URLs (https:// or http://)
            if idx > 0 {
                let before = search.as_bytes().get(idx - 1).copied().unwrap_or(0);
                if before == b'/' || before == b':' {
                    search = &search[idx + 2..];
                    continue;
                }
            }
            let start = if idx > 0 && search.as_bytes().get(idx - 1).copied() == Some(b'.') {
                // ../path — back up one char to include the leading ..
                idx - 1
            } else {
                idx
            };
            let path_part = &search[start..];
            let end = path_part
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '`' || c == ')')
                .unwrap_or(path_part.len());
            let path_str = &path_part[..end];
            if path_str.len() > 2 {
                refs.push(PathBuf::from(path_str));
            }
            search = &search[start + end..];
        }

        // Pattern 3: bare script invocations after known interpreters
        for interpreter in INTERPRETERS {
            let mut search = line;
            while let Some(pos) = search.find(interpreter) {
                // Ensure interpreter is at start of line or preceded by whitespace/backtick
                let valid_start = pos == 0 || {
                    let prev = search.as_bytes().get(pos - 1).copied().unwrap_or(0);
                    prev == b' ' || prev == b'\t' || prev == b'`' || prev == b'$' || prev == b'('
                };
                let after = &search[pos + interpreter.len()..];
                let after_trimmed = after.trim_start();
                let end = after_trimmed
                    .find(|c: char| {
                        c.is_whitespace() || c == '"' || c == '\'' || c == '`' || c == ')'
                    })
                    .unwrap_or(after_trimmed.len());
                let path_str = &after_trimmed[..end];
                if valid_start
                    && !path_str.is_empty()
                    && !path_str.starts_with('-')
                    && !path_str.starts_with("http://")
                    && !path_str.starts_with("https://")
                    && (path_str.contains('/') || path_str.contains('.'))
                {
                    refs.push(PathBuf::from(path_str));
                }
                search = &search[pos + interpreter.len()..];
            }
        }
    }

    refs.sort();
    refs.dedup();
    refs
}

/// Recursively collect all files in a directory, returning paths relative to `base`.
pub fn collect_files_recursive(
    dir: &Path,
    base: &Path,
    fs: &dyn Fs,
) -> Result<Vec<PathBuf>, Error> {
    let mut files = Vec::new();
    let entries = fs.read_dir(dir)?;

    for entry in entries {
        let full_path = dir.join(&entry.name);
        if entry.is_dir {
            let sub_files = collect_files_recursive(&full_path, base, fs)?;
            files.extend(sub_files);
        } else if let Ok(relative) = full_path.strip_prefix(base) {
            files.push(relative.to_path_buf());
        }
    }

    Ok(files)
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
    fn parse_frontmatter_extracts_name_and_description() {
        let result = parse_skill_frontmatter(
            "---\nname: deploy\ndescription: Deploy app\n---\nbody",
            Path::new("test"),
        );
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert_eq!(meta.name.as_deref(), Some("deploy"));
        assert_eq!(meta.description.as_deref(), Some("Deploy app"));
    }

    #[test]
    fn parse_frontmatter_no_frontmatter() {
        let result = parse_skill_frontmatter("just plain text", Path::new("test"));
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert!(meta.name.is_none());
    }

    #[test]
    fn parse_frontmatter_no_closing() {
        let result = parse_skill_frontmatter("---\nname: test\nno closing", Path::new("test"));
        assert!(result.is_err());
    }

    #[test]
    fn parse_frontmatter_with_hooks() {
        let result = parse_skill_frontmatter(
            "---\nhooks:\n  PreToolUse: check\n---\nbody",
            Path::new("test"),
        );
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert!(meta.hooks.is_some());
    }

    #[test]
    fn parse_frontmatter_disable_model_invocation() {
        let result = parse_skill_frontmatter(
            "---\ndisable-model-invocation: true\n---\nbody",
            Path::new("test"),
        );
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert!(meta.model_invocation_disabled);
    }

    #[test]
    fn extract_scripts_with_claude_prefix() {
        let content = "Run `${CLAUDE_SKILL_DIR}/scripts/deploy.sh`";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
    }

    #[test]
    fn extract_scripts_with_skill_dir_prefix() {
        let content = "Run `${SKILL_DIR}/scripts/deploy.sh`";
        let scripts = extract_script_references(content, "${SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
    }

    #[test]
    fn extract_scripts_non_script_path_ignored() {
        let content = "Use ${CLAUDE_SKILL_DIR}/readme.md";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert!(scripts.is_empty());
    }

    #[test]
    fn extract_scripts_multiple() {
        let content = "Run ${CLAUDE_SKILL_DIR}/scripts/a.sh and ${CLAUDE_SKILL_DIR}/scripts/b.sh";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 2);
    }

    #[test]
    fn collect_files_flat_dir() {
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from("/base"),
            vec![
                crate::fs::DirEntry { name: "a.txt".to_string(), is_dir: false },
                crate::fs::DirEntry { name: "b.txt".to_string(), is_dir: false },
            ],
        );

        let result = collect_files_recursive(Path::new("/base"), Path::new("/base"), &fs);
        assert!(result.is_ok());
        let files = result.ok().unwrap_or_default();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn collect_files_nested_dir() {
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from("/base"),
            vec![
                crate::fs::DirEntry { name: "a.txt".to_string(), is_dir: false },
                crate::fs::DirEntry { name: "sub".to_string(), is_dir: true },
            ],
        );
        fs.dirs.insert(
            PathBuf::from("/base/sub"),
            vec![crate::fs::DirEntry { name: "c.txt".to_string(), is_dir: false }],
        );

        let result = collect_files_recursive(Path::new("/base"), Path::new("/base"), &fs);
        assert!(result.is_ok());
        let files = result.ok().unwrap_or_default();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn parse_frontmatter_with_hooks_tab_indent() {
        let result = parse_skill_frontmatter(
            "---\nhooks:\n\tPreToolUse: check\n---\nbody",
            Path::new("test"),
        );
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert!(meta.hooks.is_some());
    }

    #[test]
    fn parse_frontmatter_hooks_inline_value() {
        let result =
            parse_skill_frontmatter("---\nhooks: inline-value\n---\nbody", Path::new("test"));
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert!(meta.hooks.is_some());
    }

    #[test]
    fn parse_frontmatter_empty_name() {
        let result =
            parse_skill_frontmatter("---\nname:\ndescription: test\n---\nbody", Path::new("test"));
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert!(meta.name.is_some());
    }

    #[test]
    fn parse_frontmatter_disable_model_invocation_false() {
        let result = parse_skill_frontmatter(
            "---\ndisable-model-invocation: false\n---\nbody",
            Path::new("test"),
        );
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert!(!meta.model_invocation_disabled);
    }

    #[test]
    fn parse_frontmatter_unknown_key_ignored() {
        let result = parse_skill_frontmatter(
            "---\nunknown-key: value\nname: test\n---\nbody",
            Path::new("test"),
        );
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert_eq!(meta.name.as_deref(), Some("test"));
    }

    #[test]
    fn extract_scripts_no_match() {
        let content = "no script references here";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert!(scripts.is_empty());
    }

    #[test]
    fn parse_frontmatter_hooks_single_space_indent_falls_back_to_raw_line() {
        // A hooks continuation line indented with exactly one space (not two spaces,
        // not a tab) exercises the `strip_prefix("  ").or_else(|| strip_prefix('\t')).unwrap_or(line)`
        // fallback — both strip_prefix calls return None, so the original line is kept.
        let content = "---\nhooks:\n PreToolUse: check\n---\nbody";
        let result = parse_skill_frontmatter(content, Path::new("test"));
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        // The line " PreToolUse: check" starts with ' ', so it is treated as a
        // hook continuation; it lands in hooks_lines as-is via unwrap_or.
        assert!(meta.hooks.is_some());
    }

    #[test]
    fn extract_scripts_terminated_by_double_quote() {
        // `c == '"'` True branch: a double-quote following the path ends extraction.
        let content = r#"Run "${CLAUDE_SKILL_DIR}/scripts/deploy.sh" here"#;
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0], PathBuf::from("scripts/deploy.sh"));
    }

    #[test]
    fn extract_scripts_terminated_by_single_quote() {
        // `c == '\''` True branch: a single-quote following the path ends extraction.
        let content = "Run '${CLAUDE_SKILL_DIR}/scripts/deploy.sh' here";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0], PathBuf::from("scripts/deploy.sh"));
    }

    #[test]
    fn extract_scripts_terminated_by_closing_paren() {
        // `c == ')'` True branch: a closing paren following the path ends extraction.
        let content = "Run(${CLAUDE_SKILL_DIR}/scripts/deploy.sh) here";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0], PathBuf::from("scripts/deploy.sh"));
    }

    #[test]
    fn parse_frontmatter_hooks_block_exits_on_non_indented_line() {
        // After a hooks: continuation block, hitting a non-indented line triggers
        // `in_hooks = false` — the False branch of `line.starts_with('\t')` when
        // `line.starts_with(' ')` is also False.
        let content = "---\nhooks:\n\tPreToolUse: check\nname: my-skill\n---\nbody";
        let result = parse_skill_frontmatter(content, Path::new("test"));
        assert!(result.is_ok());
        let meta = result.ok().unwrap_or_default();
        assert_eq!(meta.name.as_deref(), Some("my-skill"));
        assert!(meta.hooks.is_some());
    }

    #[test]
    fn extract_variable_prefix_still_works() {
        let content = "Run `${CLAUDE_SKILL_DIR}/scripts/helper.sh` here";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0], PathBuf::from("scripts/helper.sh"));
    }

    #[test]
    fn extract_relative_dot_slash() {
        let content = "Run `./scripts/helper.sh` to deploy";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0], PathBuf::from("./scripts/helper.sh"));
    }

    #[test]
    fn extract_relative_dot_dot() {
        let content = "Source ../utils/lib.sh for shared functions";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0], PathBuf::from("../utils/lib.sh"));
    }

    #[test]
    fn extract_bare_invocation() {
        let content = "bash scripts/deploy.sh --env prod";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0], PathBuf::from("scripts/deploy.sh"));
    }

    #[test]
    fn extract_ignores_urls() {
        let content =
            "Download from https://example.com/scripts/foo.sh and http://example.com/bar.sh";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert!(scripts.is_empty());
    }

    #[test]
    fn extract_deduplicates() {
        let content = "Run ./scripts/a.sh then ./scripts/a.sh again";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0], PathBuf::from("./scripts/a.sh"));
    }

    #[test]
    fn extract_bare_invocation_python3() {
        let content = "python3 utils/run.py --flag";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0], PathBuf::from("utils/run.py"));
    }

    #[test]
    fn extract_bare_invocation_ignores_flags() {
        let content = "bash -c 'echo hello'";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert!(scripts.is_empty());
    }

    #[test]
    fn collect_files_skips_entries_not_under_base() {
        // When `full_path.strip_prefix(base)` fails — i.e., the entry's absolute
        // path does not start with `base` — the file is silently skipped.
        // This exercises the False (Err) branch of `if let Ok(relative) = ...`.
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from("/other"),
            vec![crate::fs::DirEntry { name: "file.txt".to_string(), is_dir: false }],
        );

        let result = collect_files_recursive(Path::new("/other"), Path::new("/base"), &fs);
        assert!(result.is_ok());
        let files = result.ok().unwrap_or_default();
        // /other/file.txt does not start with /base, so strip_prefix fails and the
        // entry is not collected.
        assert!(files.is_empty());
    }

    #[test]
    fn extract_pattern2_at_start_of_line() {
        // ./path at the very start of a line (idx == 0)
        let content = "./scripts/run.sh arg1";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0], PathBuf::from("./scripts/run.sh"));
    }

    #[test]
    fn extract_pattern2_dot_dot_slash() {
        // ../ at idx > 0 with preceding '.' char
        let content = "source ../shared/utils.sh";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0], PathBuf::from("../shared/utils.sh"));
    }

    #[test]
    fn extract_pattern2_dot_slash_dot_dot_is_captured() {
        // ./..<path> is now captured (no longer filtered out)
        let content = "path ./..invalid here";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0], PathBuf::from("./..invalid"));
    }

    #[test]
    fn extract_pattern2_short_path_skipped() {
        // A path like "./" alone (len == 2) should be skipped
        let content = "path ./ more";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert!(scripts.is_empty());
    }

    #[test]
    fn extract_pattern3_invalid_start() {
        // Interpreter mid-word: "notbash scripts/deploy.sh" — valid_start is false
        let content = "notbash scripts/deploy.sh";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert!(scripts.is_empty());
    }

    #[test]
    fn extract_pattern3_empty_path() {
        // "bash " at end of line — empty path_str
        let content = "bash ";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert!(scripts.is_empty());
    }

    #[test]
    fn extract_pattern3_no_slash_or_dot() {
        // "bash simple" — no directory separator or extension
        let content = "bash simple";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert!(scripts.is_empty());
    }

    #[test]
    fn extract_pattern3_with_url() {
        // "bash https://example.com" — URL should be excluded
        let content = "bash https://example.com/script.sh";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert!(scripts.is_empty());
    }

    #[test]
    fn extract_pattern3_backtick_preceded() {
        // Interpreter preceded by backtick
        let content = "run `bash scripts/test.sh`";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
    }

    #[test]
    fn extract_pattern3_dollar_preceded() {
        // Interpreter preceded by $
        let content = "$(bash scripts/test.sh)";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
    }

    #[test]
    fn extract_pattern3_paren_preceded() {
        // Interpreter preceded by (
        let content = "(bash scripts/test.sh)";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
    }

    #[test]
    fn extract_pattern2_colon_before_skips() {
        // A colon before ./ should skip (like file:./path)
        let content = "file:./path/to/thing";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert!(scripts.is_empty());
    }

    #[test]
    fn extract_multiple_patterns_combined() {
        // Multiple patterns on the same line
        let content =
            "Use ${CLAUDE_SKILL_DIR}/scripts/a.sh and ./scripts/b.sh then bash scripts/c.sh";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 3);
    }

    #[test]
    fn extract_pattern3_with_extension_only() {
        // "node script.js" — has extension but no /
        let content = "node script.js";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0], PathBuf::from("script.js"));
    }

    #[test]
    fn extract_pattern2_slash_before_dot_slash_skipped() {
        // A "./" preceded by "/" (e.g. "//./path") triggers the True branch of
        // `if before == b'/' || before == b':'` and is skipped.
        let content = "//./skipped/path";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert!(scripts.is_empty());
    }

    #[test]
    fn extract_pattern3_with_http_url_is_ignored() {
        // An interpreter followed by an http:// URL (not https://) is excluded.
        // This covers the False branch of `!path_str.starts_with("http://")`.
        let content = "bash http://example.com/run/script.sh";
        let scripts = extract_script_references(content, "${CLAUDE_SKILL_DIR}/");
        assert!(scripts.is_empty());
    }
}
