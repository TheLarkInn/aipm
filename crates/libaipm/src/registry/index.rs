//! Git registry index layout and parsing.
//!
//! The index uses a 2-character prefix sharding scheme inspired by Cargo:
//! - 1-char names: `1/{name}`
//! - 2-char names: `2/{name}`
//! - 3-char names: `3/{first-char}/{name}`
//! - 4+ char names: `{first-2-chars}/{next-2-chars}/{name}`
//! - Scoped names (`@scope/name`): `@{first-2-of-scope}/{next-2-of-scope}/@scope/{name}`

use std::path::{Path, PathBuf};

use super::error::Error;
use super::VersionEntry;

/// Compute the index file path for a given package name.
///
/// Follows the Cargo-inspired sharding scheme for the git registry index.
///
/// # Errors
///
/// Returns [`Error::IndexParse`] if the package name is empty or invalid.
pub fn package_path(name: &str) -> Result<PathBuf, Error> {
    if name.is_empty() {
        return Err(Error::IndexParse { reason: "package name cannot be empty".to_string() });
    }

    // Handle scoped packages: @scope/name
    if let Some(stripped) = name.strip_prefix('@') {
        return scoped_package_path(stripped);
    }

    let path = match name.len() {
        1 => PathBuf::from("1").join(name),
        2 => PathBuf::from("2").join(name),
        3 => {
            let first = &name[..1];
            PathBuf::from("3").join(first).join(name)
        },
        _ => {
            let prefix = &name[..2];
            let next = &name[2..4.min(name.len())];
            PathBuf::from(prefix).join(next).join(name)
        },
    };

    Ok(path)
}

/// Compute the index path for a scoped package (without the leading `@`).
///
/// Input: `scope/name` (the `@` has been stripped).
fn scoped_package_path(scope_and_name: &str) -> Result<PathBuf, Error> {
    let (scope, name) = scope_and_name.split_once('/').ok_or_else(|| Error::IndexParse {
        reason: format!("invalid scoped package: @{scope_and_name}"),
    })?;

    if scope.is_empty() || name.is_empty() {
        return Err(Error::IndexParse {
            reason: format!("empty scope or name in @{scope_and_name}"),
        });
    }

    // Use first 2 and next 2 chars of scope for sharding
    let prefix = scope.get(..2).map_or_else(|| format!("@{scope}"), |s| format!("@{s}"));
    let next = scope.get(2..4.min(scope.len())).unwrap_or("").to_string();

    let mut path = PathBuf::from(&prefix);
    if !next.is_empty() {
        path = path.join(&next);
    }
    path = path.join(format!("@{scope}")).join(name);

    Ok(path)
}

/// Parse a JSON-lines index file into a list of version entries.
///
/// Each line in the file is a self-contained JSON object representing
/// one published version.
///
/// # Errors
///
/// Returns [`Error::IndexParse`] if any line fails to parse.
pub fn parse_index_file(content: &str) -> Result<Vec<VersionEntry>, Error> {
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line).map_err(|e| Error::IndexParse {
                reason: format!("failed to parse index line: {e}"),
            })
        })
        .collect()
}

/// Read and parse a package's index file from a local index directory.
///
/// # Errors
///
/// Returns [`Error::PackageNotFound`] if the index file doesn't exist.
/// Returns [`Error::IndexParse`] if the file content is invalid.
pub fn read_package(index_root: &Path, name: &str) -> Result<Vec<VersionEntry>, Error> {
    let rel_path = package_path(name)?;
    let full_path = index_root.join(rel_path);

    let content = std::fs::read_to_string(&full_path)
        .map_err(|_| Error::PackageNotFound { name: name.to_string() })?;

    parse_index_file(&content)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- index_path tests ---

    #[test]
    fn index_path_one_char() {
        let path = package_path("a").unwrap();
        assert_eq!(path, PathBuf::from("1/a"));
    }

    #[test]
    fn index_path_two_char() {
        let path = package_path("ab").unwrap();
        assert_eq!(path, PathBuf::from("2/ab"));
    }

    #[test]
    fn index_path_three_char() {
        let path = package_path("abc").unwrap();
        assert_eq!(path, PathBuf::from("3/a/abc"));
    }

    #[test]
    fn index_path_four_plus_char() {
        let path = package_path("code-review").unwrap();
        assert_eq!(path, PathBuf::from("co/de/code-review"));
    }

    #[test]
    fn index_path_exactly_four_char() {
        let path = package_path("abcd").unwrap();
        assert_eq!(path, PathBuf::from("ab/cd/abcd"));
    }

    #[test]
    fn index_path_scoped() {
        let path = package_path("@company/review-plugin").unwrap();
        assert_eq!(path, PathBuf::from("@co/mp/@company/review-plugin"));
    }

    #[test]
    fn index_path_scoped_short_scope() {
        let path = package_path("@ab/tool").unwrap();
        assert_eq!(path, PathBuf::from("@ab/@ab/tool"));
    }

    #[test]
    fn index_path_empty_errors() {
        assert!(package_path("").is_err());
    }

    #[test]
    fn index_path_scoped_no_name_errors() {
        assert!(package_path("@scope/").is_err());
    }

    #[test]
    fn index_path_scoped_no_scope_errors() {
        assert!(package_path("@/name").is_err());
    }

    // --- parse_index_file tests ---

    #[test]
    fn parse_index_file_single_line() {
        let content = r#"{"name":"pkg","vers":"1.0.0","cksum":"sha512-abc","yanked":false}"#;
        let entries = parse_index_file(content).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "pkg");
    }

    #[test]
    fn parse_index_file_multiple_lines() {
        let content = "\
{\"name\":\"pkg\",\"vers\":\"1.0.0\",\"cksum\":\"sha512-a\"}\n\
{\"name\":\"pkg\",\"vers\":\"1.1.0\",\"cksum\":\"sha512-b\"}\n\
{\"name\":\"pkg\",\"vers\":\"2.0.0\",\"cksum\":\"sha512-c\",\"yanked\":true}\n";

        let entries = parse_index_file(content).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[2].vers, "2.0.0");
        assert!(entries[2].yanked);
    }

    #[test]
    fn parse_index_file_empty() {
        let entries = parse_index_file("").unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_index_file_blank_lines_skipped() {
        let content = "\n{\"name\":\"pkg\",\"vers\":\"1.0.0\",\"cksum\":\"sha512-a\"}\n\n";
        let entries = parse_index_file(content).unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn parse_index_file_invalid_json_errors() {
        let content = "not valid json";
        assert!(parse_index_file(content).is_err());
    }

    // --- read_package_index tests ---

    #[test]
    fn read_package_index_from_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let index_dir = tmp.path();

        // Create the index file for "code-review" at co/de/code-review
        let pkg_dir = index_dir.join("co").join("de");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join("code-review"),
            "{\"name\":\"code-review\",\"vers\":\"1.0.0\",\"cksum\":\"sha512-test\"}\n",
        )
        .unwrap();

        let entries = read_package(index_dir, "code-review").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "code-review");
    }

    #[test]
    fn read_package_index_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let result = read_package(tmp.path(), "nonexistent");
        assert!(result.is_err());
    }
}
