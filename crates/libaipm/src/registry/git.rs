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

    #[test]
    fn checksum_with_sha512_prefix() {
        let data = b"test data";
        let hash = sha512_hex(data);
        let prefixed = format!("sha512-{hash}");
        assert!(verify_checksum(data, &prefixed, "pkg", "1.0.0").is_ok());
    }

    #[test]
    fn normalize_strips_prefix() {
        assert_eq!(normalize_checksum("sha512-abc123"), "abc123");
    }

    #[test]
    fn normalize_no_prefix_unchanged() {
        assert_eq!(normalize_checksum("abc123"), "abc123");
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
    fn new_returns_err_when_cache_dir_cannot_be_created() {
        let tmp = tempfile::tempdir().unwrap();
        // Place a regular file where cache_root would be.  create_dir_all will
        // fail because it cannot descend into a file to create a sub-directory.
        let not_a_dir = tmp.path().join("not_a_dir");
        std::fs::write(&not_a_dir, b"I am a file").unwrap();

        // cache_dir = not_a_dir/<url_hash> — creation must fail.
        assert!(
            Git::new("https://example.com/index.git", &not_a_dir).is_err(),
            "expected Err when cache_dir cannot be created"
        );
    }

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

    // --- http_get tests ---

    /// Bind a TCP listener on a random loopback port and return it together
    /// with the URL base `http://127.0.0.1:<port>`.
    fn bind_test_server() -> (std::net::TcpListener, String) {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("bind failed in test helper");
        let port = listener.local_addr().expect("local_addr failed in test helper").port();
        let base = format!("http://127.0.0.1:{port}");
        (listener, base)
    }

    /// Serve exactly one request from `listener`, writing `response_bytes` as
    /// the raw HTTP response, then close the connection.
    fn serve_one(listener: std::net::TcpListener, response_bytes: &'static [u8]) {
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                use std::io::{Read, Write};
                let mut buf = [0u8; 4096];
                // Drain the request so the client does not get a broken-pipe
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(response_bytes);
            }
        });
    }

    #[test]
    fn http_get_success_returns_body() {
        let body = b"hello tarball";
        let response: &'static [u8] =
            b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\nhello tarball";

        let (listener, base) = bind_test_server();
        serve_one(listener, response);

        let url = format!("{base}/pkg.tar.gz");
        let result = http_get(&url);
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        assert_eq!(result.unwrap(), body);
    }

    #[test]
    fn http_get_404_returns_err() {
        let response: &'static [u8] =
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

        let (listener, base) = bind_test_server();
        serve_one(listener, response);

        let url = format!("{base}/missing.tar.gz");
        let result = http_get(&url);
        assert!(result.is_err(), "expected Err for 404, got Ok");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("HTTP request failed"), "unexpected error message: {err_msg}");
    }

    #[test]
    fn http_get_connection_refused_returns_err() {
        // Pick a port then immediately drop the listener so it is not listening.
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind failed in test helper");
            l.local_addr().expect("local_addr failed in test helper").port()
            // `l` is dropped here — the port is no longer bound
        };
        let url = format!("http://127.0.0.1:{port}/pkg.tar.gz");
        let result = http_get(&url);
        assert!(result.is_err(), "expected Err for refused connection, got Ok");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("HTTP request failed"), "unexpected error message: {err_msg}");
    }

    // --- ensure_index branch coverage ---

    /// Covers: `synced=false` + `git_dir.exists()=false` → clone path in `ensure_index`.
    ///
    /// `Git::new()` initialises `synced=false`. The first `get_metadata` call therefore
    /// enters the "not yet synced" branch of `ensure_index`.  Because the cache directory
    /// has no `.git` sub-directory, the code takes the clone path.  Pointing the URL at a
    /// plain (non-git) directory makes `clone_index` fail immediately, so the test does not
    /// need network access.
    #[test]
    fn ensure_index_clone_path_covered_when_not_synced() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache_root = tmp.path().join("cache");

        // A plain directory (not a git repo) used as the "remote" URL.
        let not_a_repo = tmp.path().join("not-a-git-repo");
        std::fs::create_dir_all(&not_a_repo).expect("create dir");
        let url = format!("file://{}", not_a_repo.display());

        // Git::new() creates cache_dir with synced=false.
        let git = Git::new(&url, &cache_root).expect("Git::new should succeed");

        // Triggers: ensure_index → synced=false → git_dir.exists()=false → clone_index → Err.
        let result = git.get_metadata("any-pkg");
        assert!(result.is_err(), "expected Err when source is not a git repo: {result:?}");
    }

    /// Covers: `synced=false` + `git_dir.exists()=true` → fetch path in `ensure_index`.
    ///
    /// A minimal local git repository is initialised and cloned into the exact cache
    /// directory that `Git::new()` would compute, so that `.git` already exists when
    /// `ensure_index` runs.  The code takes the fetch path; the fetch succeeds against
    /// the local repo.  The package lookup then fails with "not found", which is fine —
    /// the goal is branch coverage of `ensure_index`, not a successful download.
    #[test]
    fn ensure_index_fetch_path_covered_when_git_dir_exists() {
        let tmp = tempfile::tempdir().expect("tempdir");

        // --- set up a minimal local git repo to act as the "remote" ---
        let remote_dir = tmp.path().join("remote");
        std::fs::create_dir_all(&remote_dir).expect("create remote dir");
        let repo = git2::Repository::init(&remote_dir).expect("git init");

        // Create an initial commit so the repo has a valid HEAD.
        let sig = git2::Signature::now("Test", "test@test.invalid").expect("signature");
        let mut index = repo.index().expect("repo index");
        let tree_id = index.write_tree().expect("write_tree");
        let tree = repo.find_tree(tree_id).expect("find_tree");
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[]).expect("initial commit");

        let url = format!("file://{}", remote_dir.display());

        // --- pre-populate the cache directory that Git::new() will use ---
        let cache_root = tmp.path().join("cache");
        let git = Git::new(&url, &cache_root).expect("Git::new");
        std::fs::create_dir_all(&git.cache_dir).expect("create cache_dir");
        git2::Repository::clone(&url, &git.cache_dir).expect("pre-clone");
        assert!(
            git.cache_dir.join(".git").exists(),
            "expected pre-cloned cache dir to contain .git so the fetch path is exercised"
        );

        // --- exercise the fetch branch ---

        // Triggers: ensure_index → synced=false → git_dir.exists()=true → fetch_index.
        // After fetching, the package lookup fails with "not found" — that is expected.
        let result = git.get_metadata("nonexistent-pkg");
        assert!(result.is_err(), "expected Err (package not found after fetch): {result:?}");
    }

    // --- download() integration tests ---

    /// Covers the happy-path through `download()`: index lookup → config load →
    /// HTTP fetch → checksum verification → return bytes.
    ///
    /// Uses `from_local_index` (already marked synced) so no git operations are needed,
    /// plus a minimal in-process HTTP server via `bind_test_server` / `serve_one`.
    #[test]
    fn download_success_from_local_index() {
        let content: &[u8] = b"fake tarball content for download test";
        let cksum = sha512_hex(content);

        let tmp = tempfile::tempdir().unwrap();
        let index_dir = tmp.path();

        // Create index entry for "code-review" at co/de/code-review
        let pkg_dir = index_dir.join("co").join("de");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join("code-review"),
            format!("{{\"name\":\"code-review\",\"vers\":\"1.0.0\",\"cksum\":\"{cksum}\"}}\n"),
        )
        .unwrap();

        // Bind a local HTTP test server
        let (listener, base) = bind_test_server();

        // Write config.json pointing at our in-process test server
        std::fs::write(
            index_dir.join("config.json"),
            format!("{{\"dl\":\"{base}/{{name}}/{{version}}.aipm\"}}"),
        )
        .unwrap();

        // Build a complete HTTP/1.1 response
        let mut response_vec: Vec<u8> = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            content.len()
        )
        .into_bytes();
        response_vec.extend_from_slice(content);
        let response: &'static [u8] = Box::leak(response_vec.into_boxed_slice());

        serve_one(listener, response);

        let registry = Git::from_local_index(index_dir);
        let version = Version::parse("1.0.0").unwrap();
        let result = registry.download("code-review", &version);
        assert!(result.is_ok(), "expected download to succeed, got: {result:?}");
        assert_eq!(result.unwrap(), content);
    }

    /// Covers the `find_version` error branch inside `download()`:
    /// the requested version does not exist in the index.
    #[test]
    fn download_version_not_found_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let index_dir = tmp.path();

        let pkg_dir = index_dir.join("co").join("de");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join("code-review"),
            "{\"name\":\"code-review\",\"vers\":\"1.0.0\",\"cksum\":\"abc\"}\n",
        )
        .unwrap();

        // config.json is not needed: find_version fails before load_config is called
        let registry = Git::from_local_index(index_dir);
        let version = Version::parse("9.9.9").unwrap();
        let result = registry.download("code-review", &version);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("9.9.9"),
            "error should mention the missing version"
        );
    }

    /// Covers: `synced=false` + `git_dir.exists()=true` → `fetch_index` → `Repository::open`
    /// fails because the `.git` directory is not a valid git repository.
    ///
    /// This exercises the error branch at the `git2::Repository::open` call inside
    /// `fetch_index`, which returns `Err(Error::Io { reason: "failed to open registry cache …" })`.
    #[test]
    fn fetch_index_returns_err_when_git_dir_is_invalid() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache_root = tmp.path().join("cache");
        let url = "https://invalid.example.com/index.git";

        // Git::new() creates cache_dir with synced=false.
        let git = Git::new(url, &cache_root).expect("Git::new should succeed");

        // Create a fake (empty) .git directory so ensure_index takes the fetch path.
        let git_dir = git.cache_dir.join(".git");
        std::fs::create_dir_all(&git_dir).expect("create fake .git dir");

        // Triggers: ensure_index → synced=false → git_dir.exists()=true →
        //           fetch_index → Repository::open fails → Err.
        let result = git.get_metadata("any-pkg");
        assert!(result.is_err(), "expected Err when cache dir has invalid .git: {result:?}");
        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            err_msg.contains("failed to open registry cache"),
            "unexpected error message: {err_msg}"
        );
    }

    /// Covers the checksum-mismatch branch inside `download()`:
    /// the server returns bytes whose SHA-512 does not match the index entry.
    #[test]
    fn download_checksum_mismatch_errors() {
        let content: &[u8] = b"tampered tarball bytes";
        let wrong_cksum = sha512_hex(b"different content entirely");

        let tmp = tempfile::tempdir().unwrap();
        let index_dir = tmp.path();

        let pkg_dir = index_dir.join("co").join("de");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join("code-review"),
            format!(
                "{{\"name\":\"code-review\",\"vers\":\"1.0.0\",\"cksum\":\"{wrong_cksum}\"}}\n"
            ),
        )
        .unwrap();

        let (listener, base) = bind_test_server();
        std::fs::write(
            index_dir.join("config.json"),
            format!("{{\"dl\":\"{base}/{{name}}/{{version}}.aipm\"}}"),
        )
        .unwrap();

        let mut response_vec: Vec<u8> = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            content.len()
        )
        .into_bytes();
        response_vec.extend_from_slice(content);
        let response: &'static [u8] = Box::leak(response_vec.into_boxed_slice());
        serve_one(listener, response);

        let registry = Git::from_local_index(index_dir);
        let version = Version::parse("1.0.0").unwrap();
        let result = registry.download("code-review", &version);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("checksum mismatch"),
            "expected a checksum-mismatch error"
        );
    }
}
