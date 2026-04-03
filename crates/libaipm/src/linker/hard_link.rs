//! Hard-link assembly from the content store to `.aipm/links/{pkg}/`.
//!
//! Given a map of relative paths to content hashes (produced by
//! [`Store::store_package`](crate::store::Store::store_package)), this module
//! recreates the package directory tree using hard-links from the store.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::store;

use super::error::Error;

/// Assemble a package directory at `target_dir` by hard-linking each file
/// from the content store.
///
/// `file_hashes` is a map of `relative_path -> content_hash` as returned by
/// [`Store::store_package`](crate::store::Store::store_package).
///
/// The directory structure is recreated under `target_dir`. If `target_dir`
/// already exists, it is removed first to ensure a clean state.
///
/// # Errors
///
/// Returns [`Error::Io`] if directory creation or hard-link/copy fails.
/// Returns [`Error::Io`] wrapping store errors if the hash is not found.
pub fn assemble(
    store: &store::Store,
    file_hashes: &BTreeMap<PathBuf, String>,
    target_dir: &Path,
) -> Result<(), Error> {
    // Clean existing assembly directory for a fresh state.
    if target_dir.exists() {
        std::fs::remove_dir_all(target_dir)
            .map_err(|e| Error::Io { path: target_dir.to_path_buf(), source: e })?;
    }

    std::fs::create_dir_all(target_dir)
        .map_err(|e| Error::Io { path: target_dir.to_path_buf(), source: e })?;

    for (rel_path, hash) in file_hashes {
        let file_target = target_dir.join(rel_path);

        // Ensure parent directories exist.
        if let Some(parent) = file_target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
        }

        // Use the store's link_to which handles cross-volume fallback.
        store.link_to(hash, &file_target).map_err(|e| Error::Io {
            path: file_target,
            source: std::io::Error::other(e.to_string()),
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store_and_package() -> (tempfile::TempDir, store::Store, BTreeMap<PathBuf, String>) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = store::Store::new(tmp.path().join("store"));

        // Create and store some files.
        let hash1 = store.store_file(b"file one content").expect("store file 1");
        let hash2 = store.store_file(b"file two content").expect("store file 2");

        let mut file_hashes = BTreeMap::new();
        file_hashes.insert(PathBuf::from("aipm.toml"), hash1);
        file_hashes.insert(PathBuf::from("skills/review.md"), hash2);

        (tmp, store, file_hashes)
    }

    #[test]
    fn assemble_creates_directory_tree() {
        let (tmp, store, file_hashes) = make_store_and_package();
        let target = tmp.path().join("links").join("my-pkg");

        let result = assemble(&store, &file_hashes, &target);
        assert!(result.is_ok(), "assemble should succeed: {result:?}");

        assert!(target.join("aipm.toml").exists());
        assert!(target.join("skills").join("review.md").exists());
    }

    #[test]
    fn assemble_file_content_matches() {
        let (tmp, store, file_hashes) = make_store_and_package();
        let target = tmp.path().join("links").join("my-pkg");

        assert!(assemble(&store, &file_hashes, &target).is_ok());

        let content1 = std::fs::read(target.join("aipm.toml")).expect("read file 1");
        assert_eq!(content1, b"file one content");

        let content2 = std::fs::read(target.join("skills").join("review.md")).expect("read file 2");
        assert_eq!(content2, b"file two content");
    }

    #[test]
    fn assemble_cleans_existing_directory() {
        let (tmp, store, file_hashes) = make_store_and_package();
        let target = tmp.path().join("links").join("my-pkg");

        // Pre-create directory with stale content.
        std::fs::create_dir_all(&target).expect("create target");
        std::fs::write(target.join("stale.txt"), "old").expect("write stale");

        assert!(assemble(&store, &file_hashes, &target).is_ok());

        // Stale file should be gone.
        assert!(!target.join("stale.txt").exists());
        // New files should be present.
        assert!(target.join("aipm.toml").exists());
    }

    #[test]
    fn assemble_empty_package() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = store::Store::new(tmp.path().join("store"));
        let target = tmp.path().join("links").join("empty-pkg");

        let result = assemble(&store, &BTreeMap::new(), &target);
        assert!(result.is_ok());
        assert!(target.exists());
        assert!(target.is_dir());
    }

    #[test]
    fn assemble_deeply_nested_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = store::Store::new(tmp.path().join("store"));
        let hash = store.store_file(b"deep content").expect("store file");

        let mut file_hashes = BTreeMap::new();
        file_hashes.insert(PathBuf::from("a/b/c/d/file.txt"), hash);

        let target = tmp.path().join("links").join("deep-pkg");
        let result = assemble(&store, &file_hashes, &target);
        assert!(result.is_ok());
        assert!(target.join("a/b/c/d/file.txt").exists());
    }

    #[test]
    fn assemble_missing_hash_returns_error() {
        // A valid-format hash that was never stored — link_to returns NotFound,
        // covering the map_err closure on the store.link_to call (lines 51-54).
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = store::Store::new(tmp.path().join("store"));

        let ghost_hash = "a".repeat(128); // valid format, but never stored
        let mut file_hashes = BTreeMap::new();
        file_hashes.insert(PathBuf::from("ghost.txt"), ghost_hash);

        let target = tmp.path().join("links").join("ghost-pkg");
        let result = assemble(&store, &file_hashes, &target);
        assert!(result.is_err(), "assemble should fail when hash is not in the store");
    }
}
