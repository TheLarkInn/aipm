//! Plugin acquisition from various sources.
//!
//! Acquires plugins from local filesystem paths and git repositories via
//! shallow clone.  Authentication for git sources is delegated to the system's
//! git credential helper — aipm does not manage credentials.
//!
//! After initial acquisition, checks for a source redirect in the acquired
//! plugin's `aipm.toml` (`[package.source]` section) and follows it one
//! level deep.

use std::path::{Path, PathBuf};

use crate::engine::Engine;
use crate::path_security::ValidatedPath;
use crate::spec::GitSource;

/// Maximum number of files allowed in a single plugin.
const MAX_PLUGIN_FILES: usize = 500;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur during plugin acquisition.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The local plugin directory does not exist.
    #[error("Plugin directory does not exist: {}", path.display())]
    LocalNotFound { path: PathBuf },

    /// The local plugin path is not a directory.
    #[error("Plugin path is not a directory: {}", path.display())]
    LocalNotDirectory { path: PathBuf },

    /// Failed to copy a local plugin.
    #[error("Failed to copy plugin from {} to {}: {reason}", src.display(), dst.display())]
    CopyFailed { src: PathBuf, dst: PathBuf, reason: String },

    /// Git clone failed.
    #[error("Git clone failed for {url}: {reason}")]
    GitClone { url: String, reason: String },

    /// The requested path does not exist in the cloned repository.
    #[error("Plugin path '{path}' does not exist in repository (ref: {git_ref})")]
    PathNotFound { path: String, git_ref: String },

    /// The plugin directory is empty.
    #[error("Plugin directory is empty: {path}")]
    EmptyDirectory { path: String },

    /// The plugin has too many files.
    #[error("Plugin has too many files: {count} (limit: {limit})")]
    TooManyFiles { count: usize, limit: usize },

    /// Plugin structure validation failed.
    #[error(transparent)]
    Validation(#[from] crate::engine::ValidationError),

    /// A source redirect loop was detected.
    #[error("Source redirect loop detected (max 1 redirect allowed)")]
    RedirectLoop,

    /// An I/O error.
    #[error("I/O error at {}: {reason}", path.display())]
    Io { path: PathBuf, reason: String },
}

// ---------------------------------------------------------------------------
// Local acquisition
// ---------------------------------------------------------------------------

/// Acquire a plugin from a local filesystem path.
///
/// Copies the plugin directory into `dest_dir/<folder_name>/` and validates
/// the resulting structure.
pub fn acquire_local(
    path: &ValidatedPath,
    dest_dir: &Path,
    engine: Engine,
) -> Result<PathBuf, Error> {
    let source = PathBuf::from(path.as_str());

    if !source.exists() {
        return Err(Error::LocalNotFound { path: source });
    }
    if !source.is_dir() {
        return Err(Error::LocalNotDirectory { path: source });
    }

    let folder_name = path.folder_name();
    let dest = dest_dir.join(folder_name);

    std::fs::create_dir_all(&dest)
        .map_err(|e| Error::Io { path: dest.clone(), reason: e.to_string() })?;

    copy_dir_recursive(&source, &dest)?;
    check_file_count(&dest)?;
    crate::engine::validate_plugin(&dest, engine)?;

    Ok(dest)
}

// ---------------------------------------------------------------------------
// Git acquisition
// ---------------------------------------------------------------------------

/// Acquire a plugin from a git repository via shallow clone.
///
/// 1. Clones the repository with `--depth=1`
/// 2. If a subdirectory path is specified, copies just that directory
/// 3. Validates the plugin structure
/// 4. Cleans up the temp clone
pub fn acquire_git(source: &GitSource, dest_dir: &Path, engine: Engine) -> Result<PathBuf, Error> {
    let temp_clone = dest_dir.join(".aipm-clone-temp");
    std::fs::create_dir_all(&temp_clone)
        .map_err(|e| Error::Io { path: temp_clone.clone(), reason: e.to_string() })?;

    // Build git clone command
    let clone_result = run_git_clone(&source.url, source.git_ref.as_deref(), &temp_clone);

    if let Err(e) = clone_result {
        let _ = std::fs::remove_dir_all(&temp_clone);
        return Err(e);
    }

    // Determine source directory (subdirectory or entire clone)
    let plugin_source = if let Some(ref sub_path) = source.path {
        let sub = temp_clone.join(sub_path.as_str());
        if !sub.exists() || !sub.is_dir() {
            let _ = std::fs::remove_dir_all(&temp_clone);
            return Err(Error::PathNotFound {
                path: sub_path.to_string(),
                git_ref: source.git_ref.clone().unwrap_or_else(|| "HEAD".to_string()),
            });
        }
        sub
    } else {
        temp_clone.clone()
    };

    // Check for empty directory
    let is_empty =
        std::fs::read_dir(&plugin_source).map(|mut d| d.next().is_none()).unwrap_or(true);
    if is_empty {
        let _ = std::fs::remove_dir_all(&temp_clone);
        return Err(Error::EmptyDirectory {
            path: source.path.as_ref().map_or_else(|| source.url.clone(), ToString::to_string),
        });
    }

    // Copy to final destination
    let folder_name = source.folder_name();
    let dest = dest_dir.join(&folder_name);
    std::fs::create_dir_all(&dest)
        .map_err(|e| Error::Io { path: dest.clone(), reason: e.to_string() })?;

    let copy_result = copy_dir_recursive(&plugin_source, &dest);

    // Clean up temp clone
    let _ = std::fs::remove_dir_all(&temp_clone);

    copy_result?;
    check_file_count(&dest)?;
    crate::engine::validate_plugin(&dest, engine)?;

    Ok(dest)
}

/// Run `git clone --depth=1` via `std::process::Command`.
fn run_git_clone(url: &str, git_ref: Option<&str>, dest: &Path) -> Result<(), Error> {
    let mut cmd = std::process::Command::new("git");
    cmd.arg("clone").arg("--depth=1");

    if let Some(r) = git_ref {
        cmd.arg("--branch").arg(r);
    }

    cmd.arg(url).arg(dest);

    let output = cmd.output().map_err(|e| Error::GitClone {
        url: url.to_string(),
        reason: format!("failed to execute git: {e}"),
    })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(Error::GitClone { url: url.to_string(), reason: stderr.trim().to_string() })
    }
}

// ---------------------------------------------------------------------------
// Source redirect
// ---------------------------------------------------------------------------

/// Minimal struct for parsing `[package.source]` redirect.
#[derive(serde::Deserialize)]
struct RedirectManifest {
    package: Option<RedirectPackage>,
}

#[derive(serde::Deserialize)]
struct RedirectPackage {
    source: Option<RedirectSource>,
}

#[derive(serde::Deserialize)]
struct RedirectSource {
    #[serde(rename = "type")]
    _type: Option<String>,
    url: String,
    path: Option<String>,
}

/// Check for a source redirect in the acquired plugin's `aipm.toml`.
///
/// If `[package.source]` is present, returns the redirect spec.
/// Otherwise returns `None`.
pub fn check_source_redirect(plugin_dir: &Path) -> Option<GitSource> {
    let manifest_path = plugin_dir.join("aipm.toml");
    let content = std::fs::read_to_string(manifest_path).ok()?;

    let manifest: RedirectManifest = toml::from_str(&content).ok()?;
    let source = manifest.package?.source?;

    let validated_path = source.path.and_then(|p| ValidatedPath::new(p).ok());

    Some(GitSource { url: source.url, path: validated_path, git_ref: None })
}

/// Acquire a plugin, following one level of source redirect if present.
pub fn acquire_with_redirect(
    source: &GitSource,
    dest_dir: &Path,
    engine: Engine,
) -> Result<PathBuf, Error> {
    let plugin_path = acquire_git(source, dest_dir, engine)?;

    // Check for redirect
    if let Some(redirect) = check_source_redirect(&plugin_path) {
        // Delete the stub
        let _ = std::fs::remove_dir_all(&plugin_path);
        // Re-acquire from redirect (no further redirects)
        let redirected_path = acquire_git(&redirect, dest_dir, engine)?;

        // Ensure no second redirect
        if check_source_redirect(&redirected_path).is_some() {
            let _ = std::fs::remove_dir_all(&redirected_path);
            return Err(Error::RedirectLoop);
        }

        return Ok(redirected_path);
    }

    Ok(plugin_path)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Count files in a directory recursively.
fn count_files(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                count += count_files(&entry.path());
            } else {
                count += 1;
            }
        }
    }
    count
}

/// Check that a plugin doesn't exceed the file count limit.
fn check_file_count(dir: &Path) -> Result<(), Error> {
    let count = count_files(dir);
    if count > MAX_PLUGIN_FILES {
        return Err(Error::TooManyFiles { count, limit: MAX_PLUGIN_FILES });
    }
    Ok(())
}

/// Recursively copy directory contents from `src` to `dst`.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), Error> {
    for entry in std::fs::read_dir(src)
        .map_err(|e| Error::Io { path: src.to_path_buf(), reason: e.to_string() })?
        .flatten()
    {
        let dest_path = dst.join(entry.file_name());
        let ft = entry
            .file_type()
            .map_err(|e| Error::Io { path: entry.path(), reason: e.to_string() })?;

        if ft.is_dir() {
            // Skip .git directory
            if entry.file_name() == ".git" {
                continue;
            }
            std::fs::create_dir_all(&dest_path)
                .map_err(|e| Error::Io { path: dest_path.clone(), reason: e.to_string() })?;
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else if ft.is_file() {
            std::fs::copy(entry.path(), &dest_path).map_err(|e| Error::CopyFailed {
                src: entry.path(),
                dst: dest_path,
                reason: e.to_string(),
            })?;
        }
        // Skip symlinks
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
        tempfile::tempdir().unwrap_or_else(|_| std::process::abort())
    }

    fn make_local_plugin(temp: &tempfile::TempDir, name: &str) -> PathBuf {
        let dir = temp.path().join(name);
        std::fs::create_dir_all(dir.join(".claude-plugin")).unwrap_or_else(|_| {});
        std::fs::write(dir.join(".claude-plugin/plugin.json"), "{}").unwrap_or_else(|_| {});
        std::fs::write(dir.join("README.md"), "hello").unwrap_or_else(|_| {});
        dir
    }

    #[test]
    fn acquire_local_valid_plugin() {
        let temp = make_temp();
        let _src = make_local_plugin(&temp, "source-plugin");
        let dest = temp.path().join("dest");
        std::fs::create_dir_all(&dest).unwrap_or_else(|_| {});

        let path = ValidatedPath::new("source-plugin").unwrap_or_else(|_| std::process::abort());
        // Use the temp path as cwd context
        let source_path = temp.path().join("source-plugin");
        let result = acquire_local_from(&source_path, &dest, Engine::Claude, "source-plugin");
        assert!(result.is_ok());
        let plugin_path = result.unwrap_or_else(|_| PathBuf::new());
        assert!(plugin_path.join(".claude-plugin/plugin.json").exists());
        assert!(plugin_path.join("README.md").exists());

        let _ = path; // satisfy unused warning
    }

    #[test]
    fn acquire_local_not_found() {
        let temp = make_temp();
        let path = ValidatedPath::new("nonexistent").unwrap_or_else(|_| std::process::abort());
        let result = acquire_local(&path, temp.path(), Engine::Claude);
        assert!(result.is_err());
    }

    #[test]
    fn acquire_local_from_source_not_found() {
        let temp = make_temp();
        let nonexistent = temp.path().join("does-not-exist");
        let dest = temp.path().join("dest");

        let result = acquire_local_from(&nonexistent, &dest, Engine::Claude, "plugin");
        assert!(result.is_err());
    }

    #[test]
    fn acquire_local_not_directory() {
        let temp = make_temp();
        let file_path = temp.path().join("not-a-dir");
        std::fs::write(&file_path, "just a file").unwrap_or_else(|_| {});

        let path = ValidatedPath::new("not-a-dir").unwrap_or_else(|_| std::process::abort());
        // Create a fake path pointing to the file
        let result = acquire_local_from(&file_path, temp.path(), Engine::Claude, "not-a-dir");
        assert!(result.is_err());

        let _ = path;
    }

    #[test]
    fn acquire_local_validates_structure() {
        let temp = make_temp();
        // Create a directory without any marker files or aipm.toml
        let bad_plugin = temp.path().join("bad-plugin");
        std::fs::create_dir_all(&bad_plugin).unwrap_or_else(|_| {});
        std::fs::write(bad_plugin.join("some-file.txt"), "data").unwrap_or_else(|_| {});

        let dest = temp.path().join("dest");
        std::fs::create_dir_all(&dest).unwrap_or_else(|_| {});

        let result = acquire_local_from(&bad_plugin, &dest, Engine::Claude, "bad-plugin");
        assert!(result.is_err());
    }

    #[test]
    fn file_count_check_passes_normal() {
        let temp = make_temp();
        let dir = temp.path().join("small-plugin");
        std::fs::create_dir_all(&dir).unwrap_or_else(|_| {});
        for i in 0..10 {
            std::fs::write(dir.join(format!("file{i}.txt")), "data").unwrap_or_else(|_| {});
        }
        assert!(check_file_count(&dir).is_ok());
    }

    #[test]
    fn file_count_exceeds_limit() {
        let temp = make_temp();
        let dir = temp.path().join("huge-plugin");
        std::fs::create_dir_all(&dir).unwrap_or_else(|_| {});
        for i in 0..=MAX_PLUGIN_FILES {
            std::fs::write(dir.join(format!("file{i}.txt")), "x").unwrap_or_else(|_| {});
        }
        let result = check_file_count(&dir);
        assert!(result.is_err());
    }

    #[test]
    fn copy_dir_recursive_skips_git_dir() {
        let temp = make_temp();
        let src = temp.path().join("src");
        std::fs::create_dir_all(src.join(".git/objects")).unwrap_or_else(|_| {});
        std::fs::write(src.join(".git/HEAD"), "ref: refs/heads/main").unwrap_or_else(|_| {});
        std::fs::write(src.join("plugin.json"), "{}").unwrap_or_else(|_| {});

        let dst = temp.path().join("dst");
        std::fs::create_dir_all(&dst).unwrap_or_else(|_| {});

        let result = copy_dir_recursive(&src, &dst);
        assert!(result.is_ok());
        assert!(dst.join("plugin.json").exists());
        assert!(!dst.join(".git").exists(), ".git directory should be skipped");
    }

    #[test]
    #[cfg(unix)]
    fn copy_dir_recursive_skips_symlinks() {
        // Covers the `else if ft.is_file()` false branch (line 306): when a
        // directory entry is a symlink, `ft.is_file()` returns false and the
        // entry is silently skipped.
        let temp = make_temp();
        let src = temp.path().join("src");
        std::fs::create_dir_all(&src).unwrap_or_else(|_| {});
        std::fs::write(src.join("regular.txt"), "content").unwrap();
        // A symlink: is_file() returns false for the link itself.
        std::os::unix::fs::symlink(src.join("regular.txt"), src.join("link.txt")).unwrap();

        let dst = temp.path().join("dst");
        std::fs::create_dir_all(&dst).unwrap_or_else(|_| {});

        let result = copy_dir_recursive(&src, &dst);
        assert!(result.is_ok());
        assert!(dst.join("regular.txt").exists(), "regular file should be copied");
        assert!(!dst.join("link.txt").exists(), "symlink should be silently skipped");
    }

    #[test]
    fn source_redirect_parses_from_aipm_toml() {
        let temp = make_temp();
        let dir = temp.path().join("stub-plugin");
        std::fs::create_dir_all(&dir).unwrap_or_else(|_| {});
        std::fs::write(
            dir.join("aipm.toml"),
            concat!(
                "[package]\n",
                "name = \"stub\"\n",
                "version = \"0.0.0\"\n",
                "[package.source]\n",
                "type = \"git\"\n",
                "url = \"https://github.com/org/repo.git\"\n",
                "path = \"plugins/my-plugin\"\n",
            ),
        )
        .unwrap_or_else(|_| {});

        let redirect = check_source_redirect(&dir);
        assert!(redirect.is_some());
        let redirect = redirect.unwrap_or_else(|| std::process::abort());
        assert_eq!(redirect.url, "https://github.com/org/repo.git");
        assert_eq!(redirect.path.as_ref().map(ValidatedPath::as_str), Some("plugins/my-plugin"));
    }

    #[test]
    fn source_redirect_none_when_no_source_section() {
        let temp = make_temp();
        let dir = temp.path().join("normal-plugin");
        std::fs::create_dir_all(&dir).unwrap_or_else(|_| {});
        std::fs::write(
            dir.join("aipm.toml"),
            "[package]\nname = \"normal\"\nversion = \"1.0.0\"\n",
        )
        .unwrap_or_else(|_| {});

        let redirect = check_source_redirect(&dir);
        assert!(redirect.is_none());
    }

    #[test]
    fn source_redirect_none_when_no_aipm_toml() {
        let temp = make_temp();
        let dir = temp.path().join("no-manifest");
        std::fs::create_dir_all(&dir).unwrap_or_else(|_| {});

        let redirect = check_source_redirect(&dir);
        assert!(redirect.is_none());
    }

    #[test]
    fn count_files_nested() {
        let temp = make_temp();
        let dir = temp.path().join("nested");
        std::fs::create_dir_all(dir.join("a/b")).unwrap_or_else(|_| {});
        std::fs::write(dir.join("root.txt"), "r").unwrap_or_else(|_| {});
        std::fs::write(dir.join("a/mid.txt"), "m").unwrap_or_else(|_| {});
        std::fs::write(dir.join("a/b/deep.txt"), "d").unwrap_or_else(|_| {});
        assert_eq!(count_files(&dir), 3);
    }

    #[test]
    fn count_files_empty() {
        let temp = make_temp();
        let dir = temp.path().join("empty");
        std::fs::create_dir_all(&dir).unwrap_or_else(|_| {});
        assert_eq!(count_files(&dir), 0);
    }

    #[test]
    fn count_files_nonexistent_dir() {
        let temp = make_temp();
        let dir = temp.path().join("does-not-exist");
        assert_eq!(count_files(&dir), 0);
    }

    #[test]
    fn check_file_count_empty_dir() {
        let temp = make_temp();
        let dir = temp.path().join("empty");
        std::fs::create_dir_all(&dir).unwrap_or_else(|_| {});
        assert!(check_file_count(&dir).is_ok());
    }

    #[test]
    fn copy_dir_recursive_empty_src() {
        let temp = make_temp();
        let src = temp.path().join("empty-src");
        let dst = temp.path().join("empty-dst");
        std::fs::create_dir_all(&src).unwrap_or_else(|_| {});
        std::fs::create_dir_all(&dst).unwrap_or_else(|_| {});
        assert!(copy_dir_recursive(&src, &dst).is_ok());
    }

    #[test]
    fn source_redirect_with_invalid_toml() {
        let temp = make_temp();
        let dir = temp.path().join("bad-toml");
        std::fs::create_dir_all(&dir).unwrap_or_else(|_| {});
        std::fs::write(dir.join("aipm.toml"), "{{invalid}}").unwrap_or_else(|_| {});
        assert!(check_source_redirect(&dir).is_none());
    }

    #[test]
    fn source_redirect_with_no_package_section() {
        let temp = make_temp();
        let dir = temp.path().join("no-pkg");
        std::fs::create_dir_all(&dir).unwrap_or_else(|_| {});
        std::fs::write(dir.join("aipm.toml"), "[dependencies]\nfoo = \"1.0\"\n")
            .unwrap_or_else(|_| {});
        assert!(check_source_redirect(&dir).is_none());
    }

    /// Covers the `acquire_local` path where the source IS a directory (False
    /// branch of `if !source.is_dir()`). The call proceeds past the dir-check,
    /// copies the directory, then fails at plugin validation because `tests/`
    /// has no plugin structure — exercising the is_dir success branch.
    #[test]
    fn acquire_local_source_is_directory_proceeds_to_validation() {
        let temp = make_temp();
        // "tests" always exists as a directory in the crate-root CWD during
        // `cargo test` (contains a single file: bdd.rs), so the is_dir check
        // passes and acquire_local proceeds to validate_plugin, which fails.
        let path = ValidatedPath::new("tests").unwrap_or_else(|_| std::process::abort());
        let result = acquire_local(&path, temp.path(), Engine::Claude);
        assert!(result.is_err());
    }

    /// Covers the `acquire_local` path where the source path exists on disk but
    /// is a regular file rather than a directory (False at "not found" check,
    /// True at "not a dir" check).
    #[test]
    fn acquire_local_source_is_file_not_dir() {
        let temp = make_temp();
        // "Cargo.toml" always exists in the crate-root CWD during `cargo test`
        // and is a file, not a directory — so acquire_local must return an error.
        let path = ValidatedPath::new("Cargo.toml").unwrap_or_else(|_| std::process::abort());
        let result = acquire_local(&path, temp.path(), Engine::Claude);
        assert!(result.is_err());
    }

    /// Helper: acquire from an explicit source path (bypasses `ValidatedPath`
    /// CWD-relative resolution which doesn't work in temp dirs).
    fn acquire_local_from(
        source: &Path,
        dest_dir: &Path,
        engine: Engine,
        folder_name: &str,
    ) -> Result<PathBuf, Error> {
        if !source.exists() {
            return Err(Error::LocalNotFound { path: source.to_path_buf() });
        }
        if !source.is_dir() {
            return Err(Error::LocalNotDirectory { path: source.to_path_buf() });
        }

        let dest = dest_dir.join(folder_name);
        std::fs::create_dir_all(&dest)
            .map_err(|e| Error::Io { path: dest.clone(), reason: e.to_string() })?;
        copy_dir_recursive(source, &dest)?;
        check_file_count(&dest)?;
        crate::engine::validate_plugin(&dest, engine)?;
        Ok(dest)
    }
}
