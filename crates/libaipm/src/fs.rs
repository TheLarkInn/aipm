//! Filesystem abstraction for testability.
//!
//! Production code uses [`Real`] which delegates to `std::fs`.
//! Tests can inject a mock implementation to simulate I/O errors
//! without touching the real filesystem.

use std::path::Path;

/// A directory entry returned by `Fs::read_dir`.
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// The file or directory name (not the full path).
    pub name: String,
    /// Whether this entry is a directory.
    pub is_dir: bool,
}

/// Abstraction over filesystem operations used by init and `workspace_init`.
pub trait Fs {
    /// Check if a path exists.
    fn exists(&self, path: &Path) -> bool;

    /// Recursively create directories.
    fn create_dir_all(&self, path: &Path) -> std::io::Result<()>;

    /// Create (or truncate) a file and write content.
    fn write_file(&self, path: &Path, content: &[u8]) -> std::io::Result<()>;

    /// Read a file's entire contents as a string.
    fn read_to_string(&self, path: &Path) -> std::io::Result<String>;

    /// List entries in a directory. Returns file names (not full paths).
    fn read_dir(&self, path: &Path) -> std::io::Result<Vec<DirEntry>>;
}

/// Standard filesystem — delegates to `std::fs`.
pub struct Real;

impl Fs for Real {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
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
