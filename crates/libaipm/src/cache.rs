//! Plugin download cache for avoiding redundant fetches.
//!
//! Stores downloaded plugins in `~/.aipm/cache/` with a JSON index tracking
//! metadata (timestamps, TTL, spec keys).  Supports configurable cache
//! policies and garbage collection of stale entries.
//!
//! This module sits alongside the content-addressable [`store`](crate::store).
//! The store handles file-level deduplication; this module handles
//! download-level freshness and offline support.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::locked_file::LockedFile;

/// Default time (seconds) before a cached plugin is considered stale.
const DEFAULT_TTL_SECS: u64 = 24 * 60 * 60; // 24 hours

/// Default time (days since last access) before a cached plugin is eligible
/// for garbage collection.
const DEFAULT_GC_DAYS: u64 = 30;

// ---------------------------------------------------------------------------
// Cache policy
// ---------------------------------------------------------------------------

/// Controls how the download cache is used during plugin acquisition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Policy {
    /// Use cache if fresh (within TTL), otherwise fetch and update cache.
    #[default]
    Auto,
    /// Only use plugins already in cache; fail if not present.
    CacheOnly,
    /// Always fetch from source; do not read or write cache.
    SkipCache,
    /// Always fetch from source and update the cache.
    ForceRefresh,
    /// Use cache if present (ignore TTL staleness), otherwise fetch and cache.
    CacheNoRefresh,
}

impl Serialize for Policy {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Policy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl std::str::FromStr for Policy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "cache-only" | "cacheonly" => Ok(Self::CacheOnly),
            "skip" | "skip-cache" | "skipcache" => Ok(Self::SkipCache),
            "force-refresh" | "forcerefresh" | "force" => Ok(Self::ForceRefresh),
            "no-refresh" | "norefresh" | "cache-no-refresh" => Ok(Self::CacheNoRefresh),
            _ => Err(format!(
                "Unknown cache policy: '{s}'. Valid: auto, cache-only, skip, force-refresh, no-refresh"
            )),
        }
    }
}

impl std::fmt::Display for Policy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => f.write_str("auto"),
            Self::CacheOnly => f.write_str("cache-only"),
            Self::SkipCache => f.write_str("skip"),
            Self::ForceRefresh => f.write_str("force-refresh"),
            Self::CacheNoRefresh => f.write_str("no-refresh"),
        }
    }
}

// ---------------------------------------------------------------------------
// Cache index (persisted as JSON)
// ---------------------------------------------------------------------------

/// Persistent cache index stored as JSON.
#[derive(Debug, Serialize, Deserialize, Default)]
struct CacheIndex {
    entries: HashMap<String, CacheEntry>,
}

/// Metadata for a single cached plugin.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct CacheEntry {
    /// The original plugin spec string (cache key).
    spec: String,
    /// Directory name under `entries/` (UUID v4 via simple random hex).
    dir_name: String,
    /// Timestamp (secs since epoch) when the entry was last fetched/updated.
    fetched_at: u64,
    /// Timestamp (secs since epoch) when the entry was last accessed.
    last_accessed: u64,
    /// Whether this entry belongs to an installed plugin (exempt from GC).
    #[serde(default)]
    installed: bool,
    /// Per-entry TTL override in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ttl_secs: Option<u64>,
}

impl CacheEntry {
    fn is_stale(&self, global_ttl: Duration) -> bool {
        let ttl = self.ttl_secs.map_or(global_ttl, Duration::from_secs);
        let now = now_secs();
        now.saturating_sub(self.fetched_at) >= ttl.as_secs()
    }
}

// ---------------------------------------------------------------------------
// Download cache
// ---------------------------------------------------------------------------

/// Plugin download cache backed by the filesystem.
pub struct Cache {
    /// Root cache directory (`~/.aipm/cache/`)
    root: PathBuf,
    /// Active cache policy
    policy: Policy,
    /// TTL for cache freshness checks
    ttl: Duration,
    /// GC threshold (days since last access)
    gc_days: u64,
}

impl Cache {
    /// Create a cache with an explicit root (for testing and production use).
    pub const fn with_root(root: PathBuf, policy: Policy) -> Self {
        Self { root, policy, ttl: Duration::from_secs(DEFAULT_TTL_SECS), gc_days: DEFAULT_GC_DAYS }
    }

    /// Check if caching is enabled (reads or writes may happen).
    pub const fn is_enabled(&self) -> bool {
        !matches!(self.policy, Policy::SkipCache)
    }

    /// Try to get a cached plugin, returning its path if valid per current policy.
    ///
    /// Returns `Ok(Some(path))` if a usable cached copy exists,
    /// `Ok(None)` if the plugin needs to be fetched,
    /// `Err` if policy is `CacheOnly` and the plugin is not cached.
    pub fn get(&self, spec_key: &str) -> Result<Option<PathBuf>, Error> {
        if self.policy == Policy::SkipCache || self.policy == Policy::ForceRefresh {
            return Ok(None);
        }

        let index = self.read_index()?;
        let Some(entry) = index.entries.get(spec_key) else {
            if self.policy == Policy::CacheOnly {
                return Err(Error::CacheMiss { spec: spec_key.to_string() });
            }
            return Ok(None);
        };

        let entry_dir = self.entries_dir().join(&entry.dir_name);
        if !entry_dir.exists() {
            if self.policy == Policy::CacheOnly {
                return Err(Error::CacheCorrupted { spec: spec_key.to_string() });
            }
            return Ok(None);
        }

        // Check staleness (Auto policy only)
        if self.policy == Policy::Auto && entry.is_stale(self.ttl) {
            return Ok(None);
        }

        // Update last_accessed
        let _ = self.touch_entry(spec_key);

        Ok(Some(entry_dir))
    }

    /// Store a plugin in the cache.
    ///
    /// Copies plugin content into a new directory, then updates the index
    /// under lock and removes the old directory (if any).
    pub fn put(
        &self,
        spec_key: &str,
        source_dir: &Path,
        ttl_secs: Option<u64>,
    ) -> Result<PathBuf, Error> {
        if self.policy == Policy::SkipCache {
            return Err(Error::SkipCacheWrite);
        }

        self.ensure_dirs()?;

        let dir_name = new_entry_dir_name();
        let entry_dir = self.entries_dir().join(&dir_name);

        std::fs::create_dir_all(&entry_dir)
            .map_err(|source| Error::Io { path: entry_dir.clone(), source })?;

        copy_dir_contents(source_dir, &entry_dir)?;

        let now = now_secs();
        let mut old_dir_name: Option<String> = None;

        self.with_index(|index| {
            let installed = index.entries.get(spec_key).is_some_and(|e| e.installed);

            let old = index.entries.insert(
                spec_key.to_string(),
                CacheEntry {
                    spec: spec_key.to_string(),
                    dir_name: dir_name.clone(),
                    fetched_at: now,
                    last_accessed: now,
                    installed,
                    ttl_secs,
                },
            );

            old_dir_name = old.map(|e| e.dir_name);
        })?;

        // Clean up old directory outside the lock
        if let Some(ref old_name) = old_dir_name {
            if *old_name != dir_name {
                let old_dir = self.entries_dir().join(old_name);
                if old_dir.exists() {
                    let _ = std::fs::remove_dir_all(&old_dir);
                }
            }
        }

        Ok(entry_dir)
    }

    /// Mark a cache entry as "installed" (exempt from GC).
    pub fn mark_installed(&self, spec_key: &str, installed: bool) -> Result<(), Error> {
        self.with_index(|index| {
            if let Some(entry) = index.entries.get_mut(spec_key) {
                entry.installed = installed;
            }
        })
    }

    /// Copy a cached plugin to a session directory.
    pub fn copy_to_session(
        &self,
        spec_key: &str,
        dest_dir: &Path,
        folder_name: &str,
    ) -> Result<PathBuf, Error> {
        let index = self.read_index()?;
        let entry = index
            .entries
            .get(spec_key)
            .ok_or_else(|| Error::CacheMiss { spec: spec_key.to_string() })?;
        let entry_dir = self.entries_dir().join(&entry.dir_name);

        if !entry_dir.exists() {
            return Err(Error::CacheCorrupted { spec: spec_key.to_string() });
        }

        let session_plugin_dir = dest_dir.join(folder_name);
        std::fs::create_dir_all(&session_plugin_dir)
            .map_err(|source| Error::Io { path: session_plugin_dir.clone(), source })?;

        copy_dir_contents(&entry_dir, &session_plugin_dir)?;
        let _ = self.touch_entry(spec_key);

        Ok(session_plugin_dir)
    }

    /// Update the TTL for an existing cache entry.
    pub fn set_entry_ttl(&self, spec_key: &str, ttl_secs: Option<u64>) -> Result<(), Error> {
        self.with_index(|index| {
            if let Some(entry) = index.entries.get_mut(spec_key) {
                entry.ttl_secs = ttl_secs;
            }
        })
    }

    /// Return a copy of this cache with a different policy (shared root).
    #[must_use]
    pub fn with_policy(&self, policy: Policy) -> Self {
        Self { root: self.root.clone(), policy, ttl: self.ttl, gc_days: self.gc_days }
    }

    /// Remove stale cache entries and unreferenced directories.
    pub fn gc(&self) -> Result<(), Error> {
        let now = now_secs();
        let gc_threshold_secs = self.gc_days * 24 * 3600;

        let mut stale_dirs = Vec::new();
        let mut referenced_dirs = std::collections::HashSet::new();

        self.with_index(|index| {
            for entry in index.entries.values() {
                referenced_dirs.insert(entry.dir_name.clone());
            }

            let stale_keys: Vec<String> = index
                .entries
                .iter()
                .filter(|(_, entry)| {
                    !entry.installed && now.saturating_sub(entry.last_accessed) > gc_threshold_secs
                })
                .map(|(key, _)| key.clone())
                .collect();

            for key in &stale_keys {
                if let Some(entry) = index.entries.remove(key) {
                    referenced_dirs.remove(&entry.dir_name);
                    stale_dirs.push(entry.dir_name);
                }
            }
        })?;

        // Remove stale entry directories
        for dir_name in &stale_dirs {
            let dir = self.entries_dir().join(dir_name);
            if dir.exists() {
                let _ = std::fs::remove_dir_all(&dir);
            }
        }

        // Remove unreferenced directories older than GC threshold
        let gc_threshold = Duration::from_secs(gc_threshold_secs);
        if let Ok(read_dir) = std::fs::read_dir(self.entries_dir()) {
            for entry in read_dir.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if !referenced_dirs.contains(name)
                        && entry.path().is_dir()
                        && is_older_than(&entry.path(), gc_threshold)
                    {
                        let _ = std::fs::remove_dir_all(entry.path());
                    }
                }
            }
        }

        Ok(())
    }

    // ---- Internal helpers ----

    fn entries_dir(&self) -> PathBuf {
        self.root.join("entries")
    }

    fn index_path(&self) -> PathBuf {
        self.root.join("cache_index.json")
    }

    fn ensure_dirs(&self) -> Result<(), Error> {
        std::fs::create_dir_all(self.entries_dir())
            .map_err(|source| Error::Io { path: self.root.clone(), source })
    }

    fn read_index(&self) -> Result<CacheIndex, Error> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(CacheIndex::default());
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|source| Error::Io { path: path.clone(), source })?;
        if content.is_empty() {
            return Ok(CacheIndex::default());
        }
        serde_json::from_str(&content).map_err(|e| Error::IndexParse { reason: e.to_string() })
    }

    fn with_index(&self, f: impl FnOnce(&mut CacheIndex)) -> Result<(), Error> {
        self.ensure_dirs()?;

        let mut locked = LockedFile::open(&self.index_path())
            .map_err(|e| Error::Lock { reason: e.to_string() })?;
        let content = locked.read_content().map_err(|e| Error::Lock { reason: e.to_string() })?;
        let mut index: CacheIndex = if content.is_empty() {
            CacheIndex::default()
        } else {
            serde_json::from_str(&content)
                .map_err(|e| Error::IndexParse { reason: e.to_string() })?
        };

        f(&mut index);

        let new_content = serde_json::to_string_pretty(&index)
            .map_err(|e| Error::IndexParse { reason: e.to_string() })?;
        locked.write_content(&new_content).map_err(|e| Error::Lock { reason: e.to_string() })?;
        Ok(())
    }

    fn touch_entry(&self, spec_key: &str) -> Result<(), Error> {
        let now = now_secs();
        self.with_index(|index| {
            if let Some(entry) = index.entries.get_mut(spec_key) {
                entry.last_accessed = now;
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from download cache operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Plugin not found in cache (cache-only mode).
    #[error("Plugin not found in cache (cache-only mode): {spec}")]
    CacheMiss { spec: String },
    /// Cache entry exists but directory is missing.
    #[error("Cache entry corrupted (directory missing): {spec}")]
    CacheCorrupted { spec: String },
    /// Cannot write to cache with skip-cache policy.
    #[error("Cannot write to cache with skip-cache policy")]
    SkipCacheWrite,
    /// An I/O error.
    #[error("Cache I/O error at {}: {source}", path.display())]
    Io { path: PathBuf, source: std::io::Error },
    /// Failed to parse the cache index.
    #[error("Failed to parse cache index: {reason}")]
    IndexParse { reason: String },
    /// Failed to acquire or use file lock.
    #[error("Cache lock error: {reason}")]
    Lock { reason: String },
    /// Failed to copy files.
    #[error("Failed to copy cache files: {reason}")]
    Copy { reason: String },
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Seconds since Unix epoch.
fn now_secs() -> u64 {
    SystemTime::UNIX_EPOCH.elapsed().unwrap_or_default().as_secs()
}

/// Generate a unique directory name using random hex bytes.
fn new_entry_dir_name() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    // Monotonic counter ensures uniqueness even within the same nanosecond
    COUNTER.fetch_add(1, Ordering::Relaxed).hash(&mut hasher);
    SystemTime::UNIX_EPOCH.elapsed().unwrap_or_default().as_nanos().hash(&mut hasher);
    std::thread::current().id().hash(&mut hasher);
    let stack_local = 0u8;
    (std::ptr::addr_of!(stack_local) as usize).hash(&mut hasher);
    format!("{:032x}", u128::from(hasher.finish()) | (u128::from(now_secs()) << 64))
}

/// Returns `true` if the path's modification time is older than `threshold`.
fn is_older_than(path: &Path, threshold: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|mtime| mtime.elapsed().ok())
        .is_some_and(|age| age > threshold)
}

/// Copy directory contents from `src` to `dst` (both must exist).
fn copy_dir_contents(src: &Path, dst: &Path) -> Result<(), Error> {
    for entry in std::fs::read_dir(src)
        .map_err(|source| Error::Io { path: src.to_path_buf(), source })?
        .flatten()
    {
        let dest_path = dst.join(entry.file_name());
        let file_type =
            entry.file_type().map_err(|source| Error::Io { path: entry.path(), source })?;

        if file_type.is_dir() {
            std::fs::create_dir_all(&dest_path)
                .map_err(|source| Error::Io { path: dest_path.clone(), source })?;
            copy_dir_contents(&entry.path(), &dest_path)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &dest_path)
                .map_err(|source| Error::Io { path: dest_path.clone(), source })?;
        }
        // Skip symlinks and other special files
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp() -> tempfile::TempDir {
        match tempfile::tempdir() {
            Ok(t) => t,
            Err(_) => tempfile::tempdir_in(".").unwrap_or_else(|_| std::process::abort()),
        }
    }

    fn test_cache(policy: Policy) -> (tempfile::TempDir, Cache) {
        let temp = make_temp();
        let cache = Cache::with_root(temp.path().join("cache"), policy);
        (temp, cache)
    }

    fn create_source_plugin(temp: &tempfile::TempDir) -> PathBuf {
        let src = temp.path().join("source_plugin");
        std::fs::create_dir_all(src.join("sub")).unwrap_or_else(|_| {});
        std::fs::write(src.join("plugin.json"), "{}").unwrap_or_else(|_| {});
        std::fs::write(src.join("README.md"), "hello").unwrap_or_else(|_| {});
        std::fs::write(src.join("sub/data.txt"), "nested").unwrap_or_else(|_| {});
        src
    }

    #[test]
    fn cache_policy_roundtrip() {
        for policy in [
            Policy::Auto,
            Policy::CacheOnly,
            Policy::SkipCache,
            Policy::ForceRefresh,
            Policy::CacheNoRefresh,
        ] {
            let s = policy.to_string();
            let Ok(parsed) = s.parse::<Policy>() else { continue };
            let parsed: Policy = parsed;
            assert_eq!(policy, parsed);
        }
    }

    #[test]
    fn cache_miss_returns_none() {
        let (_temp, cache) = test_cache(Policy::Auto);
        let result = cache.get("github:owner/repo:plugin@main");
        assert!(result.is_ok());
        let val = result.unwrap_or(Some(PathBuf::new()));
        assert!(val.is_none());
    }

    #[test]
    fn cache_put_and_get() {
        let (temp, cache) = test_cache(Policy::Auto);
        let spec = "github:owner/repo:plugin@main";
        let src = create_source_plugin(&temp);

        let cached_path = cache.put(spec, &src, None);
        assert!(cached_path.is_ok());
        let cached_path = cached_path.unwrap_or_else(|_| PathBuf::new());
        assert!(cached_path.exists());
        assert!(cached_path.join("plugin.json").exists());
        assert!(cached_path.join("README.md").exists());
        assert!(cached_path.join("sub/data.txt").exists());

        let hit = cache.get(spec);
        assert!(hit.is_ok());
        let hit = hit.unwrap_or(None);
        assert!(hit.is_some());
        assert_eq!(hit.unwrap_or_default(), cached_path);
    }

    #[test]
    fn cache_skip_policy_always_misses() {
        let (_temp, cache) = test_cache(Policy::SkipCache);
        let result = cache.get("some-spec");
        assert!(result.is_ok());
        assert!(result.unwrap_or(Some(PathBuf::new())).is_none());
    }

    #[test]
    fn cache_only_errors_on_miss() {
        let (_temp, cache) = test_cache(Policy::CacheOnly);
        let result = cache.get("missing-spec");
        assert!(result.is_err());
    }

    #[test]
    fn cache_force_refresh_always_misses() {
        let (temp, cache) = test_cache(Policy::ForceRefresh);
        let src = create_source_plugin(&temp);
        let _ = cache.put("spec", &src, None);

        let result = cache.get("spec");
        assert!(result.is_ok());
        assert!(result.unwrap_or(Some(PathBuf::new())).is_none());
    }

    #[test]
    fn cache_stale_entry_returns_none() {
        let temp = make_temp();
        let mut cache = Cache::with_root(temp.path().join("cache"), Policy::Auto);
        cache.ttl = Duration::from_secs(0); // Immediately stale

        let src = create_source_plugin(&temp);
        let _ = cache.put("spec", &src, None);

        let result = cache.get("spec");
        assert!(result.is_ok());
        assert!(result.unwrap_or(Some(PathBuf::new())).is_none());
    }

    #[test]
    fn installed_still_respects_ttl() {
        let temp = make_temp();
        let mut cache = Cache::with_root(temp.path().join("cache"), Policy::Auto);
        cache.ttl = Duration::from_secs(0);

        let src = create_source_plugin(&temp);
        let spec = "installed-spec";
        let _ = cache.put(spec, &src, None);
        let _ = cache.mark_installed(spec, true);

        let result = cache.get(spec);
        assert!(result.is_ok());
        assert!(result.unwrap_or(Some(PathBuf::new())).is_none());
    }

    #[test]
    fn copy_to_session() {
        let (temp, cache) = test_cache(Policy::Auto);
        let spec = "test-spec";
        let src = create_source_plugin(&temp);
        let _ = cache.put(spec, &src, None);

        let session_dir = temp.path().join("session");
        std::fs::create_dir_all(&session_dir).unwrap_or_else(|_| {});

        let result = cache.copy_to_session(spec, &session_dir, "my-plugin");
        assert!(result.is_ok());
        let session_path = result.unwrap_or_else(|_| PathBuf::new());
        assert!(session_path.join("plugin.json").exists());
        assert!(session_path.join("sub/data.txt").exists());
    }

    #[test]
    fn new_entry_dir_name_is_unique() {
        let a = new_entry_dir_name();
        // Small delay to ensure different timestamp contribution
        std::thread::sleep(Duration::from_millis(2));
        let b = new_entry_dir_name();
        assert_ne!(a, b, "Directory names should be unique");
    }

    #[test]
    fn gc_removes_old_entries() {
        let temp = make_temp();
        let mut cache = Cache::with_root(temp.path().join("cache"), Policy::Auto);
        cache.gc_days = 0;

        let src = create_source_plugin(&temp);
        let _ = cache.put("old-spec", &src, None);

        // Manually set last_accessed to the past
        let _ = cache.with_index(|index| {
            if let Some(entry) = index.entries.get_mut("old-spec") {
                entry.last_accessed = 0;
            }
        });

        let _ = cache.gc();

        let index = cache.read_index().unwrap_or_default();
        assert!(!index.entries.contains_key("old-spec"));
    }

    #[test]
    fn put_replaces_old_entry_dir() {
        let (temp, cache) = test_cache(Policy::Auto);
        let spec = "replace-spec";

        let src1 = temp.path().join("src1");
        std::fs::create_dir_all(&src1).unwrap_or_else(|_| {});
        std::fs::write(src1.join("version.txt"), "v1").unwrap_or_else(|_| {});
        let dir1 = cache.put(spec, &src1, None).unwrap_or_else(|_| PathBuf::new());
        assert!(dir1.exists());

        let src2 = temp.path().join("src2");
        std::fs::create_dir_all(&src2).unwrap_or_else(|_| {});
        std::fs::write(src2.join("version.txt"), "v2").unwrap_or_else(|_| {});
        let dir2 = cache.put(spec, &src2, None).unwrap_or_else(|_| PathBuf::new());

        assert!(dir2.exists());
        let content = std::fs::read_to_string(dir2.join("version.txt")).unwrap_or_default();
        assert_eq!(content, "v2");
        assert!(!dir1.exists(), "Old directory should be cleaned up");
        assert_ne!(dir1, dir2);
    }

    #[test]
    fn gc_removes_unreferenced_directories() {
        let temp = make_temp();
        let mut cache = Cache::with_root(temp.path().join("cache"), Policy::Auto);
        cache.gc_days = 0;
        let _ = cache.ensure_dirs();

        let stray = cache.entries_dir().join("stray-dir");
        std::fs::create_dir_all(&stray).unwrap_or_else(|_| {});
        std::fs::write(stray.join("file.txt"), "data").unwrap_or_else(|_| {});
        assert!(stray.exists());

        std::thread::sleep(Duration::from_millis(50));
        let _ = cache.gc();

        assert!(!stray.exists());
    }

    #[test]
    fn gc_preserves_recent_unreferenced_directories() {
        let (_temp, cache) = test_cache(Policy::Auto);
        let _ = cache.ensure_dirs();

        let stray = cache.entries_dir().join("recent-stray");
        std::fs::create_dir_all(&stray).unwrap_or_else(|_| {});
        std::fs::write(stray.join("file.txt"), "data").unwrap_or_else(|_| {});

        let _ = cache.gc();
        assert!(stray.exists());
    }

    #[test]
    fn gc_preserves_installed_entries() {
        let temp = make_temp();
        let mut cache = Cache::with_root(temp.path().join("cache"), Policy::Auto);
        cache.gc_days = 0;

        let src = create_source_plugin(&temp);
        let spec = "installed-spec";
        let entry_dir = cache.put(spec, &src, None).unwrap_or_else(|_| PathBuf::new());
        let _ = cache.mark_installed(spec, true);

        // Set last_accessed to the distant past
        let _ = cache.with_index(|index| {
            if let Some(entry) = index.entries.get_mut(spec) {
                entry.last_accessed = 0;
            }
        });

        let _ = cache.gc();

        let index = cache.read_index().unwrap_or_default();
        assert!(index.entries.contains_key(spec));
        assert!(entry_dir.exists());
    }

    #[test]
    fn per_entry_ttl_overrides_global() {
        let temp = make_temp();
        let mut cache = Cache::with_root(temp.path().join("cache"), Policy::Auto);
        // Very long global TTL
        cache.ttl = Duration::from_secs(u64::MAX / 2);

        let src = create_source_plugin(&temp);
        // Per-entry TTL = 0 (immediately stale)
        let _ = cache.put("spec", &src, Some(0));

        let result = cache.get("spec");
        assert!(result.is_ok());
        assert!(
            result.unwrap_or(Some(PathBuf::new())).is_none(),
            "per-entry TTL=0 should make entry immediately stale"
        );
    }

    #[test]
    fn with_policy_shares_root() {
        let (temp, cache) = test_cache(Policy::Auto);
        let src = create_source_plugin(&temp);
        let _ = cache.put("spec", &src, None);

        let no_refresh = cache.with_policy(Policy::CacheNoRefresh);
        let result = no_refresh.get("spec");
        assert!(result.is_ok());
        assert!(
            result.unwrap_or(None).is_some(),
            "no-refresh cache should find entry written by auto cache"
        );
    }

    #[test]
    fn set_entry_ttl_updates_stored_ttl() {
        let (temp, cache) = test_cache(Policy::Auto);
        let src = create_source_plugin(&temp);
        let _ = cache.put("spec", &src, None);

        let _ = cache.set_entry_ttl("spec", Some(7200));
        let index = cache.read_index().unwrap_or_default();
        assert_eq!(index.entries.get("spec").and_then(|e| e.ttl_secs), Some(7200));

        let _ = cache.set_entry_ttl("spec", None);
        let index = cache.read_index().unwrap_or_default();
        assert_eq!(index.entries.get("spec").and_then(|e| e.ttl_secs), None);
    }

    // ---- Additional coverage tests ----

    #[test]
    fn cache_only_corrupted_when_dir_missing() {
        let (temp, cache) = test_cache(Policy::CacheOnly);
        let src = create_source_plugin(&temp);
        let _ = cache.put("spec", &src, None);

        // Remove the cached directory to simulate corruption
        let index = cache.read_index().unwrap_or_default();
        if let Some(entry) = index.entries.get("spec") {
            let dir = cache.entries_dir().join(&entry.dir_name);
            let _ = std::fs::remove_dir_all(&dir);
        }

        let result = cache.get("spec");
        assert!(result.is_err()); // CacheCorrupted error
    }

    #[test]
    fn auto_returns_none_when_dir_missing() {
        let (temp, cache) = test_cache(Policy::Auto);
        let src = create_source_plugin(&temp);
        let _ = cache.put("spec", &src, None);

        // Remove the cached directory
        let index = cache.read_index().unwrap_or_default();
        if let Some(entry) = index.entries.get("spec") {
            let dir = cache.entries_dir().join(&entry.dir_name);
            let _ = std::fs::remove_dir_all(&dir);
        }

        let result = cache.get("spec");
        assert!(result.is_ok());
        assert!(result.unwrap_or(Some(PathBuf::new())).is_none());
    }

    #[test]
    fn skip_cache_put_returns_error() {
        let (_temp, cache) = test_cache(Policy::SkipCache);
        let src = _temp.path().join("src");
        std::fs::create_dir_all(&src).unwrap_or_else(|_| {});
        std::fs::write(src.join("f.txt"), "x").unwrap_or_else(|_| {});

        let result = cache.put("spec", &src, None);
        assert!(result.is_err());
    }

    #[test]
    fn copy_to_session_missing_spec_errors() {
        let (temp, cache) = test_cache(Policy::Auto);
        let session = temp.path().join("session");
        std::fs::create_dir_all(&session).unwrap_or_else(|_| {});

        let result = cache.copy_to_session("nonexistent", &session, "plugin");
        assert!(result.is_err());
    }

    #[test]
    fn copy_to_session_missing_dir_errors() {
        let (temp, cache) = test_cache(Policy::Auto);
        let src = create_source_plugin(&temp);
        let _ = cache.put("spec", &src, None);

        // Remove directory
        let index = cache.read_index().unwrap_or_default();
        if let Some(entry) = index.entries.get("spec") {
            let dir = cache.entries_dir().join(&entry.dir_name);
            let _ = std::fs::remove_dir_all(&dir);
        }

        let session = temp.path().join("session");
        std::fs::create_dir_all(&session).unwrap_or_else(|_| {});
        let result = cache.copy_to_session("spec", &session, "plugin");
        assert!(result.is_err());
    }

    #[test]
    fn mark_installed_nonexistent_is_noop() {
        let (_temp, cache) = test_cache(Policy::Auto);
        let _ = cache.ensure_dirs();
        let result = cache.mark_installed("nonexistent", true);
        assert!(result.is_ok());
    }

    #[test]
    fn set_entry_ttl_nonexistent_is_noop() {
        let (_temp, cache) = test_cache(Policy::Auto);
        let _ = cache.ensure_dirs();
        let result = cache.set_entry_ttl("nonexistent", Some(100));
        assert!(result.is_ok());
    }

    #[test]
    fn gc_with_no_entries_is_noop() {
        let (_temp, cache) = test_cache(Policy::Auto);
        let _ = cache.ensure_dirs();
        let result = cache.gc();
        assert!(result.is_ok());
    }

    #[test]
    fn cache_no_refresh_uses_stale_entry() {
        let temp = make_temp();
        let mut cache = Cache::with_root(temp.path().join("cache"), Policy::CacheNoRefresh);
        cache.ttl = Duration::from_secs(0); // Everything immediately stale

        let src = create_source_plugin(&temp);
        // Put with Auto first (CacheNoRefresh still writes)
        let auto_cache = cache.with_policy(Policy::Auto);
        let _ = auto_cache.put("spec", &src, None);

        // CacheNoRefresh should still return the stale entry
        let result = cache.get("spec");
        assert!(result.is_ok());
        assert!(result.unwrap_or(None).is_some(), "CacheNoRefresh should use stale entry");
    }

    #[test]
    fn is_enabled_for_each_policy() {
        assert!(Cache::with_root(PathBuf::new(), Policy::Auto).is_enabled());
        assert!(Cache::with_root(PathBuf::new(), Policy::CacheOnly).is_enabled());
        assert!(!Cache::with_root(PathBuf::new(), Policy::SkipCache).is_enabled());
        assert!(Cache::with_root(PathBuf::new(), Policy::ForceRefresh).is_enabled());
        assert!(Cache::with_root(PathBuf::new(), Policy::CacheNoRefresh).is_enabled());
    }

    #[test]
    fn gc_stale_dir_that_no_longer_exists() {
        let temp = make_temp();
        let mut cache = Cache::with_root(temp.path().join("cache"), Policy::Auto);
        cache.gc_days = 0;

        let src = create_source_plugin(&temp);
        let _ = cache.put("old", &src, None);

        // Set last_accessed to past AND remove the directory
        let _ = cache.with_index(|index| {
            if let Some(entry) = index.entries.get_mut("old") {
                entry.last_accessed = 0;
                // Remove the actual directory so GC tries to remove a non-existent dir
                let dir = temp.path().join("cache/entries").join(&entry.dir_name);
                let _ = std::fs::remove_dir_all(&dir);
            }
        });

        let result = cache.gc();
        assert!(result.is_ok());
    }
}
