//! Three-tier linking pipeline: store → `.aipm/links/` → `plugins_dir/`.
//!
//! 1. Hard-link files from the content store into `.aipm/links/{pkg}/`
//! 2. Create a directory symlink/junction from `{plugins_dir}/{pkg}/` to `.aipm/links/{pkg}/`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::store;

use super::{directory_link, error::Error, hard_link};

/// Link a package through the full three-tier pipeline.
///
/// - `store`: the content-addressable store
/// - `file_hashes`: relative path → content hash (from `store.store_package()`)
/// - `pkg_name`: the package name (used for directory naming)
/// - `links_dir`: the `.aipm/links/` directory
/// - `plugins_dir`: the `claude-plugins/` (or `.ai/`) directory for discovery
///
/// # Errors
///
/// Returns [`Error`] if any step in the pipeline fails.
pub fn link_package(
    store: &store::Store,
    file_hashes: &BTreeMap<PathBuf, String>,
    pkg_name: &str,
    links_dir: &Path,
    plugins_dir: &Path,
) -> Result<(), Error> {
    let assembled_dir = links_dir.join(pkg_name);
    let plugin_link = plugins_dir.join(pkg_name);

    tracing::info!(package = pkg_name, files = file_hashes.len(), "assembling package from store");

    // Step 1: Assemble package via hard-links from store.
    hard_link::assemble(store, file_hashes, &assembled_dir)?;

    tracing::info!(
        package = pkg_name,
        source = %assembled_dir.display(),
        target = %plugin_link.display(),
        "creating directory link"
    );

    // Step 2: Create directory link from plugins dir to assembled dir.
    directory_link::create(&assembled_dir, &plugin_link)?;

    tracing::info!(package = pkg_name, "package linked successfully");

    Ok(())
}

/// Unlink a package by removing both the directory link and the assembled directory.
///
/// # Errors
///
/// Returns [`Error`] if removal fails.
pub fn unlink_package(pkg_name: &str, links_dir: &Path, plugins_dir: &Path) -> Result<(), Error> {
    tracing::info!(package = pkg_name, "unlinking package");

    let plugin_link = plugins_dir.join(pkg_name);
    let assembled_dir = links_dir.join(pkg_name);

    // Remove the directory symlink/junction first.
    if directory_link::is_link(&plugin_link) {
        tracing::debug!(path = %plugin_link.display(), "removing directory link");
        directory_link::remove(&plugin_link)?;
    }

    // Remove the assembled directory.
    if assembled_dir.exists() {
        std::fs::remove_dir_all(&assembled_dir)
            .map_err(|e| Error::Io { path: assembled_dir, source: e })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, store::Store, BTreeMap<PathBuf, String>) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = store::Store::new(tmp.path().join("store"));

        let hash1 = store.store_file(b"manifest content").expect("store 1");
        let hash2 = store.store_file(b"skill content").expect("store 2");

        let mut file_hashes = BTreeMap::new();
        file_hashes.insert(PathBuf::from("aipm.toml"), hash1);
        file_hashes.insert(PathBuf::from("skills/review.md"), hash2);

        (tmp, store, file_hashes)
    }

    #[test]
    fn link_package_creates_both_tiers() {
        let (tmp, store, file_hashes) = setup();
        let links_dir = tmp.path().join(".aipm/links");
        let plugins_dir = tmp.path().join("claude-plugins");

        let result = link_package(&store, &file_hashes, "my-pkg", &links_dir, &plugins_dir);
        assert!(result.is_ok(), "link_package failed: {result:?}");

        // Tier 1: assembled directory with hard-linked files.
        assert!(links_dir.join("my-pkg/aipm.toml").exists());
        assert!(links_dir.join("my-pkg/skills/review.md").exists());

        // Tier 2: symlink from plugins dir.
        let plugin_link = plugins_dir.join("my-pkg");
        assert!(directory_link::is_link(&plugin_link));

        // Verify content is accessible through the symlink.
        let content = std::fs::read_to_string(plugin_link.join("aipm.toml")).expect("read");
        assert_eq!(content, "manifest content");
    }

    #[test]
    fn unlink_package_removes_both_tiers() {
        let (tmp, store, file_hashes) = setup();
        let links_dir = tmp.path().join(".aipm/links");
        let plugins_dir = tmp.path().join("claude-plugins");

        assert!(link_package(&store, &file_hashes, "my-pkg", &links_dir, &plugins_dir).is_ok());
        let result = unlink_package("my-pkg", &links_dir, &plugins_dir);
        assert!(result.is_ok(), "unlink_package failed: {result:?}");

        assert!(!plugins_dir.join("my-pkg").exists());
        assert!(!links_dir.join("my-pkg").exists());
    }

    #[test]
    fn link_package_replaces_existing() {
        let (tmp, store, file_hashes) = setup();
        let links_dir = tmp.path().join(".aipm/links");
        let plugins_dir = tmp.path().join("claude-plugins");

        // Link once.
        assert!(link_package(&store, &file_hashes, "my-pkg", &links_dir, &plugins_dir).is_ok());

        // Link again — should replace cleanly.
        let result = link_package(&store, &file_hashes, "my-pkg", &links_dir, &plugins_dir);
        assert!(result.is_ok(), "re-link failed: {result:?}");

        assert!(plugins_dir.join("my-pkg/aipm.toml").exists());
    }

    #[test]
    fn unlink_nonexistent_is_noop() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let links_dir = tmp.path().join(".aipm/links");
        let plugins_dir = tmp.path().join("claude-plugins");

        let result = unlink_package("nonexistent", &links_dir, &plugins_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn unlink_package_cleans_assembled_dir_when_plugin_link_absent() {
        // Simulate a state where the assembled dir exists but the plugin link
        // has already been removed (e.g., interrupted uninstall).  This ensures
        // the `if assembled_dir.exists()` branch in `unlink_package` is taken.
        let tmp = tempfile::tempdir().expect("tempdir");
        let links_dir = tmp.path().join(".aipm/links");
        let plugins_dir = tmp.path().join("claude-plugins");

        let assembled_dir = links_dir.join("orphan-pkg");
        std::fs::create_dir_all(&assembled_dir).expect("create assembled dir");
        std::fs::write(assembled_dir.join("aipm.toml"), b"[package]").expect("write file");

        // No plugin link exists — directory_link::is_link branch is skipped.
        let result = unlink_package("orphan-pkg", &links_dir, &plugins_dir);
        assert!(result.is_ok(), "unlink_package failed: {result:?}");

        // Assembled dir must be gone.
        assert!(!assembled_dir.exists(), "assembled_dir should have been removed");
    }
}
