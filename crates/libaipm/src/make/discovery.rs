//! Marketplace discovery — walk-up search for `.ai/` directories.

use std::path::{Path, PathBuf};

use crate::fs::Fs;

use super::error::Error;

/// Walk up from `start_dir` to find the nearest `.ai/` marketplace.
///
/// At each directory level, checks for `.ai/.claude-plugin/marketplace.json`.
/// If found, returns the path to the `.ai/` directory.
///
/// Directories that contain `.ai/` but lack the marketplace.json marker are
/// skipped — they may be incomplete or unrelated.
///
/// Returns [`Error::MarketplaceNotFound`] if no valid marketplace is found
/// before reaching the filesystem root.
pub fn find_marketplace(start_dir: &Path, fs: &dyn Fs) -> Result<PathBuf, Error> {
    let mut current = start_dir.to_path_buf();

    loop {
        let ai_dir = current.join(".ai");
        let marker = ai_dir.join(".claude-plugin").join("marketplace.json");

        if fs.exists(&marker) {
            return Ok(ai_dir);
        }

        match current.parent() {
            Some(parent) if parent != current => {
                current = parent.to_path_buf();
            },
            // Reached the root (parent is self or None)
            _ => return Err(Error::MarketplaceNotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    struct MockFs {
        files: Mutex<HashMap<PathBuf, Vec<u8>>>,
    }

    impl MockFs {
        fn new() -> Self {
            Self { files: Mutex::new(HashMap::new()) }
        }

        fn add_file(&self, path: &str) {
            self.files
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(PathBuf::from(path), Vec::new());
        }
    }

    impl crate::fs::Fs for MockFs {
        fn exists(&self, path: &Path) -> bool {
            self.files.lock().unwrap_or_else(|p| p.into_inner()).contains_key(path)
        }

        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            Ok(())
        }

        fn write_file(&self, _: &Path, _: &[u8]) -> std::io::Result<()> {
            Ok(())
        }

        fn read_to_string(&self, _: &Path) -> std::io::Result<String> {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "not used"))
        }

        fn read_dir(&self, _: &Path) -> std::io::Result<Vec<crate::fs::DirEntry>> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn find_marketplace_in_current_dir() {
        let fs = MockFs::new();
        fs.add_file("/project/.ai/.claude-plugin/marketplace.json");

        let result = find_marketplace(Path::new("/project"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.unwrap_or_default(), PathBuf::from("/project/.ai"));
    }

    #[test]
    fn find_marketplace_walks_up() {
        let fs = MockFs::new();
        // Marketplace is two levels up
        fs.add_file("/project/.ai/.claude-plugin/marketplace.json");

        let result = find_marketplace(Path::new("/project/src/deep"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.unwrap_or_default(), PathBuf::from("/project/.ai"));
    }

    #[test]
    fn find_marketplace_not_found() {
        let fs = MockFs::new();
        // No .ai/ anywhere

        let result = find_marketplace(Path::new("/project/src"), &fs);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("marketplace not found"));
    }

    #[test]
    fn find_marketplace_skips_incomplete() {
        let fs = MockFs::new();
        // Has .ai/ but no marketplace.json marker — should skip this level
        fs.add_file("/project/sub/.ai/some-other-file");
        // The real marketplace is one level up
        fs.add_file("/project/.ai/.claude-plugin/marketplace.json");

        let result = find_marketplace(Path::new("/project/sub"), &fs);
        assert!(result.is_ok());
        // Should find the one at /project, not /project/sub
        assert_eq!(result.unwrap_or_default(), PathBuf::from("/project/.ai"));
    }

    #[test]
    fn find_marketplace_root_returns_error() {
        let fs = MockFs::new();

        let result = find_marketplace(Path::new("/"), &fs);
        assert!(result.is_err());
    }
}
