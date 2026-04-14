//! Managed `.gitignore` section for registry-installed packages.
//!
//! Entries between the `aipm managed start` and `aipm managed end` markers
//! are owned by AIPM. Manual entries outside the markers are preserved.

use std::path::Path;

use super::error::Error;
use crate::fs::Fs;

/// Start marker for the AIPM-managed section.
const MARKER_START: &str = "# === aipm managed start ===";
/// End marker for the AIPM-managed section.
const MARKER_END: &str = "# === aipm managed end ===";
/// Header comment placed before the start marker.
const HEADER: &str = "# Managed by aipm — do not edit between markers";

/// Add a package entry to the managed section of a `.gitignore` file.
///
/// Creates the file with markers if it does not exist. For scoped packages
/// (e.g. `@company/plugin`), adds both the full name and the scope directory.
///
/// # Errors
///
/// Returns [`Error::Io`] if file read/write fails.
pub fn add_entry(fs: &dyn Fs, gitignore_path: &Path, package_name: &str) -> Result<(), Error> {
    let content = read_or_default(fs, gitignore_path)?;
    let (before, mut managed, after) = split_sections(&content);

    // Add the package name if not already present.
    if !managed.iter().any(|e| e == package_name) {
        managed.push(package_name.to_string());
    }

    // For scoped packages, also add the scope directory.
    if let Some(scope) = extract_scope(package_name) {
        let scope_entry = format!("{scope}/");
        if !managed.iter().any(|e| e == &scope_entry) {
            managed.push(scope_entry);
        }
    }

    managed.sort();
    managed.dedup();

    let output = build_content(&before, &managed, &after);
    write_gitignore(fs, gitignore_path, &output)
}

/// Remove a package entry from the managed section.
///
/// # Errors
///
/// Returns [`Error::Io`] if file read/write fails.
pub fn remove_entry(fs: &dyn Fs, gitignore_path: &Path, package_name: &str) -> Result<(), Error> {
    let content = read_or_default(fs, gitignore_path)?;
    let (before, mut managed, after) = split_sections(&content);

    managed.retain(|e| e != package_name);

    // Remove scope directory if no other packages share the scope.
    if let Some(scope) = extract_scope(package_name) {
        let scope_entry = format!("{scope}/");
        let has_other_scoped =
            managed.iter().any(|e| e.starts_with(&format!("{scope}/")) && e != &scope_entry);
        if !has_other_scoped {
            managed.retain(|e| e != &scope_entry);
        }
    }

    let output = build_content(&before, &managed, &after);
    write_gitignore(fs, gitignore_path, &output)
}

/// Read the managed entries from a `.gitignore` file.
///
/// # Errors
///
/// Returns [`Error::Io`] if file read fails.
pub fn read_entries(fs: &dyn Fs, gitignore_path: &Path) -> Result<Vec<String>, Error> {
    let content = read_or_default(fs, gitignore_path)?;
    let (_, managed, _) = split_sections(&content);
    Ok(managed)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Read gitignore content, returning empty string if file doesn't exist.
fn read_or_default(fs: &dyn Fs, path: &Path) -> Result<String, Error> {
    match fs.read_to_string(path) {
        Ok(content) => Ok(content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(Error::Io { path: path.to_path_buf(), source: e }),
    }
}

/// Split gitignore content into (before, managed, after) sections around markers.
fn split_sections(content: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut before = Vec::new();
    let mut managed = Vec::new();
    let mut after = Vec::new();

    let start_idx = lines.iter().position(|l| l.trim() == MARKER_START);
    let end_idx = lines.iter().position(|l| l.trim() == MARKER_END);

    match (start_idx, end_idx) {
        (Some(start), Some(end)) if start < end => {
            // Filter out the header comment line that precedes the start marker.
            for line in lines.get(..start).unwrap_or_default() {
                if line.trim() != HEADER {
                    before.push((*line).to_string());
                }
            }
            for line in lines.get(start + 1..end).unwrap_or_default() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    managed.push(trimmed.to_string());
                }
            }
            for line in lines.get(end + 1..).unwrap_or_default() {
                after.push((*line).to_string());
            }
        },
        _ => {
            // No valid marker pair found — all content is "before".
            for line in &lines {
                before.push((*line).to_string());
            }
        },
    }

    (before, managed, after)
}

/// Rebuild gitignore content from sections.
fn build_content(before: &[String], managed: &[String], after: &[String]) -> String {
    let mut lines = Vec::new();

    for line in before {
        lines.push(line.as_str());
    }

    lines.push(HEADER);
    lines.push(MARKER_START);
    for entry in managed {
        lines.push(entry.as_str());
    }
    lines.push(MARKER_END);

    for line in after {
        lines.push(line.as_str());
    }

    let mut result = lines.join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Extract the scope from a scoped package name (e.g. `@company` from `@company/plugin`).
fn extract_scope(name: &str) -> Option<&str> {
    if name.starts_with('@') {
        name.split_once('/').map(|(scope, _)| scope)
    } else {
        None
    }
}

/// Write content to the gitignore file, creating parent dirs if needed.
fn write_gitignore(fs: &dyn Fs, path: &Path, content: &str) -> Result<(), Error> {
    fs.write_file_with_parents(path, content.as_bytes())
        .map_err(|source| Error::Io { path: path.to_path_buf(), source })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: &crate::fs::Real = &crate::fs::Real;

    #[test]
    fn add_entry_creates_file_with_markers() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(FS, &gitignore, "code-review").is_ok());

        let content = std::fs::read_to_string(&gitignore).expect("read");
        assert!(content.contains(MARKER_START));
        assert!(content.contains(MARKER_END));
        assert!(content.contains("code-review"));
    }

    #[test]
    fn add_entry_preserves_existing_manual_entries() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        std::fs::write(&gitignore, "node_modules/\n.env\n").expect("write");

        assert!(add_entry(FS, &gitignore, "my-plugin").is_ok());

        let content = std::fs::read_to_string(&gitignore).expect("read");
        assert!(content.contains("node_modules/"));
        assert!(content.contains(".env"));
        assert!(content.contains("my-plugin"));
    }

    #[test]
    fn add_entry_idempotent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(FS, &gitignore, "pkg").is_ok());
        assert!(add_entry(FS, &gitignore, "pkg").is_ok());

        let entries = read_entries(FS, &gitignore).expect("read");
        assert_eq!(entries.iter().filter(|e| *e == "pkg").count(), 1);
    }

    #[test]
    fn add_scoped_entry_adds_scope_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(FS, &gitignore, "@company/review-plugin").is_ok());

        let entries = read_entries(FS, &gitignore).expect("read");
        assert!(entries.contains(&"@company/review-plugin".to_string()));
        assert!(entries.contains(&"@company/".to_string()));
    }

    #[test]
    fn remove_entry_removes_package() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(FS, &gitignore, "pkg-a").is_ok());
        assert!(add_entry(FS, &gitignore, "pkg-b").is_ok());
        assert!(remove_entry(FS, &gitignore, "pkg-a").is_ok());

        let entries = read_entries(FS, &gitignore).expect("read");
        assert!(!entries.contains(&"pkg-a".to_string()));
        assert!(entries.contains(&"pkg-b".to_string()));
    }

    #[test]
    fn remove_scoped_entry_removes_scope_when_last() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(FS, &gitignore, "@company/plugin-a").is_ok());
        assert!(remove_entry(FS, &gitignore, "@company/plugin-a").is_ok());

        let entries = read_entries(FS, &gitignore).expect("read");
        assert!(!entries.contains(&"@company/".to_string()));
    }

    #[test]
    fn remove_scoped_entry_keeps_scope_when_others_exist() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(FS, &gitignore, "@company/plugin-a").is_ok());
        assert!(add_entry(FS, &gitignore, "@company/plugin-b").is_ok());
        assert!(remove_entry(FS, &gitignore, "@company/plugin-a").is_ok());

        let entries = read_entries(FS, &gitignore).expect("read");
        assert!(!entries.contains(&"@company/plugin-a".to_string()));
        assert!(entries.contains(&"@company/plugin-b".to_string()));
        // Scope dir still needed for plugin-b.
        assert!(entries.contains(&"@company/".to_string()));
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(FS, &gitignore, "existing").is_ok());
        assert!(remove_entry(FS, &gitignore, "nonexistent").is_ok());

        let entries = read_entries(FS, &gitignore).expect("read");
        assert!(entries.contains(&"existing".to_string()));
    }

    #[test]
    fn read_entries_empty_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        let entries = read_entries(FS, &gitignore).expect("read");
        assert!(entries.is_empty());
    }

    #[test]
    fn preserves_content_after_markers() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        let initial =
            format!("before\n{HEADER}\n{MARKER_START}\nold-pkg\n{MARKER_END}\nafter-content\n");
        std::fs::write(&gitignore, &initial).expect("write");

        assert!(add_entry(FS, &gitignore, "new-pkg").is_ok());

        let content = std::fs::read_to_string(&gitignore).expect("read");
        assert!(content.contains("before"));
        assert!(content.contains("after-content"));
        assert!(content.contains("new-pkg"));
        assert!(content.contains("old-pkg"));
    }

    #[test]
    fn split_sections_reversed_markers_treated_as_no_markers() {
        // If end marker comes before start marker, treat all as "before"
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        let content = format!("line1\n{MARKER_END}\nline2\n{MARKER_START}\nline3\n");
        std::fs::write(&gitignore, &content).expect("write");

        let entries = read_entries(FS, &gitignore).expect("read");
        // No managed entries since markers are reversed
        assert!(entries.is_empty());
    }

    #[test]
    fn split_sections_only_start_marker_no_end() {
        // Only start marker present — treated as no valid marker pair, all content is "before"
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        let content = format!("line1\n{MARKER_START}\nsome-pkg\n");
        std::fs::write(&gitignore, &content).expect("write");

        let entries = read_entries(FS, &gitignore).expect("read");
        assert!(entries.is_empty());
    }

    #[test]
    fn add_scoped_entry_scope_dir_already_present_is_idempotent() {
        // Adding a second package under the same scope shouldn't duplicate the scope entry
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(FS, &gitignore, "@company/plugin-a").is_ok());
        assert!(add_entry(FS, &gitignore, "@company/plugin-b").is_ok());

        let entries = read_entries(FS, &gitignore).expect("read");
        let scope_count = entries.iter().filter(|e| *e == "@company/").count();
        assert_eq!(scope_count, 1, "scope directory should appear exactly once");
    }

    #[test]
    fn remove_scoped_entry_with_another_scoped_package_keeps_scope() {
        // When removing @company/plugin-a while @company/plugin-b still exists,
        // the @company/ scope dir must remain because plugin-b still uses it.
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(FS, &gitignore, "@company/plugin-a").is_ok());
        assert!(add_entry(FS, &gitignore, "@company/plugin-b").is_ok());
        assert!(remove_entry(FS, &gitignore, "@company/plugin-a").is_ok());

        let entries = read_entries(FS, &gitignore).expect("read");
        assert!(entries.contains(&"@company/".to_string()));
        assert!(!entries.contains(&"@company/plugin-a".to_string()));
        assert!(entries.contains(&"@company/plugin-b".to_string()));
    }

    #[test]
    fn managed_section_ignores_comment_lines_within_markers() {
        // Lines starting with # inside the managed section are filtered out
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        let content = format!("{HEADER}\n{MARKER_START}\n# a comment\nreal-pkg\n{MARKER_END}\n");
        std::fs::write(&gitignore, &content).expect("write");

        let entries = read_entries(FS, &gitignore).expect("read");
        assert_eq!(entries.len(), 1);
        assert!(entries.contains(&"real-pkg".to_string()));
    }

    #[test]
    fn read_or_default_returns_error_on_permission_denied() {
        // read_or_default only returns Ok(empty) for NotFound; other errors propagate
        // We test this by trying to read a path that is a directory, not a file,
        // so read_to_string will fail with a non-NotFound error.
        let tmp = tempfile::tempdir().expect("tempdir");
        // A directory path passed as a file should yield an error (IsADirectory)
        let dir_as_file = tmp.path().to_path_buf();
        let result = read_entries(FS, &dir_as_file);
        // On Linux reading a directory with read_to_string fails with EISDIR
        // which is not NotFound, so it should propagate as Err
        assert!(result.is_err());
    }

    #[test]
    fn add_entry_at_sign_without_slash_is_not_scoped() {
        // extract_scope returns None when name starts with '@' but has no '/'.
        // This covers the Option::map() on None branch inside extract_scope.
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        // "@company" has no '/', so extract_scope returns None — no scope dir added.
        assert!(add_entry(FS, &gitignore, "@company").is_ok());

        let entries = read_entries(FS, &gitignore).expect("read");
        assert!(entries.contains(&"@company".to_string()));
        // No scope directory should be added since there is no '/'.
        assert!(!entries.iter().any(|e| e.ends_with('/')));
    }

    #[test]
    fn split_sections_empty_line_in_managed_section_is_skipped() {
        // Covers the short-circuit `!trimmed.is_empty()` false branch in split_sections.
        // An empty line between the markers is not added to managed entries.
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        // Insert a blank line inside the managed section.
        let content = format!("{HEADER}\n{MARKER_START}\n\nreal-pkg\n{MARKER_END}\n");
        std::fs::write(&gitignore, &content).expect("write");

        let entries = read_entries(FS, &gitignore).expect("read");
        // Only real-pkg should appear; the blank line is filtered out.
        assert_eq!(entries.len(), 1);
        assert!(entries.contains(&"real-pkg".to_string()));
    }

    #[test]
    fn build_content_trailing_newline_not_doubled_when_after_empty_line() {
        // Covers the `!result.ends_with('\n')` false branch in build_content.
        // When the content after MARKER_END ends with a blank line, join("\n") already
        // produces a trailing '\n', so the explicit push must be skipped.
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        // The double '\n' at the end causes `lines()` to produce a trailing "" element
        // so that after build_content's join the string already ends with '\n'.
        let content = format!("{HEADER}\n{MARKER_START}\npkg\n{MARKER_END}\n\n");
        std::fs::write(&gitignore, &content).expect("write");

        // Adding an entry round-trips through build_content.
        assert!(add_entry(FS, &gitignore, "new-pkg").is_ok());

        let written = std::fs::read_to_string(&gitignore).expect("read");
        // Must end with exactly one newline (not two).
        assert!(written.ends_with('\n'));
        assert!(!written.ends_with("\n\n"));
    }

    #[test]
    fn remove_scoped_entry_scope_removed_when_non_scoped_packages_also_present() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(FS, &gitignore, "plain-pkg").is_ok());
        assert!(add_entry(FS, &gitignore, "@company/plugin-a").is_ok());
        assert!(remove_entry(FS, &gitignore, "@company/plugin-a").is_ok());

        let entries = read_entries(FS, &gitignore).expect("read entries");
        // Non-scoped package must survive.
        assert!(entries.contains(&"plain-pkg".to_string()));
        // Removed package must be gone.
        assert!(!entries.contains(&"@company/plugin-a".to_string()));
        // Scope directory must also be removed — no other @company packages remain.
        assert!(!entries.contains(&"@company/".to_string()));
    }

    #[test]
    fn write_gitignore_empty_path_returns_io_error() {
        // write_file_with_parents on an empty path fails with an I/O error.
        let result = write_gitignore(FS, Path::new(""), "content");
        assert!(result.is_err());
    }
}
