//! Filesystem abstraction for testability.
//!
//! Production code uses [`Real`] which delegates to `std::fs`.
//! Tests can inject a mock implementation to simulate I/O errors
//! without touching the real filesystem.

use std::path::Path;

use serde::de::DeserializeOwned;

/// A directory entry returned by `Fs::read_dir`.
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// The file or directory name (not the full path).
    pub name: String,
    /// Whether this entry is a directory.
    pub is_dir: bool,
}

/// Abstraction over filesystem operations used by init and `workspace_init`.
///
/// `Send + Sync` enables sharing across threads for parallel detection and emission.
///
/// The core five methods are required; the extended methods (for install/link)
/// have default implementations that return `Unsupported` so that existing
/// mock implementations are not broken.
pub trait Fs: Send + Sync {
    /// Check if a path exists.
    fn exists(&self, path: &Path) -> bool;

    /// Check if a path is a regular file (not a directory, symlink-to-directory, or special file).
    fn is_file(&self, path: &Path) -> bool {
        self.exists(path)
    }

    /// Recursively create directories.
    fn create_dir_all(&self, path: &Path) -> std::io::Result<()>;

    /// Create (or truncate) a file and write content.
    fn write_file(&self, path: &Path, content: &[u8]) -> std::io::Result<()>;

    /// Read a file's entire contents as a string.
    fn read_to_string(&self, path: &Path) -> std::io::Result<String>;

    /// List entries in a directory. Returns file names (not full paths).
    fn read_dir(&self, path: &Path) -> std::io::Result<Vec<DirEntry>>;

    // -----------------------------------------------------------------
    // Extended methods for install/link operations (default = unsupported)
    // -----------------------------------------------------------------

    /// Remove a file.
    fn remove_file(&self, _path: &Path) -> std::io::Result<()> {
        Err(std::io::Error::other("remove_file not implemented"))
    }

    /// Remove a directory and all contents.
    fn remove_dir_all(&self, _path: &Path) -> std::io::Result<()> {
        Err(std::io::Error::other("remove_dir_all not implemented"))
    }

    /// Create a hard link from `source` to `target`.
    fn hard_link(&self, _source: &Path, _target: &Path) -> std::io::Result<()> {
        Err(std::io::Error::other("hard_link not implemented"))
    }

    /// Copy a file from `source` to `target`.
    fn copy_file(&self, _source: &Path, _target: &Path) -> std::io::Result<u64> {
        Err(std::io::Error::other("copy_file not implemented"))
    }

    /// Create a symlink (Unix) or directory junction (Windows) from `source` to `target`.
    fn symlink_dir(&self, _source: &Path, _target: &Path) -> std::io::Result<()> {
        Err(std::io::Error::other("symlink_dir not implemented"))
    }

    /// Read the target of a symlink.
    fn read_link(&self, _path: &Path) -> std::io::Result<std::path::PathBuf> {
        Err(std::io::Error::other("read_link not implemented"))
    }

    /// Check if a path is a symlink or junction.
    fn is_symlink(&self, _path: &Path) -> bool {
        false
    }

    /// Atomically write a file (write to temp, then rename).
    fn atomic_write(&self, _path: &Path, _content: &[u8]) -> std::io::Result<()> {
        Err(std::io::Error::other("atomic_write not implemented"))
    }

    // -----------------------------------------------------------------
    // Convenience defaults (compose required methods above)
    // -----------------------------------------------------------------

    /// Creates parent directories if needed, then writes the file.
    ///
    /// Equivalent to `create_dir_all(parent) + write_file(path, content)`.
    /// All 6+ locations in the codebase that do this manually can call
    /// this method instead.
    fn write_file_with_parents(&self, path: &Path, content: &[u8]) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            // Only call create_dir_all when the parent is a non-empty path
            // (path.parent() returns Some("") for a bare filename).
            if !parent.as_os_str().is_empty() {
                self.create_dir_all(parent)?;
            }
        }
        self.write_file(path, content)
    }
}

/// Standard filesystem — delegates to `std::fs`.
pub struct Real;

impl Fs for Real {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn create_dir_all(&self, path: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn write_file(&self, path: &Path, content: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::File::create(path)?;
        file.write_all(content)?;
        Ok(())
    }

    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn read_dir(&self, path: &Path) -> std::io::Result<Vec<DirEntry>> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            entries.push(DirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir: file_type.is_dir(),
            });
        }
        Ok(entries)
    }

    fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        std::fs::remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> std::io::Result<()> {
        std::fs::remove_dir_all(path)
    }

    fn hard_link(&self, source: &Path, target: &Path) -> std::io::Result<()> {
        std::fs::hard_link(source, target)
    }

    fn copy_file(&self, source: &Path, target: &Path) -> std::io::Result<u64> {
        std::fs::copy(source, target)
    }

    fn symlink_dir(&self, source: &Path, target: &Path) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(source, target)
        }
        #[cfg(windows)]
        {
            junction::create(source, target)
        }
    }

    fn read_link(&self, path: &Path) -> std::io::Result<std::path::PathBuf> {
        std::fs::read_link(path)
    }

    fn is_symlink(&self, path: &Path) -> bool {
        #[cfg(unix)]
        {
            path.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink())
        }
        #[cfg(windows)]
        {
            if path.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
                return true;
            }
            junction::exists(path).unwrap_or(false)
        }
    }

    fn atomic_write(&self, path: &Path, content: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        use std::sync::atomic::{AtomicU64, Ordering};

        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;

        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_path = parent.join(format!(".aipm-tmp-{}-{seq}", std::process::id()));
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(content)?;
        file.sync_all()?;
        drop(file);

        #[cfg(unix)]
        {
            std::fs::rename(&tmp_path, path)
        }
        #[cfg(windows)]
        {
            use std::fs;

            // On Windows, std::fs::rename does not atomically replace an existing
            // destination. Use a backup-then-swap strategy so that a failed rename
            // does not permanently delete the original file.
            if path.exists() {
                let backup_path = parent.join(format!(".aipm-backup-{}-{seq}", std::process::id()));

                fs::rename(path, &backup_path)?;

                match fs::rename(&tmp_path, path) {
                    Ok(()) => {
                        if let Err(e) = fs::remove_file(&backup_path) {
                            tracing::debug!(
                                path = %backup_path.display(),
                                error = %e,
                                "could not remove atomic write backup file"
                            );
                        }
                        Ok(())
                    },
                    Err(e) => {
                        if let Err(restore_err) = fs::rename(&backup_path, path) {
                            tracing::warn!(
                                backup = %backup_path.display(),
                                target = %path.display(),
                                error = %restore_err,
                                "could not restore backup after failed atomic write"
                            );
                        }
                        Err(e)
                    },
                }
            } else {
                fs::rename(&tmp_path, path)
            }
        }
    }
}

// -----------------------------------------------------------------
// Read-or-default helpers (free functions so they remain dyn-compatible)
// -----------------------------------------------------------------

/// Reads and JSON-deserializes a file via `fs`, returning `T::default()` if
/// the file does not exist.
///
/// Returns an error if the file exists but cannot be read or contains invalid
/// JSON.
pub fn read_or_default<T>(fs: &dyn Fs, path: &Path) -> std::io::Result<T>
where
    T: Default + DeserializeOwned,
{
    match fs.read_to_string(path) {
        Ok(content) => serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e),
    }
}

/// Reads and TOML-deserializes a file via `fs`, returning `T::default()` if
/// the file does not exist.
///
/// Returns an error if the file exists but cannot be read or contains invalid
/// TOML.
pub fn read_toml_or_default<T>(fs: &dyn Fs, path: &Path) -> std::io::Result<T>
where
    T: Default + DeserializeOwned,
{
    match fs.read_to_string(path) {
        Ok(content) => toml::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_read_dir_lists_entries() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let dir = tmp.as_ref().map(tempfile::TempDir::path);
        let dir = dir.as_ref().copied();
        assert!(dir.is_some(), "tempdir path must be available");
        let dir = dir.unwrap_or(Path::new("."));

        assert!(std::fs::write(dir.join("file1.txt"), "hello").is_ok(), "write must succeed");
        assert!(std::fs::create_dir(dir.join("subdir")).is_ok(), "create_dir must succeed");

        let result = Real.read_dir(dir);
        assert!(result.is_ok());
        let mut entries = result.ok().unwrap_or_default();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries.first().map(|e| e.name.as_str()), Some("file1.txt"));
        assert_eq!(entries.first().map(|e| e.is_dir), Some(false));
        assert_eq!(entries.get(1).map(|e| e.name.as_str()), Some("subdir"));
        assert_eq!(entries.get(1).map(|e| e.is_dir), Some(true));
    }

    #[test]
    fn real_read_dir_empty_dir() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let dir = tmp.as_ref().map(tempfile::TempDir::path);
        let dir = dir.as_ref().copied();
        assert!(dir.is_some(), "tempdir path must be available");
        let dir = dir.unwrap_or(Path::new("."));

        let result = Real.read_dir(dir);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 0);
    }

    #[test]
    fn real_read_dir_nonexistent() {
        let result = Real.read_dir(Path::new("/nonexistent/path/that/does/not/exist"));
        assert!(result.is_err());
    }

    #[test]
    fn real_remove_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("to_remove.txt");
        std::fs::write(&file, "content").expect("write");
        assert!(file.exists());

        assert!(Real.remove_file(&file).is_ok());
        assert!(!file.exists());
    }

    #[test]
    fn real_remove_dir_all() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("to_remove");
        std::fs::create_dir_all(dir.join("nested")).expect("create");
        std::fs::write(dir.join("nested/file.txt"), "content").expect("write");

        assert!(Real.remove_dir_all(&dir).is_ok());
        assert!(!dir.exists());
    }

    #[test]
    fn real_hard_link() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = tmp.path().join("source.txt");
        std::fs::write(&source, "link me").expect("write");

        let target = tmp.path().join("linked.txt");
        assert!(Real.hard_link(&source, &target).is_ok());
        assert!(target.exists());
        assert_eq!(std::fs::read_to_string(&target).expect("read"), "link me");
    }

    #[test]
    fn real_copy_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = tmp.path().join("source.txt");
        std::fs::write(&source, "copy me").expect("write");

        let target = tmp.path().join("copied.txt");
        let result = Real.copy_file(&source, &target);
        assert!(result.is_ok());
        assert_eq!(std::fs::read_to_string(&target).expect("read"), "copy me");
    }

    #[test]
    fn real_symlink_dir_and_read_link() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = tmp.path().join("source_dir");
        std::fs::create_dir_all(&source).expect("create");

        let target = tmp.path().join("link_dir");
        assert!(Real.symlink_dir(&source, &target).is_ok());
        assert!(Real.is_symlink(&target));

        let link_target = Real.read_link(&target);
        assert!(link_target.is_ok());
        assert_eq!(link_target.expect("read_link"), source);
    }

    #[test]
    fn real_is_symlink_returns_false_for_regular() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("regular");
        std::fs::create_dir_all(&dir).expect("create");
        assert!(!Real.is_symlink(&dir));
    }

    #[test]
    fn real_atomic_write() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("atomic.txt");

        assert!(Real.atomic_write(&file, b"atomic content").is_ok());
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "atomic content");
    }

    #[test]
    fn real_is_file_returns_true_for_regular_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("regular.txt");
        std::fs::write(&file, "content").expect("write");
        assert!(Real.is_file(&file));
    }

    #[test]
    fn real_is_file_returns_false_for_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(!Real.is_file(tmp.path()));
    }

    #[test]
    fn real_is_file_returns_false_for_symlink_to_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target_dir = tmp.path().join("target_dir");
        std::fs::create_dir_all(&target_dir).expect("create dir");
        let link = tmp.path().join("link_to_dir");
        std::os::unix::fs::symlink(&target_dir, &link).expect("symlink");
        assert!(!Real.is_file(&link));
    }

    #[test]
    fn real_is_file_returns_true_for_symlink_to_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target_file = tmp.path().join("target.txt");
        std::fs::write(&target_file, "content").expect("write");
        let link = tmp.path().join("link_to_file");
        std::os::unix::fs::symlink(&target_file, &link).expect("symlink");
        assert!(Real.is_file(&link));
    }

    #[test]
    fn real_is_file_returns_false_for_nonexistent() {
        assert!(!Real.is_file(Path::new("/nonexistent/path/that/does/not/exist")));
    }

    #[test]
    fn write_file_with_parents_creates_nested_dirs_and_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let nested = tmp.path().join("a").join("b").join("c").join("output.txt");

        assert!(Real.write_file_with_parents(&nested, b"hello nested").is_ok());
        assert!(nested.exists());
        assert_eq!(std::fs::read_to_string(&nested).expect("read"), "hello nested");
    }

    #[test]
    fn write_file_with_parents_bare_filename_no_parent_create() {
        // A bare filename has no meaningful parent directory — write_file_with_parents
        // must not call create_dir_all with an empty path (which would be a no-op
        // on most OSes but is semantically wrong). We test via a tmp dir cd-less
        // approach: just check the call succeeds when parent is empty.
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("just_a_file.txt");

        // Parent exists (tmp dir), so create_dir_all is a no-op; file is created.
        assert!(Real.write_file_with_parents(&file, b"bare").is_ok());
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "bare");
    }

    #[test]
    fn write_file_with_parents_overwrites_existing_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("sub").join("overwrite.txt");

        assert!(Real.write_file_with_parents(&file, b"first").is_ok());
        assert!(Real.write_file_with_parents(&file, b"second").is_ok());
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "second");
    }

    #[test]
    fn write_file_with_parents_single_level_parent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("one_level").join("file.txt");

        assert!(Real.write_file_with_parents(&file, b"content").is_ok());
        assert!(file.exists());
    }

    #[test]
    fn write_file_with_parents_empty_parent_skips_create_dir_all() {
        // When path has a bare filename (parent = Some("")), create_dir_all must
        // NOT be called. Use a CWD-change guarded by a mutex so parallel tests
        // don't interfere, and call through Real so LLVM coverage registers it.
        static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = CWD_LOCK.lock().expect("CWD_LOCK");
        let tmp = tempfile::tempdir().expect("tempdir");
        let orig = std::env::current_dir().expect("current_dir");
        std::env::set_current_dir(tmp.path()).expect("set_current_dir");
        let result = Real.write_file_with_parents(Path::new("bare_file.txt"), b"data");
        std::env::set_current_dir(&orig).expect("restore current_dir");
        assert!(result.is_ok());
        assert!(tmp.path().join("bare_file.txt").exists());
    }

    // ---- read_or_default tests (JSON) ----

    #[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct TestJson {
        value: u32,
    }

    #[test]
    fn read_or_default_returns_default_when_file_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("missing.json");
        let result: std::io::Result<TestJson> = read_or_default(&Real, &path);
        assert!(result.is_ok());
        assert_eq!(result.expect("ok"), TestJson::default());
    }

    #[test]
    fn read_or_default_deserializes_valid_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("data.json");
        std::fs::write(&path, r#"{"value": 42}"#).expect("write");
        let result: std::io::Result<TestJson> = read_or_default(&Real, &path);
        assert!(result.is_ok());
        assert_eq!(result.expect("ok"), TestJson { value: 42 });
    }

    #[test]
    fn read_or_default_errors_on_invalid_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("bad.json");
        std::fs::write(&path, "not json at all :::").expect("write");
        let result: std::io::Result<TestJson> = read_or_default(&Real, &path);
        assert!(result.is_err());
        assert_eq!(result.err().map(|e| e.kind()), Some(std::io::ErrorKind::InvalidData));
    }

    // ---- read_toml_or_default tests ----

    #[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct TestToml {
        count: u32,
    }

    #[test]
    fn read_toml_or_default_returns_default_when_file_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("missing.toml");
        let result: std::io::Result<TestToml> = read_toml_or_default(&Real, &path);
        assert!(result.is_ok());
        assert_eq!(result.expect("ok"), TestToml::default());
    }

    #[test]
    fn read_toml_or_default_deserializes_valid_toml() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("data.toml");
        std::fs::write(&path, "count = 7\n").expect("write");
        let result: std::io::Result<TestToml> = read_toml_or_default(&Real, &path);
        assert!(result.is_ok());
        assert_eq!(result.expect("ok"), TestToml { count: 7 });
    }

    #[test]
    fn read_toml_or_default_errors_on_invalid_toml() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("bad.toml");
        std::fs::write(&path, "[[not valid toml :::").expect("write");
        let result: std::io::Result<TestToml> = read_toml_or_default(&Real, &path);
        assert!(result.is_err());
        assert_eq!(result.err().map(|e| e.kind()), Some(std::io::ErrorKind::InvalidData));
    }

    #[test]
    fn read_or_default_returns_err_on_non_not_found_io_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // Reading a directory path as a file returns a non-NotFound I/O error (EISDIR).
        let path = tmp.path().join("i_am_a_dir");
        std::fs::create_dir_all(&path).expect("create dir");
        let result: std::io::Result<TestJson> = read_or_default(&Real, &path);
        assert!(result.is_err());
        assert_ne!(result.err().map(|e| e.kind()), Some(std::io::ErrorKind::NotFound));
    }

    #[test]
    fn read_toml_or_default_returns_err_on_non_not_found_io_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // Reading a directory path as a file returns a non-NotFound I/O error (EISDIR).
        let path = tmp.path().join("i_am_a_dir");
        std::fs::create_dir_all(&path).expect("create dir");
        let result: std::io::Result<TestToml> = read_toml_or_default(&Real, &path);
        assert!(result.is_err());
        assert_ne!(result.err().map(|e| e.kind()), Some(std::io::ErrorKind::NotFound));
    }

    #[test]
    fn real_read_dir_distinguishes_files_and_dirs() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let dir = tmp.as_ref().map(tempfile::TempDir::path);
        let dir = dir.as_ref().copied();
        assert!(dir.is_some(), "tempdir path must be available");
        let dir = dir.unwrap_or(Path::new("."));

        assert!(std::fs::write(dir.join("a_file.txt"), "content").is_ok());
        assert!(std::fs::write(dir.join("b_file.rs"), "code").is_ok());
        assert!(std::fs::create_dir(dir.join("c_dir")).is_ok());
        assert!(std::fs::create_dir(dir.join("d_dir")).is_ok());

        let result = Real.read_dir(dir);
        assert!(result.is_ok());
        let mut entries = result.ok().unwrap_or_default();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(entries.len(), 4);

        let files: Vec<_> = entries.iter().filter(|e| !e.is_dir).collect();
        let dirs: Vec<_> = entries.iter().filter(|e| e.is_dir).collect();
        assert_eq!(files.len(), 2);
        assert_eq!(dirs.len(), 2);
    }
}
