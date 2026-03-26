//! Managed `.gitignore` section for registry-installed packages.
//!
//! Entries between the `aipm managed start` and `aipm managed end` markers
//! are owned by AIPM. Manual entries outside the markers are preserved.

use std::path::Path;

use super::error::Error;

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
pub fn add_entry(gitignore_path: &Path, package_name: &str) -> Result<(), Error> {
    let content = read_or_default(gitignore_path)?;
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
    write_gitignore(gitignore_path, &output)
}

/// Remove a package entry from the managed section.
///
/// # Errors
///
/// Returns [`Error::Io`] if file read/write fails.
pub fn remove_entry(gitignore_path: &Path, package_name: &str) -> Result<(), Error> {
    let content = read_or_default(gitignore_path)?;
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
    write_gitignore(gitignore_path, &output)
}

/// Read the managed entries from a `.gitignore` file.
///
/// # Errors
///
/// Returns [`Error::Io`] if file read fails.
pub fn read_entries(gitignore_path: &Path) -> Result<Vec<String>, Error> {
    let content = read_or_default(gitignore_path)?;
    let (_, managed, _) = split_sections(&content);
    Ok(managed)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Read gitignore content, returning empty string if file doesn't exist.
fn read_or_default(path: &Path) -> Result<String, Error> {
    match std::fs::read_to_string(path) {
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
fn write_gitignore(path: &Path, content: &str) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
    }
    std::fs::write(path, content).map_err(|e| Error::Io { path: path.to_path_buf(), source: e })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_entry_creates_file_with_markers() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(&gitignore, "code-review").is_ok());

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

        assert!(add_entry(&gitignore, "my-plugin").is_ok());

        let content = std::fs::read_to_string(&gitignore).expect("read");
        assert!(content.contains("node_modules/"));
        assert!(content.contains(".env"));
        assert!(content.contains("my-plugin"));
    }

    #[test]
    fn add_entry_idempotent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(&gitignore, "pkg").is_ok());
        assert!(add_entry(&gitignore, "pkg").is_ok());

        let entries = read_entries(&gitignore).expect("read");
        assert_eq!(entries.iter().filter(|e| *e == "pkg").count(), 1);
    }

    #[test]
    fn add_scoped_entry_adds_scope_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(&gitignore, "@company/review-plugin").is_ok());

        let entries = read_entries(&gitignore).expect("read");
        assert!(entries.contains(&"@company/review-plugin".to_string()));
        assert!(entries.contains(&"@company/".to_string()));
    }

    #[test]
    fn remove_entry_removes_package() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(&gitignore, "pkg-a").is_ok());
        assert!(add_entry(&gitignore, "pkg-b").is_ok());
        assert!(remove_entry(&gitignore, "pkg-a").is_ok());

        let entries = read_entries(&gitignore).expect("read");
        assert!(!entries.contains(&"pkg-a".to_string()));
        assert!(entries.contains(&"pkg-b".to_string()));
    }

    #[test]
    fn remove_scoped_entry_removes_scope_when_last() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(&gitignore, "@company/plugin-a").is_ok());
        assert!(remove_entry(&gitignore, "@company/plugin-a").is_ok());

        let entries = read_entries(&gitignore).expect("read");
        assert!(!entries.contains(&"@company/".to_string()));
    }

    #[test]
    fn remove_scoped_entry_keeps_scope_when_others_exist() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(&gitignore, "@company/plugin-a").is_ok());
        assert!(add_entry(&gitignore, "@company/plugin-b").is_ok());
        assert!(remove_entry(&gitignore, "@company/plugin-a").is_ok());

        let entries = read_entries(&gitignore).expect("read");
        assert!(!entries.contains(&"@company/plugin-a".to_string()));
        assert!(entries.contains(&"@company/plugin-b".to_string()));
        // Scope dir still needed for plugin-b.
        assert!(entries.contains(&"@company/".to_string()));
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        assert!(add_entry(&gitignore, "existing").is_ok());
        assert!(remove_entry(&gitignore, "nonexistent").is_ok());

        let entries = read_entries(&gitignore).expect("read");
        assert!(entries.contains(&"existing".to_string()));
    }

    #[test]
    fn read_entries_empty_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        let entries = read_entries(&gitignore).expect("read");
        assert!(entries.is_empty());
    }

    #[test]
    fn preserves_content_after_markers() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gitignore = tmp.path().join(".gitignore");

        let initial =
            format!("before\n{HEADER}\n{MARKER_START}\nold-pkg\n{MARKER_END}\nafter-content\n");
        std::fs::write(&gitignore, &initial).expect("write");

        assert!(add_entry(&gitignore, "new-pkg").is_ok());

        let content = std::fs::read_to_string(&gitignore).expect("read");
        assert!(content.contains("before"));
        assert!(content.contains("after-content"));
        assert!(content.contains("new-pkg"));
        assert!(content.contains("old-pkg"));
    }
}
