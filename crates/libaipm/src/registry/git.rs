//! Git-based registry backend.
//!
//! Clones a git index repository, reads package metadata from JSON-lines
//! index files, downloads tarballs via HTTP, and verifies SHA-512 checksums.

use std::path::{Path, PathBuf};

use crate::store::hash::sha512_hex;
use crate::version::Version;

use super::config::IndexMeta;
use super::error::Error;
use super::{PackageMetadata, Registry, VersionEntry};

/// A registry backed by a git index repository and HTTP tarball downloads.
///
/// The index is cloned to a local cache directory on first access and
/// fetched (fast-forwarded) on subsequent accesses. Package tarballs are
/// downloaded from the URL template found in the index's `config.json`.
pub struct Git {
    /// Local path to the cloned index repository.
    cache_dir: PathBuf,

    /// URL of the remote git index repository.
    index_url: String,

    /// Whether the index has been synced during this session.
    synced: std::sync::atomic::AtomicBool,
}

impl Git {
    /// Create a new `Git` from a remote index URL.
    ///
    /// The index will be cloned/fetched under `cache_root/{url_hash}/`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the cache directory cannot be created.
    pub fn new(index_url: &str, cache_root: &Path) -> Result<Self, Error> {
        let url_hash = &sha512_hex(index_url.as_bytes())[..16];
        let cache_dir = cache_root.join(url_hash);
        std::fs::create_dir_all(&cache_dir).map_err(|e| Error::Io {
            reason: format!("failed to create registry cache at '{}': {e}", cache_dir.display()),
        })?;

        Ok(Self {
            cache_dir,
            index_url: index_url.to_string(),
            synced: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Create a `Git` from an already-populated local index directory.
    ///
    /// Skips all git operations — useful for testing with a pre-built index.
    pub fn from_local_index(index_dir: &Path) -> Self {
        Self {
            cache_dir: index_dir.to_path_buf(),
            index_url: String::new(),
            synced: std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// Ensure the local index is up-to-date (clone or fetch).
    fn ensure_index(&self) -> Result<(), Error> {
        if self.synced.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }

        let git_dir = self.cache_dir.join(".git");
        if git_dir.exists() {
            self.fetch_index()?;
        } else {
            self.clone_index()?;
        }

        self.synced.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Clone the remote index into `cache_dir`.
    fn clone_index(&self) -> Result<(), Error> {
        tracing::info!(url = %self.index_url, dir = %self.cache_dir.display(), "cloning registry index");
        git2::Repository::clone(&self.index_url, &self.cache_dir).map_err(|e| Error::Io {
            reason: format!("failed to clone registry index '{}': {e}", self.index_url),
        })?;
        Ok(())
    }

    /// Fetch and fast-forward the existing local index.
    fn fetch_index(&self) -> Result<(), Error> {
        tracing::info!(url = %self.index_url, dir = %self.cache_dir.display(), "fetching registry index updates");
        let repo = git2::Repository::open(&self.cache_dir).map_err(|e| Error::Io {
            reason: format!("failed to open registry cache at '{}': {e}", self.cache_dir.display()),
        })?;

        fetch_and_reset(&repo, &self.index_url)
    }

    /// Load the `config.json` from the index root.
    fn load_config(&self) -> Result<IndexMeta, Error> {
        let config_path = self.cache_dir.join("config.json");
        let content = std::fs::read_to_string(&config_path).map_err(|e| Error::Io {
            reason: format!("failed to read config.json at '{}': {e}", config_path.display()),
        })?;
        serde_json::from_str(&content)
            .map_err(|e| Error::IndexParse { reason: format!("invalid config.json: {e}") })
    }

    /// Find the [`VersionEntry`] matching the requested version.
    fn find_version<'a>(
        entries: &'a [VersionEntry],
        name: &str,
        version: &Version,
    ) -> Result<&'a VersionEntry, Error> {
        let ver_str = version.to_string();
        entries
            .iter()
            .find(|e| e.vers == ver_str)
            .ok_or_else(|| Error::VersionNotFound { name: name.to_string(), version: ver_str })
    }
}

/// Fetch origin/HEAD and reset the working tree to match.
fn fetch_and_reset(repo: &git2::Repository, url: &str) -> Result<(), Error> {
    // Fetch from origin
    let mut remote = repo
        .find_remote("origin")
        .or_else(|_| repo.remote_anonymous(url))
        .map_err(|e| Error::Io { reason: format!("failed to find remote: {e}") })?;

    remote
        .fetch(&["refs/heads/*:refs/remotes/origin/*"], None, None)
        .map_err(|e| Error::Io { reason: format!("failed to fetch: {e}") })?;

    // Fast-forward HEAD to origin/HEAD
    let fetch_head = repo
        .find_reference("refs/remotes/origin/HEAD")
        .or_else(|_| repo.find_reference("FETCH_HEAD"))
        .map_err(|e| Error::Io { reason: format!("failed to find FETCH_HEAD: {e}") })?;

    let target = fetch_head
        .peel_to_commit()
        .map_err(|e| Error::Io { reason: format!("failed to peel FETCH_HEAD to commit: {e}") })?;

    repo.reset(target.as_object(), git2::ResetType::Hard, None)
        .map_err(|e| Error::Io { reason: format!("failed to reset to fetched HEAD: {e}") })?;

    Ok(())
}

/// Download bytes from a URL using `ureq`.
fn http_get(url: &str) -> Result<Vec<u8>, Error> {
    tracing::info!(url = %url, "downloading package tarball");
    let response = ureq::get(url)
        .call()
        .map_err(|e| Error::Io { reason: format!("HTTP request failed for '{url}': {e}") })?;

    response.into_body().read_to_vec().map_err(|e| Error::Io {
        reason: format!("failed to read response body from '{url}': {e}"),
    })
}

/// Normalize a checksum string by stripping an optional `sha512-` prefix.
fn normalize_checksum(cksum: &str) -> &str {
    cksum.strip_prefix("sha512-").unwrap_or(cksum)
}

/// Verify that the SHA-512 checksum of `data` matches `expected`.
///
/// `expected` may be a raw 128-char hex string or carry a `sha512-` prefix.
fn verify_checksum(data: &[u8], expected: &str, name: &str, version: &str) -> Result<(), Error> {
    let actual = sha512_hex(data);
    let normalized = normalize_checksum(expected);
    if actual != normalized {
        return Err(Error::ChecksumMismatch {
            name: name.to_string(),
            version: version.to_string(),
            expected: normalized.to_string(),
            actual,
        });
    }
    Ok(())
}

impl Registry for Git {
    fn get_metadata(&self, name: &str) -> Result<PackageMetadata, Error> {
        self.ensure_index()?;
        let entries = super::index::read_package(&self.cache_dir, name)?;
        Ok(PackageMetadata { name: name.to_string(), versions: entries })
    }

    fn download(&self, name: &str, version: &Version) -> Result<Vec<u8>, Error> {
        self.ensure_index()?;

        // Look up the version entry for the checksum
        let entries = super::index::read_package(&self.cache_dir, name)?;
        let entry = Self::find_version(&entries, name, version)?;

        // Build download URL from config.json template
        let config = self.load_config()?;
        let url = config.download_url(name, &version.to_string());

        // Download and verify
        let data = http_get(&url)?;
        verify_checksum(&data, &entry.cksum, name, &version.to_string())?;

        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- verify_checksum tests ---

    #[test]
    fn checksum_matches() {
        let data = b"hello world";
        let expected = sha512_hex(data);
        assert!(verify_checksum(data, &expected, "pkg", "1.0.0").is_ok());
    }

    #[test]
    fn checksum_mismatch() {
        let data = b"hello world";
        let wrong = sha512_hex(b"wrong data");
        let err = verify_checksum(data, &wrong, "test-pkg", "2.0.0").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("checksum mismatch"));
        assert!(msg.contains("test-pkg"));
        assert!(msg.contains("2.0.0"));
    }

    #[test]
    fn checksum_empty_data() {
        let data = b"";
        let expected = sha512_hex(data);
        assert!(verify_checksum(data, &expected, "pkg", "0.0.1").is_ok());
    }

    // --- from_local_index + get_metadata tests ---

    #[test]
    fn get_metadata_from_local_index() {
        let tmp = tempfile::tempdir().unwrap();
        let index_dir = tmp.path();

        // Create index file for "code-review" at co/de/code-review
        let pkg_dir = index_dir.join("co").join("de");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join("code-review"),
            "{\"name\":\"code-review\",\"vers\":\"1.0.0\",\"cksum\":\"sha512-abc\"}\n\
             {\"name\":\"code-review\",\"vers\":\"1.1.0\",\"cksum\":\"sha512-def\"}\n",
        )
        .unwrap();

        let registry = Git::from_local_index(index_dir);
        let meta = registry.get_metadata("code-review").unwrap();

        assert_eq!(meta.name, "code-review");
        assert_eq!(meta.versions.len(), 2);
        assert_eq!(meta.versions[0].vers, "1.0.0");
        assert_eq!(meta.versions[1].vers, "1.1.0");
    }

    #[test]
    fn get_metadata_package_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let registry = Git::from_local_index(tmp.path());
        let result = registry.get_metadata("nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn get_metadata_scoped_package() {
        let tmp = tempfile::tempdir().unwrap();
        let index_dir = tmp.path();

        // Create index file for "@company/review-plugin" at @co/mp/@company/review-plugin
        let pkg_dir = index_dir.join("@co").join("mp").join("@company");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join("review-plugin"),
            "{\"name\":\"@company/review-plugin\",\"vers\":\"0.1.0\",\"cksum\":\"sha512-xyz\"}\n",
        )
        .unwrap();

        let registry = Git::from_local_index(index_dir);
        let meta = registry.get_metadata("@company/review-plugin").unwrap();

        assert_eq!(meta.name, "@company/review-plugin");
        assert_eq!(meta.versions.len(), 1);
    }

    // --- find_version tests ---

    #[test]
    fn find_version_found() {
        let entries = vec![
            VersionEntry {
                name: "pkg".to_string(),
                vers: "1.0.0".to_string(),
                deps: vec![],
                cksum: "abc".to_string(),
                features: std::collections::BTreeMap::new(),
                yanked: false,
            },
            VersionEntry {
                name: "pkg".to_string(),
                vers: "2.0.0".to_string(),
                deps: vec![],
                cksum: "def".to_string(),
                features: std::collections::BTreeMap::new(),
                yanked: false,
            },
        ];
        let v = Version::parse("2.0.0").unwrap();
        let found = Git::find_version(&entries, "pkg", &v).unwrap();
        assert_eq!(found.vers, "2.0.0");
        assert_eq!(found.cksum, "def");
    }

    #[test]
    fn find_version_not_found() {
        let entries = vec![VersionEntry {
            name: "pkg".to_string(),
            vers: "1.0.0".to_string(),
            deps: vec![],
            cksum: "abc".to_string(),
            features: std::collections::BTreeMap::new(),
            yanked: false,
        }];
        let v = Version::parse("9.9.9").unwrap();
        let err = Git::find_version(&entries, "pkg", &v).unwrap_err();
        assert!(err.to_string().contains("9.9.9"));
    }

    // --- load_config tests ---

    #[test]
    fn load_config_valid() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("config.json"),
            r#"{"dl":"https://example.com/{name}/{version}.tar.gz"}"#,
        )
        .unwrap();

        let registry = Git::from_local_index(tmp.path());
        let config = registry.load_config().unwrap();
        assert!(config.dl.contains("{name}"));
        assert!(config.dl.contains("{version}"));
    }

    #[test]
    fn load_config_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let registry = Git::from_local_index(tmp.path());
        let err = registry.load_config().unwrap_err();
        assert!(err.to_string().contains("config.json"));
    }

    #[test]
    fn load_config_invalid_json() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("config.json"), "not json").unwrap();

        let registry = Git::from_local_index(tmp.path());
        let err = registry.load_config().unwrap_err();
        assert!(err.to_string().contains("invalid config.json"));
    }

    // --- new() tests ---

    #[test]
    fn new_creates_cache_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let cache_root = tmp.path().join("cache");

        let registry = Git::new("https://example.com/index.git", &cache_root).unwrap();
        assert!(registry.cache_dir.exists());
        // cache_dir should be under cache_root with a hash-based name
        assert!(registry.cache_dir.starts_with(&cache_root));
    }

    // --- normalize_checksum + verify_checksum with sha512- prefix ---

    #[test]
    fn checksum_with_sha512_prefix_matches() {
        let data = b"hello world";
        let hex = sha512_hex(data);
        let prefixed = format!("sha512-{hex}");
        assert!(verify_checksum(data, &prefixed, "pkg", "1.0.0").is_ok());
    }

    #[test]
    fn checksum_with_sha512_prefix_mismatch() {
        let data = b"hello world";
        let wrong = sha512_hex(b"other data");
        let prefixed = format!("sha512-{wrong}");
        let err = verify_checksum(data, &prefixed, "pkg", "1.0.0").unwrap_err();
        assert!(err.to_string().contains("checksum mismatch"));
    }

    // --- download URL integration ---

    #[test]
    fn download_url_from_config() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("config.json"),
            r#"{"dl":"https://cdn.example.com/packages/{name}-{version}.aipm"}"#,
        )
        .unwrap();

        let registry = Git::from_local_index(tmp.path());
        let config = registry.load_config().unwrap();
        let url = config.download_url("my-plugin", "1.2.3");
        assert_eq!(url, "https://cdn.example.com/packages/my-plugin-1.2.3.aipm");
    }
}
