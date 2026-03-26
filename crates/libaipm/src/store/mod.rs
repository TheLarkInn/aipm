//! Content-addressable store for AIPM packages.
//!
//! Files are stored by their SHA-512 hash in a 2-character prefix
//! sharded directory layout: `~/.aipm/store/{prefix}/{rest}`.
//!
//! # Concurrency
//!
//! The store itself does not acquire locks internally. Callers performing
//! concurrent writes should hold a [`Lock`] from [`Store::lock()`] for the
//! duration of the operation to ensure mutual exclusion.
//!
//! This module provides:
//! - [`hash`] — SHA-512 hashing utilities
//! - [`layout`] — directory path calculation from hashes
//! - [`error`] — error types for store operations
//! - [`Store`] — the main store struct for storing and retrieving content

pub mod error;
pub mod hash;
pub mod layout;

use std::path::{Path, PathBuf};

use error::Error;

/// An exclusive advisory lock on the content store.
///
/// The lock is released when this value is dropped.
pub struct Lock {
    /// Holding the file keeps the lock active; dropping releases it.
    _file: std::fs::File,
}

/// EXDEV error code: 18 on Linux/macOS, 17 on Windows.
fn is_cross_device(err: &std::io::Error) -> bool {
    // Unix: EXDEV = 18, Windows: ERROR_NOT_SAME_DEVICE = 17
    #[cfg(unix)]
    {
        err.raw_os_error() == Some(18)
    }
    #[cfg(windows)]
    {
        err.raw_os_error() == Some(17)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = err;
        false
    }
}

/// A content-addressable file store.
///
/// Files are identified by their SHA-512 hash and stored under a
/// 2-character prefix sharding scheme to avoid overly large directories.
pub struct Store {
    /// Root directory of the store (e.g. `~/.aipm/store/`).
    store_path: PathBuf,
}

impl Store {
    /// Create a new `Store` rooted at the given path.
    pub const fn new(store_path: PathBuf) -> Self {
        Self { store_path }
    }

    /// Return the store root path.
    pub fn path(&self) -> &Path {
        &self.store_path
    }

    /// Acquire an exclusive advisory lock on the store.
    ///
    /// Returns a [`Lock`] guard that releases the lock on drop.
    /// The lock file is created at `{store_path}/.lock`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the store directory cannot be created
    /// or the lock file cannot be opened/locked.
    pub fn lock(&self) -> Result<Lock, Error> {
        use fs2::FileExt;

        std::fs::create_dir_all(&self.store_path)
            .map_err(|source| Error::Io { path: self.store_path.clone(), source })?;

        let lock_path = self.store_path.join(".lock");
        let file = std::fs::File::create(&lock_path)
            .map_err(|source| Error::Io { path: lock_path.clone(), source })?;

        file.lock_exclusive().map_err(|source| Error::Io { path: lock_path, source })?;

        Ok(Lock { _file: file })
    }

    /// Store a file by its content hash. Returns the hex-encoded SHA-512 hash.
    ///
    /// If a file with the same hash already exists, this is a no-op
    /// (content-addressable writes are idempotent).
    ///
    /// Callers performing concurrent writes should hold a [`Lock`] from
    /// [`Store::lock()`] for the duration of the operation.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if directory creation or file writing fails.
    pub fn store_file(&self, content: &[u8]) -> Result<String, Error> {
        let hex = hash::sha512_hex(content);
        let target = layout::hash_to_path(&self.store_path, &hex)?;

        // Idempotent: skip if already stored
        if target.exists() {
            return Ok(hex);
        }

        // Ensure the prefix directory exists
        let prefix_dir = layout::hash_prefix_dir(&self.store_path, &hex)?;
        std::fs::create_dir_all(&prefix_dir)
            .map_err(|source| Error::Io { path: prefix_dir, source })?;

        // Write the content
        std::fs::write(&target, content).map_err(|source| Error::Io { path: target, source })?;

        Ok(hex)
    }

    /// Retrieve the filesystem path for a stored file by its hash.
    ///
    /// Returns `None` if the content is not in the store.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidHash`] if the hash format is invalid.
    pub fn get_path(&self, hash: &str) -> Result<Option<PathBuf>, Error> {
        let path = layout::hash_to_path(&self.store_path, hash)?;
        if path.exists() {
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }

    /// Check if content exists in the store by its hash.
    ///
    /// Returns `false` for invalid hashes instead of erroring.
    pub fn has_content(&self, hash: &str) -> bool {
        layout::hash_to_path(&self.store_path, hash).map(|p| p.exists()).unwrap_or(false)
    }

    /// Hard-link a stored file to a target path.
    ///
    /// Falls back to copy with a `tracing::warn!` if hard-link fails
    /// due to a cross-volume (EXDEV) error.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] if the hash is not in the store.
    /// Returns [`Error::Io`] if the link/copy operation fails for a
    /// reason other than cross-volume.
    pub fn link_to(&self, hash: &str, target: &Path) -> Result<(), Error> {
        let source = layout::hash_to_path(&self.store_path, hash)?;
        if !source.exists() {
            return Err(Error::NotFound { hash: hash.to_string() });
        }

        // Ensure target parent directory exists
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| Error::Io { path: parent.to_path_buf(), source })?;
        }

        match std::fs::hard_link(&source, target) {
            Ok(()) => Ok(()),
            Err(e) if is_cross_device(&e) => {
                tracing::warn!(
                    source = %source.display(),
                    target = %target.display(),
                    "hard-link failed (cross-volume), falling back to copy"
                );
                std::fs::copy(&source, target)
                    .map_err(|source| Error::Io { path: target.to_path_buf(), source })?;
                Ok(())
            },
            Err(e) => Err(Error::Io { path: target.to_path_buf(), source: e }),
        }
    }

    /// Store all files from an extracted package directory.
    ///
    /// Walks the directory recursively, hashes each file individually,
    /// and stores it in the content-addressable store.
    ///
    /// Returns a map of relative paths (from `extracted_dir`) to their
    /// content hashes.
    ///
    /// Callers performing concurrent writes should hold a [`Lock`] from
    /// [`Store::lock()`] for the duration of the operation.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if directory traversal, reading, or storing fails.
    pub fn store_package(
        &self,
        extracted_dir: &Path,
    ) -> Result<std::collections::BTreeMap<PathBuf, String>, Error> {
        let mut files = Vec::new();
        Self::collect_files(extracted_dir, extracted_dir, &mut files)?;

        let mut file_hashes = std::collections::BTreeMap::new();
        for (rel_path, content) in &files {
            let hash = self.store_file(content)?;
            file_hashes.insert(rel_path.clone(), hash);
        }

        Ok(file_hashes)
    }

    /// Recursively collect all files in a directory as `(relative_path, content)` pairs.
    fn collect_files(
        base: &Path,
        current: &Path,
        out: &mut Vec<(PathBuf, Vec<u8>)>,
    ) -> Result<(), Error> {
        let entries = std::fs::read_dir(current)
            .map_err(|source| Error::Io { path: current.to_path_buf(), source })?;

        for entry in entries {
            let entry =
                entry.map_err(|source| Error::Io { path: current.to_path_buf(), source })?;
            let path = entry.path();
            let file_type =
                entry.file_type().map_err(|source| Error::Io { path: path.clone(), source })?;

            if file_type.is_dir() {
                Self::collect_files(base, &path, out)?;
            } else if file_type.is_file() {
                let content = std::fs::read(&path)
                    .map_err(|source| Error::Io { path: path.clone(), source })?;
                let rel_path =
                    path.strip_prefix(base).map_or_else(|_| path.clone(), Path::to_path_buf);
                out.push((rel_path, content));
            }
            // Skip symlinks and other special files
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> (tempfile::TempDir, Store) {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path().to_path_buf());
        (tmp, store)
    }

    #[test]
    fn store_and_retrieve_file() {
        let (_tmp, store) = make_store();
        let content = b"hello world";

        let hash = store.store_file(content).unwrap();
        assert_eq!(hash.len(), 128);
        assert!(store.has_content(&hash));

        let path = store.get_path(&hash).unwrap().unwrap();
        let read_back = std::fs::read(path).unwrap();
        assert_eq!(read_back, content);
    }

    #[test]
    fn store_file_is_idempotent() {
        let (_tmp, store) = make_store();
        let content = b"idempotent content";

        let hash1 = store.store_file(content).unwrap();
        let hash2 = store.store_file(content).unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn has_content_returns_false_for_missing() {
        let (_tmp, store) = make_store();
        let fake_hash = "a".repeat(128);
        assert!(!store.has_content(&fake_hash));
    }

    #[test]
    fn has_content_returns_false_for_invalid_hash() {
        let (_tmp, store) = make_store();
        assert!(!store.has_content("not-a-hash"));
    }

    #[test]
    fn get_path_returns_none_for_missing() {
        let (_tmp, store) = make_store();
        let fake_hash = "b".repeat(128);
        let result = store.get_path(&fake_hash).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn get_path_errors_on_invalid_hash() {
        let (_tmp, store) = make_store();
        assert!(store.get_path("short").is_err());
    }

    #[test]
    fn store_creates_prefix_directories() {
        let (_tmp, store) = make_store();
        let hash = store.store_file(b"prefix test").unwrap();

        let prefix = &hash[..2];
        let prefix_dir = store.path().join(prefix);
        assert!(prefix_dir.exists());
        assert!(prefix_dir.is_dir());
    }

    #[test]
    fn different_content_produces_different_entries() {
        let (_tmp, store) = make_store();
        let hash_a = store.store_file(b"content A").unwrap();
        let hash_b = store.store_file(b"content B").unwrap();
        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn empty_content_can_be_stored() {
        let (_tmp, store) = make_store();
        let hash = store.store_file(b"").unwrap();
        assert!(store.has_content(&hash));
    }

    #[test]
    fn link_to_creates_hard_link() {
        let (_tmp, store) = make_store();
        let content = b"linkable content";
        let hash = store.store_file(content).unwrap();

        let target_dir = tempfile::tempdir().unwrap();
        let target = target_dir.path().join("linked_file");

        store.link_to(&hash, &target).unwrap();

        assert!(target.exists());
        let read_back = std::fs::read(&target).unwrap();
        assert_eq!(read_back, content);
    }

    #[test]
    fn link_to_creates_parent_directories() {
        let (_tmp, store) = make_store();
        let content = b"nested link target";
        let hash = store.store_file(content).unwrap();

        let target_dir = tempfile::tempdir().unwrap();
        let target = target_dir.path().join("deep").join("nested").join("file");

        store.link_to(&hash, &target).unwrap();
        assert!(target.exists());
    }

    #[test]
    fn link_to_errors_on_missing_hash() {
        let (_tmp, store) = make_store();
        let fake_hash = "c".repeat(128);
        let target_dir = tempfile::tempdir().unwrap();
        let target = target_dir.path().join("missing");

        let result = store.link_to(&fake_hash, &target);
        assert!(result.is_err());
    }

    #[test]
    fn link_to_errors_on_invalid_hash() {
        let (_tmp, store) = make_store();
        let target_dir = tempfile::tempdir().unwrap();
        let target = target_dir.path().join("bad");

        let result = store.link_to("invalid", &target);
        assert!(result.is_err());
    }

    #[test]
    fn store_package_walks_directory() {
        let (_tmp, store) = make_store();
        let pkg_dir = tempfile::tempdir().unwrap();

        // Create a package directory structure
        std::fs::write(pkg_dir.path().join("aipm.toml"), b"[package]\nname = \"test\"").unwrap();
        std::fs::create_dir_all(pkg_dir.path().join("skills")).unwrap();
        std::fs::write(pkg_dir.path().join("skills").join("review.md"), b"# Review Skill").unwrap();

        let result = store.store_package(pkg_dir.path()).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.contains_key(Path::new("aipm.toml")));
        assert!(result.contains_key(&PathBuf::from("skills/review.md")));

        // All hashes should be in the store
        for hash in result.values() {
            assert!(store.has_content(hash));
        }
    }

    #[test]
    fn store_package_empty_directory() {
        let (_tmp, store) = make_store();
        let pkg_dir = tempfile::tempdir().unwrap();

        let result = store.store_package(pkg_dir.path()).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn store_package_deduplicates_identical_files() {
        let (_tmp, store) = make_store();
        let pkg_dir = tempfile::tempdir().unwrap();

        // Two files with identical content
        let content = b"same content in both";
        std::fs::write(pkg_dir.path().join("file_a.txt"), content).unwrap();
        std::fs::write(pkg_dir.path().join("file_b.txt"), content).unwrap();

        let result = store.store_package(pkg_dir.path()).unwrap();
        assert_eq!(result.len(), 2);

        // Both should map to the same hash
        let hashes: Vec<&String> = result.values().collect();
        assert_eq!(hashes[0], hashes[1]);
    }

    #[test]
    fn lock_creates_lock_file() {
        let (_tmp, store) = make_store();
        let _guard = store.lock().unwrap();

        let lock_path = store.path().join(".lock");
        assert!(lock_path.exists());
    }

    #[test]
    fn lock_released_on_drop() {
        let (_tmp, store) = make_store();

        // Acquire and release
        {
            let _guard = store.lock().unwrap();
        }

        // Should be able to lock again
        let _guard = store.lock().unwrap();
    }

    #[test]
    fn lock_creates_store_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let store_path = tmp.path().join("nested").join("store");
        let store = Store::new(store_path.clone());

        let _guard = store.lock().unwrap();
        assert!(store_path.exists());
    }

    #[test]
    fn get_path_returns_some_for_existing() {
        let (_tmp, store) = make_store();
        let hash = store.store_file(b"get path test").unwrap();

        let result = store.get_path(&hash).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().exists());
    }

    #[test]
    fn link_to_target_with_no_parent_still_works() {
        // When target path has no parent (e.g. just a filename in current dir),
        // create_dir_all(parent) is skipped via the if let Some branch
        let (_tmp, store) = make_store();
        let content = b"no-parent test";
        let hash = store.store_file(content).unwrap();

        // Use a target in the store's own directory (which already exists)
        let target = store.path().join("output_file");
        assert!(store.link_to(&hash, &target).is_ok());
        assert!(target.exists());
    }

    #[test]
    fn store_package_with_nested_dirs() {
        let (_tmp, store) = make_store();
        let pkg_dir = tempfile::tempdir().unwrap();

        // Create nested directory structure
        std::fs::create_dir_all(pkg_dir.path().join("a/b")).unwrap();
        std::fs::write(pkg_dir.path().join("a/b/file.txt"), b"deep file").unwrap();
        std::fs::write(pkg_dir.path().join("top.txt"), b"top file").unwrap();

        let result = store.store_package(pkg_dir.path()).unwrap();
        assert_eq!(result.len(), 2);

        for hash in result.values() {
            assert!(store.has_content(hash));
        }
    }

    #[test]
    fn is_cross_device_false_for_regular_error() {
        // A regular permission denied error is not a cross-device error
        let err = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert!(!is_cross_device(&err));
    }

    #[test]
    fn link_to_errors_on_existing_target_same_dir() {
        let (_tmp, store) = make_store();
        let content = b"link target";
        let hash = store.store_file(content).unwrap();

        let target_dir = tempfile::tempdir().unwrap();
        let target = target_dir.path().join("file");

        // Create the target once
        store.link_to(&hash, &target).unwrap();
        // Hard-linking to an already existing path should fail (AlreadyExists)
        let result = store.link_to(&hash, &target);
        assert!(result.is_err());
    }

    #[test]
    fn link_to_with_no_parent_target() {
        let (_tmp, store) = make_store();
        let content = b"no parent target test";
        let hash = store.store_file(content).unwrap();

        // Path::new("").parent() returns None, covering the None branch at line 158
        let result = store.link_to(&hash, Path::new(""));
        assert!(result.is_err());
    }

    #[test]
    fn store_package_skips_symlinks() {
        let (_tmp, store) = make_store();
        let pkg_dir = tempfile::tempdir().unwrap();

        // Write a regular file
        std::fs::write(pkg_dir.path().join("real.txt"), b"real file").unwrap();

        // Create a symlink to a nonexistent target
        std::os::unix::fs::symlink("/nonexistent", pkg_dir.path().join("link.txt")).unwrap();

        let result = store.store_package(pkg_dir.path()).unwrap();
        assert_eq!(result.len(), 1);
    }
}
