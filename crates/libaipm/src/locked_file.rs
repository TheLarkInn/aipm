//! OS-level exclusive file locking.
//!
//! [`LockedFile`] acquires an exclusive lock on the data file itself — no
//! separate `.lock` sidecar is needed.  The lock is released when the value
//! is dropped (including on process crash, since the OS reclaims the lock).
//!
//! This module is used by the download cache index and the global installed
//! plugin registry to perform safe read-modify-write operations.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::Path;

/// A file handle holding an OS-level exclusive lock.
///
/// All reads and writes go through this handle so the underlying data stays
/// consistent across concurrent processes.
pub struct LockedFile {
    file: File,
}

impl LockedFile {
    /// Open (or create) a file and acquire a blocking exclusive lock.
    ///
    /// Parent directories are created automatically if they do not exist.
    pub fn open(path: &Path) -> Result<Self, Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| Error::Io { path: parent.to_path_buf(), source })?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|source| Error::Io { path: path.to_path_buf(), source })?;

        #[cfg(not(target_arch = "wasm32"))]
        {
            use fs2::FileExt;
            file.lock_exclusive()
                .map_err(|source| Error::Io { path: path.to_path_buf(), source })?;
        }

        Ok(Self { file })
    }

    /// Read the entire file content as a UTF-8 string.
    pub fn read_content(&mut self) -> Result<String, Error> {
        self.file.seek(std::io::SeekFrom::Start(0)).map_err(|source| Error::Seek { source })?;
        let mut content = String::new();
        self.file.read_to_string(&mut content).map_err(|source| Error::Read { source })?;
        Ok(content)
    }

    /// Overwrite the file with the given content.
    ///
    /// Truncates the file to zero length before writing so previous (possibly
    /// longer) content does not leak through.
    pub fn write_content(&mut self, content: &str) -> Result<(), Error> {
        self.file.set_len(0).map_err(|source| Error::Write { source })?;
        self.file.seek(std::io::SeekFrom::Start(0)).map_err(|source| Error::Seek { source })?;
        self.file.write_all(content.as_bytes()).map_err(|source| Error::Write { source })?;
        self.file.flush().map_err(|source| Error::Write { source })?;
        Ok(())
    }
}

/// Errors that can occur during locked file operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An I/O error involving a specific path (open, create dir, lock).
    #[error("Locked file I/O error at {}: {source}", path.display())]
    Io { path: std::path::PathBuf, source: std::io::Error },
    /// A seek error.
    #[error("Failed to seek locked file: {source}")]
    Seek { source: std::io::Error },
    /// A read error.
    #[error("Failed to read locked file: {source}")]
    Read { source: std::io::Error },
    /// A write error.
    #[error("Failed to write locked file: {source}")]
    Write { source: std::io::Error },
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn open_creates_parent_directories() -> TestResult {
        let temp = tempfile::tempdir()?;
        let nested = temp.path().join("a").join("b").join("c").join("data.json");
        LockedFile::open(&nested)?;
        assert!(nested.exists());
        Ok(())
    }

    #[test]
    fn read_write_roundtrip() -> TestResult {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("test.json");

        let mut locked = LockedFile::open(&path)?;
        locked.write_content("{\"hello\": \"world\"}")?;
        drop(locked);

        let mut locked2 = LockedFile::open(&path)?;
        assert_eq!(locked2.read_content()?, "{\"hello\": \"world\"}");
        Ok(())
    }

    #[test]
    fn write_truncates_previous_content() -> TestResult {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("test.json");

        let mut locked = LockedFile::open(&path)?;
        locked.write_content("a]very long string with lots of content")?;
        locked.write_content("short")?;
        drop(locked);

        let mut locked2 = LockedFile::open(&path)?;
        assert_eq!(locked2.read_content()?, "short");
        Ok(())
    }

    #[test]
    fn lock_released_on_drop() -> TestResult {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("test.json");

        drop(LockedFile::open(&path)?);
        LockedFile::open(&path)?;
        Ok(())
    }

    #[test]
    fn read_empty_file_returns_empty_string() -> TestResult {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("empty.json");

        let mut locked = LockedFile::open(&path)?;
        assert!(locked.read_content()?.is_empty());
        Ok(())
    }

    #[test]
    fn open_file_in_temp_directory() -> TestResult {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("bare-file.json");
        LockedFile::open(&path)?;
        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn open_fails_when_parent_creation_errors() -> TestResult {
        // `create_dir_all` cannot create a directory beneath a regular file.
        let temp = tempfile::tempdir()?;
        let blocker = temp.path().join("blocker");
        std::fs::write(&blocker, b"not a dir")?;

        let nested = blocker.join("child").join("data.json");
        assert!(matches!(LockedFile::open(&nested), Err(Error::Io { .. })));
        Ok(())
    }

    #[test]
    fn open_path_with_no_parent_skips_mkdir_and_fails() {
        // `Path::new("/").parent()` is None; opening a directory as a file then fails.
        assert!(LockedFile::open(Path::new("/")).is_err());
    }

    #[test]
    fn error_variants_display() {
        let mk = || std::io::Error::other("boom");
        let io = Error::Io { path: "p".into(), source: mk() };
        assert!(io.to_string().contains("boom"));
        assert!(Error::Seek { source: mk() }.to_string().contains("seek"));
        assert!(Error::Read { source: mk() }.to_string().contains("read"));
        assert!(Error::Write { source: mk() }.to_string().contains("write"));
    }
}
