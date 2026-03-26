//! Directory link creation — symlinks on Unix, directory junctions on Windows.

use std::path::Path;

use super::error::Error;

/// Create a directory link from `source` to `target`.
///
/// On Unix, creates a symbolic link. On Windows, creates a directory junction.
/// The `source` directory must exist. If `target` already exists as a
/// symlink/junction, it is removed first and re-created.
///
/// # Errors
///
/// Returns [`Error::SourceMissing`] if `source` does not exist.
/// Returns [`Error::TargetExists`] if `target` exists and is not a symlink/junction.
/// Returns [`Error::Io`] on any other I/O failure.
pub fn create(source: &Path, target: &Path) -> Result<(), Error> {
    if !source.exists() {
        return Err(Error::SourceMissing { path: source.to_path_buf() });
    }

    // If target already exists, check if it's a symlink/junction we can replace.
    if target.symlink_metadata().is_ok() {
        if is_link(target) {
            remove_link(target)?;
        } else {
            return Err(Error::TargetExists { path: target.to_path_buf() });
        }
    }

    // Create parent directories if needed.
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
    }

    create_platform_link(source, target)
}

/// Remove a directory link (symlink on Unix, junction on Windows).
///
/// # Errors
///
/// Returns [`Error::Io`] if the removal fails.
pub fn remove(target: &Path) -> Result<(), Error> {
    remove_link(target)
}

/// Check whether a path is a symbolic link (Unix) or directory junction (Windows).
#[must_use]
pub fn is_link(path: &Path) -> bool {
    #[cfg(unix)]
    {
        path.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink())
    }
    #[cfg(windows)]
    {
        if path.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
            return true;
        }
        junction::exists(path).unwrap_or(false)
    }
}

/// Read the target of a directory link.
///
/// # Errors
///
/// Returns [`Error::Io`] if the read fails.
pub fn read_target(path: &Path) -> Result<std::path::PathBuf, Error> {
    std::fs::read_link(path).map_err(|e| Error::Io { path: path.to_path_buf(), source: e })
}

// ---------------------------------------------------------------------------
// Platform-specific implementation
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn create_platform_link(source: &Path, target: &Path) -> Result<(), Error> {
    std::os::unix::fs::symlink(source, target)
        .map_err(|e| Error::Io { path: target.to_path_buf(), source: e })
}

#[cfg(windows)]
fn create_platform_link(source: &Path, target: &Path) -> Result<(), Error> {
    junction::create(source, target)
        .map_err(|e| Error::Io { path: target.to_path_buf(), source: e })
}

fn remove_link(target: &Path) -> Result<(), Error> {
    // On Unix, symlinks are removed with remove_file.
    // On Windows, junctions are removed with remove_dir.
    #[cfg(unix)]
    {
        std::fs::remove_file(target)
            .map_err(|e| Error::Io { path: target.to_path_buf(), source: e })
    }
    #[cfg(windows)]
    {
        junction::delete(target).map_err(|e| Error::Io { path: target.to_path_buf(), source: e })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_read_symlink() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = tmp.path().join("source_dir");
        std::fs::create_dir_all(&source).expect("create source");
        std::fs::write(source.join("file.txt"), "hello").expect("write file");

        let target = tmp.path().join("link_dir");
        let result = create(&source, &target);
        assert!(result.is_ok(), "create should succeed: {result:?}");
        assert!(is_link(&target));

        let read = read_target(&target);
        assert!(read.is_ok());
        assert_eq!(read.expect("read_target").as_path(), source.as_path());
    }

    #[test]
    fn create_replaces_existing_symlink() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source1 = tmp.path().join("src1");
        let source2 = tmp.path().join("src2");
        std::fs::create_dir_all(&source1).expect("create src1");
        std::fs::create_dir_all(&source2).expect("create src2");

        let target = tmp.path().join("link");
        assert!(create(&source1, &target).is_ok());
        assert!(create(&source2, &target).is_ok());

        let read = read_target(&target);
        assert!(read.is_ok());
        assert_eq!(read.expect("read_target").as_path(), source2.as_path());
    }

    #[test]
    fn create_fails_if_source_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = tmp.path().join("nonexistent");
        let target = tmp.path().join("link");

        let result = create(&source, &target);
        assert!(result.is_err());
        assert!(matches!(result, Err(Error::SourceMissing { .. })));
    }

    #[test]
    fn create_fails_if_target_is_regular_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = tmp.path().join("source");
        std::fs::create_dir_all(&source).expect("create source");

        let target = tmp.path().join("existing_dir");
        std::fs::create_dir_all(&target).expect("create existing dir");

        let result = create(&source, &target);
        assert!(result.is_err());
        assert!(matches!(result, Err(Error::TargetExists { .. })));
    }

    #[test]
    fn remove_symlink() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = tmp.path().join("source");
        std::fs::create_dir_all(&source).expect("create source");
        let target = tmp.path().join("link");
        assert!(create(&source, &target).is_ok());

        let result = remove(&target);
        assert!(result.is_ok());
        assert!(!target.exists());
        assert!(!is_link(&target));
    }

    #[test]
    fn is_link_returns_false_for_regular_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("regular");
        std::fs::create_dir_all(&dir).expect("create dir");
        assert!(!is_link(&dir));
    }

    #[test]
    fn is_link_returns_false_for_nonexistent() {
        assert!(!is_link(Path::new("/nonexistent/path")));
    }

    #[test]
    fn create_makes_parent_dirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = tmp.path().join("source");
        std::fs::create_dir_all(&source).expect("create source");

        let target = tmp.path().join("deep").join("nested").join("link");
        let result = create(&source, &target);
        assert!(result.is_ok(), "should create parent dirs: {result:?}");
        assert!(is_link(&target));
    }
}
