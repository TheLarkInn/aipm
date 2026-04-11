//! Import resolution for instruction files.
//!
//! Follows `@path/to/file.md` imports and relative markdown inline links,
//! accumulating total line and character counts across all transitively
//! imported files.  Circular references are detected and skipped; absolute
//! paths and path-traversal segments (`..`) are rejected for safety.

use std::collections::HashSet;
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};

use crate::fs::Fs;

/// Parse `@path/to/file.md` import lines from file content.
///
/// A line matches when it begins with `@`, contains no whitespace after the
/// `@`, and ends with `.md` (after trimming surrounding whitespace).
fn parse_at_imports(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let rest = trimmed.strip_prefix('@')?;
            // No whitespace allowed in the path portion
            if rest.contains(char::is_whitespace) {
                return None;
            }
            let ext = Path::new(rest).extension()?;
            if ext.eq_ignore_ascii_case("md") {
                Some(rest.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Parse relative markdown inline links that point to `.md` files.
///
/// Matches `[label](url)` patterns where `url` ends with `.md` and does not
/// begin with `http://` or `https://` (external URLs are not followed).
fn parse_markdown_links(content: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut remaining = content;

    while let Some(bracket_close) = remaining.find("](") {
        let after_paren = &remaining[bracket_close + 2..];
        if let Some(paren_close) = after_paren.find(')') {
            let url = &after_paren[..paren_close];
            let is_md = Path::new(url).extension().is_some_and(|e| e.eq_ignore_ascii_case("md"));
            if is_md && !url.starts_with("http://") && !url.starts_with("https://") {
                links.push(url.to_string());
            }
        }
        remaining = &remaining[bracket_close + 2..];
    }

    links
}

/// Return `true` when `path` is safe to follow.
///
/// Rejects absolute paths and any path containing `..` segments, preventing
/// escapes outside the project tree.
fn is_path_safe(path: &str) -> bool {
    if Path::new(path).is_absolute() {
        return false;
    }
    path.split('/').all(|segment| segment != "..")
}

/// Recursively resolve imports for a single file, accumulating sizes.
///
/// Returns `(total_lines, total_chars)` for the file itself plus all files it
/// transitively imports through `@path` syntax or relative markdown links.
///
/// # Arguments
/// * `file_path` — Absolute or project-relative path of the file to resolve.
/// * `fs` — Filesystem abstraction for reading file contents.
/// * `visited` — Set of already-visited paths; updated on entry to protect
///   against circular imports.
///
/// Returns `(0, 0)` when the file cannot be read, has already been visited,
/// or any error is encountered — callers treat such files as empty.
pub fn resolve_imports<S: BuildHasher>(
    file_path: &Path,
    fs: &dyn Fs,
    visited: &mut HashSet<PathBuf, S>,
) -> (usize, usize) {
    let canonical = file_path.to_path_buf();

    if visited.contains(&canonical) {
        return (0, 0);
    }
    visited.insert(canonical);

    let Ok(content) = fs.read_to_string(file_path) else { return (0, 0) };

    let direct_lines = content.lines().count();
    let direct_chars = content.len();

    let parent_dir = file_path.parent().unwrap_or_else(|| Path::new("."));

    let mut total_lines = direct_lines;
    let mut total_chars = direct_chars;

    for import_path in parse_at_imports(&content) {
        if is_path_safe(&import_path) {
            let resolved = parent_dir.join(&import_path);
            let (lines, chars) = resolve_imports(&resolved, fs, visited);
            total_lines += lines;
            total_chars += chars;
        }
    }

    for link_path in parse_markdown_links(&content) {
        if is_path_safe(&link_path) {
            let resolved = parent_dir.join(&link_path);
            let (lines, chars) = resolve_imports(&resolved, fs, visited);
            total_lines += lines;
            total_chars += chars;
        }
    }

    (total_lines, total_chars)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::lint::rules::test_helpers::MockFs;

    fn make_fs_with_file(path: &str, content: &str) -> MockFs {
        let mut fs = MockFs::new();
        let p = PathBuf::from(path);
        fs.exists.insert(p.clone());
        fs.files.insert(p, content.to_string());
        fs
    }

    // --- parse_at_imports ---

    #[test]
    fn at_import_basic() {
        let content = "@shared/context.md\n\nsome other text";
        let imports = parse_at_imports(content);
        assert_eq!(imports, vec!["shared/context.md"]);
    }

    #[test]
    fn at_import_with_leading_whitespace_trimmed() {
        let content = "  @shared/context.md  ";
        let imports = parse_at_imports(content);
        assert_eq!(imports, vec!["shared/context.md"]);
    }

    #[test]
    fn at_import_non_md_ignored() {
        let content = "@config.json";
        let imports = parse_at_imports(content);
        assert!(imports.is_empty());
    }

    #[test]
    fn at_import_with_spaces_ignored() {
        let content = "@path with spaces.md";
        let imports = parse_at_imports(content);
        assert!(imports.is_empty());
    }

    // --- parse_markdown_links ---

    #[test]
    fn markdown_link_basic() {
        let content = "See [other doc](./other.md) for details";
        let links = parse_markdown_links(content);
        assert_eq!(links, vec!["./other.md"]);
    }

    #[test]
    fn external_url_ignored() {
        let content = "[external](https://example.com/file.md)";
        let links = parse_markdown_links(content);
        assert!(links.is_empty());
    }

    #[test]
    fn http_url_ignored() {
        let content = "[external](http://example.com/file.md)";
        let links = parse_markdown_links(content);
        assert!(links.is_empty());
    }

    #[test]
    fn non_md_link_ignored() {
        let content = "[config](./config.json)";
        let links = parse_markdown_links(content);
        assert!(links.is_empty());
    }

    // --- is_path_safe ---

    #[test]
    fn path_traversal_rejected() {
        assert!(!is_path_safe("../../etc/passwd"));
    }

    #[test]
    fn absolute_path_rejected() {
        assert!(!is_path_safe("/etc/passwd"));
    }

    #[test]
    fn relative_path_safe() {
        assert!(is_path_safe("shared/context.md"));
    }

    // --- resolve_imports integration ---

    #[test]
    fn at_import_resolves_and_sums_sizes() {
        let mut fs = MockFs::new();
        // main file imports shared.md
        let main = PathBuf::from("CLAUDE.md");
        let shared = PathBuf::from("shared.md");
        fs.exists.insert(main.clone());
        fs.files.insert(main.clone(), "@shared.md\nsome text".to_string());
        fs.exists.insert(shared.clone());
        fs.files.insert(shared, "shared content".to_string());

        let mut visited = HashSet::new();
        let (lines, chars) = resolve_imports(&main, &fs, &mut visited);
        // main: 2 lines, shared: 1 line
        assert_eq!(lines, 3);
        // chars of both files combined
        assert!(chars > 0);
    }

    #[test]
    fn markdown_link_resolves() {
        let mut fs = MockFs::new();
        let main = PathBuf::from("CLAUDE.md");
        let other = PathBuf::from("other.md");
        fs.exists.insert(main.clone());
        fs.files.insert(main.clone(), "See [other](other.md)".to_string());
        fs.exists.insert(other.clone());
        fs.files.insert(other, "other content\n".to_string());

        let mut visited = HashSet::new();
        let (lines, chars) = resolve_imports(&main, &fs, &mut visited);
        assert_eq!(lines, 2); // main: 1, other: 1
        assert!(chars > 0);
    }

    #[test]
    fn circular_import_no_infinite_loop() {
        let mut fs = MockFs::new();
        let a = PathBuf::from("a.md");
        let b = PathBuf::from("b.md");
        fs.exists.insert(a.clone());
        fs.files.insert(a.clone(), "@b.md\ncontent a".to_string());
        fs.exists.insert(b.clone());
        fs.files.insert(b, "@a.md\ncontent b".to_string());

        let mut visited = HashSet::new();
        // Should not loop; each file counted once
        let (lines, _chars) = resolve_imports(&a, &fs, &mut visited);
        assert_eq!(lines, 4); // a: 2 lines, b: 2 lines, a again: skipped (visited)
    }

    #[test]
    fn path_traversal_in_at_import_rejected() {
        let mut fs = make_fs_with_file("CLAUDE.md", "@../../etc/passwd.md\ntext");
        let path = PathBuf::from("CLAUDE.md");
        let mut visited = HashSet::new();
        let (lines, _) = resolve_imports(&path, &fs, &mut visited);
        // Only the CLAUDE.md content (2 lines), traversal not followed
        assert_eq!(lines, 2);
    }

    #[test]
    fn absolute_path_in_at_import_rejected() {
        let mut fs = make_fs_with_file("CLAUDE.md", "@/etc/passwd.md\ntext");
        let path = PathBuf::from("CLAUDE.md");
        let mut visited = HashSet::new();
        let (lines, _) = resolve_imports(&path, &fs, &mut visited);
        assert_eq!(lines, 2);
    }

    #[test]
    fn nested_imports_all_counted() {
        let mut fs = MockFs::new();
        let a = PathBuf::from("a.md");
        let b = PathBuf::from("b.md");
        let c = PathBuf::from("c.md");
        fs.exists.insert(a.clone());
        fs.files.insert(a.clone(), "@b.md\na content".to_string());
        fs.exists.insert(b.clone());
        fs.files.insert(b, "@c.md\nb content".to_string());
        fs.exists.insert(c.clone());
        fs.files.insert(c, "c content".to_string());

        let mut visited = HashSet::new();
        let (lines, _) = resolve_imports(&a, &fs, &mut visited);
        assert_eq!(lines, 5); // a: 2, b: 2, c: 1
    }

    #[test]
    fn missing_import_target_silently_skipped() {
        let mut fs = make_fs_with_file("CLAUDE.md", "@nonexistent.md\ntext");
        let path = PathBuf::from("CLAUDE.md");
        let mut visited = HashSet::new();
        let (lines, _) = resolve_imports(&path, &fs, &mut visited);
        assert_eq!(lines, 2); // only CLAUDE.md counted
    }

    #[test]
    fn mixed_imports_both_followed() {
        let mut fs = MockFs::new();
        let main = PathBuf::from("CLAUDE.md");
        let imp = PathBuf::from("imported.md");
        let linked = PathBuf::from("linked.md");
        fs.exists.insert(main.clone());
        fs.files.insert(main.clone(), "@imported.md\nSee [linked](linked.md)\ntext".to_string());
        fs.exists.insert(imp.clone());
        fs.files.insert(imp, "imported".to_string());
        fs.exists.insert(linked.clone());
        fs.files.insert(linked, "linked".to_string());

        let mut visited = HashSet::new();
        let (lines, _) = resolve_imports(&main, &fs, &mut visited);
        assert_eq!(lines, 5); // main: 3, imported: 1, linked: 1
    }

    #[test]
    fn missing_file_returns_zero() {
        let fs = MockFs::new();
        let path = PathBuf::from("nonexistent.md");
        let mut visited = HashSet::new();
        let result = resolve_imports(&path, &fs, &mut visited);
        assert_eq!(result, (0, 0));
    }
}
