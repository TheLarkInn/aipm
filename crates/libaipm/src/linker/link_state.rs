//! Link state tracking via `.aipm/links.toml`.
//!
//! Tracks active dev link overrides created by `aipm link`.
//! Each entry records the package name, local path, and timestamp.

use std::path::{Path, PathBuf};

use super::error::Error;
use crate::fs::Fs;

/// A single dev link override entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct LinkEntry {
    /// The package name being overridden.
    pub name: String,
    /// The local filesystem path linked to.
    pub path: PathBuf,
    /// ISO-8601 timestamp when the link was created.
    pub linked_at: String,
}

/// The top-level structure of `.aipm/links.toml`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct State {
    /// Active dev link entries.
    #[serde(default)]
    pub link: Vec<LinkEntry>,
}

/// Read the link state from `.aipm/links.toml`.
///
/// Returns a default (empty) state if the file does not exist.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file exists but cannot be read or parsed.
pub fn read(fs: &dyn Fs, links_toml: &Path) -> Result<State, Error> {
    crate::fs::read_toml_or_default::<State>(fs, links_toml)
        .map_err(|source| Error::Io { path: links_toml.to_path_buf(), source })
}

/// Write the link state to `.aipm/links.toml`.
///
/// Creates parent directories if needed. Includes a header comment.
///
/// # Errors
///
/// Returns [`Error::Io`] if writing fails.
pub fn write(fs: &dyn Fs, links_toml: &Path, state: &State) -> Result<(), Error> {
    let toml_str = toml::to_string_pretty(state).map_err(|e| Error::Io {
        path: links_toml.to_path_buf(),
        source: std::io::Error::other(e.to_string()),
    })?;

    let content =
        format!("# Managed by aipm \u{2014} tracks active dev link overrides\n{toml_str}");
    fs.write_file_with_parents(links_toml, content.as_bytes())
        .map_err(|source| Error::Io { path: links_toml.to_path_buf(), source })
}

/// Add a link entry for a package. Replaces any existing entry for the same name.
///
/// # Errors
///
/// Returns [`Error::Io`] if read/write fails.
pub fn add(fs: &dyn Fs, links_toml: &Path, entry: LinkEntry) -> Result<(), Error> {
    let mut state = read(fs, links_toml)?;
    state.link.retain(|e| e.name != entry.name);
    state.link.push(entry);
    write(fs, links_toml, &state)
}

/// Remove a link entry by package name.
///
/// # Errors
///
/// Returns [`Error::Io`] if read/write fails.
pub fn remove(fs: &dyn Fs, links_toml: &Path, package_name: &str) -> Result<(), Error> {
    let mut state = read(fs, links_toml)?;
    state.link.retain(|e| e.name != package_name);
    write(fs, links_toml, &state)
}

/// Clear all link entries (used by `aipm install --locked`).
///
/// # Errors
///
/// Returns [`Error::Io`] if read/write fails.
pub fn clear_all(fs: &dyn Fs, links_toml: &Path) -> Result<(), Error> {
    write(fs, links_toml, &State::default())
}

/// List all active link entries.
///
/// # Errors
///
/// Returns [`Error::Io`] if read fails.
pub fn list(fs: &dyn Fs, links_toml: &Path) -> Result<Vec<LinkEntry>, Error> {
    let state = read(fs, links_toml)?;
    Ok(state.link)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: &crate::fs::Real = &crate::fs::Real;

    fn make_entry(name: &str, path: &str) -> LinkEntry {
        LinkEntry {
            name: name.to_string(),
            path: PathBuf::from(path),
            linked_at: "2026-03-26T14:30:00Z".to_string(),
        }
    }

    #[test]
    fn read_nonexistent_returns_default() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".aipm/links.toml");

        let state = read(FS, &path).expect("read");
        assert!(state.link.is_empty());
    }

    #[test]
    fn write_and_read_roundtrip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".aipm/links.toml");

        let state = State { link: vec![make_entry("code-review", "/work/code-review")] };

        assert!(write(FS, &path, &state).is_ok());

        let read_back = read(FS, &path).expect("read back");
        assert_eq!(read_back.link.len(), 1);
        assert_eq!(read_back.link.first().map(|e| e.name.as_str()), Some("code-review"));
    }

    #[test]
    fn add_creates_entry() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".aipm/links.toml");

        assert!(add(FS, &path, make_entry("pkg-a", "/work/pkg-a")).is_ok());

        let entries = list(FS, &path).expect("list");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries.first().map(|e| e.name.as_str()), Some("pkg-a"));
    }

    #[test]
    fn add_replaces_existing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".aipm/links.toml");

        assert!(add(FS, &path, make_entry("pkg-a", "/old/path")).is_ok());
        assert!(add(FS, &path, make_entry("pkg-a", "/new/path")).is_ok());

        let entries = list(FS, &path).expect("list");
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries.first().map(|e| e.path.to_string_lossy().into_owned()),
            Some("/new/path".to_string())
        );
    }

    #[test]
    fn remove_entry() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".aipm/links.toml");

        assert!(add(FS, &path, make_entry("pkg-a", "/work/a")).is_ok());
        assert!(add(FS, &path, make_entry("pkg-b", "/work/b")).is_ok());
        assert!(remove(FS, &path, "pkg-a").is_ok());

        let entries = list(FS, &path).expect("list");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries.first().map(|e| e.name.as_str()), Some("pkg-b"));
    }

    #[test]
    fn clear_all_removes_everything() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".aipm/links.toml");

        assert!(add(FS, &path, make_entry("pkg-a", "/work/a")).is_ok());
        assert!(add(FS, &path, make_entry("pkg-b", "/work/b")).is_ok());
        assert!(clear_all(FS, &path).is_ok());

        let entries = list(FS, &path).expect("list");
        assert!(entries.is_empty());
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".aipm/links.toml");

        assert!(add(FS, &path, make_entry("pkg-a", "/work/a")).is_ok());
        assert!(remove(FS, &path, "nonexistent").is_ok());

        let entries = list(FS, &path).expect("list");
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn written_file_has_header_comment() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".aipm/links.toml");

        assert!(write(FS, &path, &State::default()).is_ok());

        let content = std::fs::read_to_string(&path).expect("read");
        assert!(content.starts_with("# Managed by aipm"));
    }

    #[test]
    fn read_invalid_toml_returns_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("links.toml");

        // Write invalid TOML content
        std::fs::write(&path, "[[link]\nNOT VALID TOML :::").expect("write");

        let result = read(FS, &path);
        assert!(result.is_err());
    }

    #[test]
    fn read_other_io_error_propagates() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // Use a directory path as a file path — read_to_string will fail with EISDIR
        let dir_as_file = tmp.path().to_path_buf();
        let result = read(FS, &dir_as_file);
        assert!(result.is_err());
    }

    #[test]
    fn write_to_root_returns_error() {
        // write_file_with_parents on "/" fails with EISDIR or permission error.
        let result = write(FS, Path::new("/"), &State::default());
        assert!(result.is_err());
    }
}
