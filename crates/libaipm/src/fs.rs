//! Filesystem abstraction for testability.
//!
//! Production code uses [`Real`] which delegates to `std::fs`.
//! Tests can inject a mock implementation to simulate I/O errors
//! without touching the real filesystem.

use std::path::Path;

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
}
