//! Strict, YAML-backed validation of markdown frontmatter blocks.
//!
//! The line-oriented scanner in the parent [`crate::frontmatter`] module is
//! deliberately lenient: it tolerates leading blank lines, never runs a real
//! YAML parser, and exposes every value as raw text. That behaviour is useful
//! for best-effort field extraction during migration, but it cannot answer the
//! questions the AI engines actually ask when they load a `SKILL.md`:
//!
//! * Does the file start *immediately* with an exact `---` line?
//! * Is the closing delimiter an exact `---` line?
//! * Is the block between the delimiters valid YAML?
//! * Is the YAML root a key/value mapping?
//! * Is a given field a *string* (rather than a number, list, or mapping)?
//! * What is the field's value *after* folded/block scalars are resolved?
//!
//! This module answers all of them. Both LF and CRLF line endings are accepted
//! because [`str::lines`] normalizes `\r\n` away.

use std::collections::BTreeMap;

use yaml_rust2::{Yaml, YamlLoader};

/// The exact delimiter line that must open and close a frontmatter block.
const DELIMITER: &str = "---";

/// Reason a frontmatter block failed strict validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The file does not begin immediately with an exact `---` line.
    MissingOpeningDelimiter {
        /// `true` when the file begins with a UTF-8 byte-order mark, an
        /// invisible cause of this failure worth calling out separately.
        byte_order_mark: bool,
    },
    /// No exact `---` line closes the block.
    MissingClosingDelimiter,
    /// The block between the delimiters is not valid YAML.
    InvalidYaml {
        /// Human-readable parser message.
        message: String,
        /// 1-based line number *within the file* where the parser gave up.
        line: usize,
    },
    /// The YAML parsed, but its root is not a key/value mapping.
    RootNotMapping,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::MissingOpeningDelimiter { byte_order_mark: true } => f.write_str(
                "file must start immediately with a `---` frontmatter delimiter (a UTF-8 byte-order mark precedes it)",
            ),
            Self::MissingOpeningDelimiter { byte_order_mark: false } => {
                f.write_str("file must start immediately with a line containing exactly `---`")
            },
            Self::MissingClosingDelimiter => {
                f.write_str("frontmatter is not closed by a line containing exactly `---`")
            },
            Self::InvalidYaml { ref message, line } => {
                write!(f, "frontmatter is not valid YAML (line {line}): {message}")
            },
            Self::RootNotMapping => {
                f.write_str("frontmatter must be a YAML mapping of keys to values")
            },
        }
    }
}

impl std::error::Error for Error {}

/// A strictly validated frontmatter block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Root mapping entries keyed by their string keys. Keys that are not
    /// YAML strings (e.g. `1: x`) are dropped — no aipm field uses them.
    fields: BTreeMap<String, Yaml>,
    /// 1-based line number of the closing `---` delimiter.
    pub end_line: usize,
}

impl Document {
    /// Look up a root-level field by key.
    pub fn get(&self, key: &str) -> Option<&Yaml> {
        self.fields.get(key)
    }

    /// Look up a root-level field that must be a YAML string.
    ///
    /// Returns `None` when the key is absent *or* present with a non-string
    /// value. Folded (`>`) and literal (`|`) block scalars are returned in
    /// their parsed form, so callers measure the value the engine sees.
    pub fn string(&self, key: &str) -> Option<&str> {
        match self.fields.get(key) {
            Some(Yaml::String(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Human-readable YAML type name of a root-level field, for diagnostics.
    ///
    /// Returns `None` when the key is absent.
    pub fn type_name(&self, key: &str) -> Option<&'static str> {
        self.fields.get(key).map(yaml_type_name)
    }
}

/// Describe a [`Yaml`] value's type for diagnostic messages.
const fn yaml_type_name(value: &Yaml) -> &'static str {
    match *value {
        Yaml::Real(_) => "number",
        Yaml::Integer(_) => "integer",
        Yaml::String(_) => "string",
        Yaml::Boolean(_) => "boolean",
        Yaml::Array(_) => "list",
        Yaml::Hash(_) => "mapping",
        Yaml::Alias(_) => "alias",
        Yaml::Null => "null",
        Yaml::BadValue => "invalid value",
    }
}

/// Strictly parse and validate the frontmatter block of `content`.
///
/// # Errors
///
/// Returns [`Error`] when the delimiters are missing or inexact, the block is
/// not valid YAML, or the YAML root is not a mapping.
pub fn parse(content: &str) -> Result<Document, Error> {
    let mut lines = content.lines();
    if lines.next() != Some(DELIMITER) {
        return Err(Error::MissingOpeningDelimiter {
            byte_order_mark: content.starts_with('\u{feff}'),
        });
    }

    let mut block = String::new();
    let mut end_line = None;
    for (offset, line) in lines.enumerate() {
        if line == DELIMITER {
            // `offset` is 0-based from the line after the opening delimiter,
            // which is file line 2 — hence `+ 2`.
            end_line = Some(offset + 2);
            break;
        }
        block.push_str(line);
        block.push('\n');
    }
    let Some(end_line) = end_line else {
        return Err(Error::MissingClosingDelimiter);
    };

    let documents = YamlLoader::load_from_str(&block).map_err(|e| Error::InvalidYaml {
        message: e.to_string(),
        // The scanner's line is 1-based within `block`, which starts on file
        // line 2.
        line: e.marker().line().saturating_add(1),
    })?;

    let Some(Yaml::Hash(hash)) = documents.first() else {
        return Err(Error::RootNotMapping);
    };

    let fields = hash
        .iter()
        .filter_map(|(key, value)| match *key {
            Yaml::String(ref k) => Some((k.clone(), value.clone())),
            _ => None,
        })
        .collect();

    Ok(Document { fields, end_line })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_minimal_document() {
        let doc = parse("---\nname: a\ndescription: b\n---\nbody").expect("valid");
        assert_eq!(doc.string("name"), Some("a"));
        assert_eq!(doc.string("description"), Some("b"));
        assert_eq!(doc.end_line, 4);
    }

    #[test]
    fn accepts_crlf_line_endings() {
        let doc = parse("---\r\nname: a\r\ndescription: b\r\n---\r\nbody").expect("valid");
        assert_eq!(doc.string("name"), Some("a"));
        assert_eq!(doc.string("description"), Some("b"));
        assert_eq!(doc.end_line, 4);
    }

    #[test]
    fn accepts_frontmatter_without_trailing_body() {
        let doc = parse("---\nname: a\n---").expect("valid");
        assert_eq!(doc.string("name"), Some("a"));
    }

    #[test]
    fn rejects_leading_blank_line() {
        assert_eq!(
            parse("\n---\nname: a\n---\n"),
            Err(Error::MissingOpeningDelimiter { byte_order_mark: false })
        );
    }

    #[test]
    fn rejects_leading_byte_order_mark() {
        assert_eq!(
            parse("\u{feff}---\nname: a\n---\n"),
            Err(Error::MissingOpeningDelimiter { byte_order_mark: true })
        );
    }

    #[test]
    fn rejects_inexact_opening_delimiter() {
        for content in ["----\nname: a\n---\n", "--- \nname: a\n---\n", " ---\nname: a\n---\n"] {
            assert!(
                matches!(parse(content), Err(Error::MissingOpeningDelimiter { .. })),
                "expected rejection of {content:?}"
            );
        }
    }

    #[test]
    fn rejects_inexact_closing_delimiter() {
        for content in ["---\nname: a\n----\nbody", "---\nname: a\n--- \nbody", "---\nname: a\n"] {
            assert_eq!(
                parse(content),
                Err(Error::MissingClosingDelimiter),
                "expected rejection of {content:?}"
            );
        }
    }

    #[test]
    fn rejects_invalid_yaml() {
        let err = parse("---\nname: [unclosed\n---\nbody").expect_err("should fail");
        assert!(matches!(err, Error::InvalidYaml { .. }), "got {err:?}");
    }

    #[test]
    fn invalid_yaml_line_is_file_relative() {
        let err = parse("---\nname: ok\ntags: [a, b\n---\nbody").expect_err("should fail");
        let line = if let Error::InvalidYaml { line, .. } = err { line } else { 0 };
        assert!(line >= 2, "expected a file-relative line >= 2, got {line}");
    }

    #[test]
    fn rejects_non_mapping_root() {
        assert_eq!(parse("---\n- a\n- b\n---\nbody"), Err(Error::RootNotMapping));
        assert_eq!(parse("---\njust a scalar\n---\nbody"), Err(Error::RootNotMapping));
    }

    #[test]
    fn rejects_empty_frontmatter() {
        assert_eq!(parse("---\n---\nbody"), Err(Error::RootNotMapping));
    }

    #[test]
    fn folded_scalar_is_measured_after_parsing() {
        let doc = parse("---\nname: a\ndescription: >\n  one\n  two\n---\nbody").expect("valid");
        assert_eq!(doc.string("description"), Some("one two\n"));
    }

    #[test]
    fn literal_scalar_is_measured_after_parsing() {
        let doc = parse("---\nname: a\ndescription: |\n  one\n  two\n---\nbody").expect("valid");
        assert_eq!(doc.string("description"), Some("one\ntwo\n"));
    }

    #[test]
    fn non_string_fields_are_not_strings() {
        let doc = parse("---\nname: 42\ndescription: [a, b]\n---\nbody").expect("valid");
        assert_eq!(doc.string("name"), None);
        assert_eq!(doc.string("description"), None);
        assert_eq!(doc.type_name("name"), Some("integer"));
        assert_eq!(doc.type_name("description"), Some("list"));
        assert_eq!(doc.type_name("missing"), None);
    }

    #[test]
    fn get_returns_underlying_value() {
        let doc = parse("---\nname: a\n---\nbody").expect("valid");
        assert_eq!(doc.get("name"), Some(&Yaml::String("a".to_string())));
        assert_eq!(doc.get("nope"), None);
    }

    #[test]
    fn non_string_keys_are_ignored() {
        let doc = parse("---\n1: one\nname: a\n---\nbody").expect("valid");
        assert_eq!(doc.string("name"), Some("a"));
        assert_eq!(doc.get("1"), None);
    }

    #[test]
    fn type_names_cover_every_variant() {
        assert_eq!(yaml_type_name(&Yaml::Real("1.5".to_string())), "number");
        assert_eq!(yaml_type_name(&Yaml::Integer(1)), "integer");
        assert_eq!(yaml_type_name(&Yaml::String(String::new())), "string");
        assert_eq!(yaml_type_name(&Yaml::Boolean(true)), "boolean");
        assert_eq!(yaml_type_name(&Yaml::Array(vec![])), "list");
        assert_eq!(yaml_type_name(&Yaml::Hash(yaml_rust2::yaml::Hash::new())), "mapping");
        assert_eq!(yaml_type_name(&Yaml::Alias(0)), "alias");
        assert_eq!(yaml_type_name(&Yaml::Null), "null");
        assert_eq!(yaml_type_name(&Yaml::BadValue), "invalid value");
    }

    #[test]
    fn error_display_messages() {
        assert!(Error::MissingOpeningDelimiter { byte_order_mark: false }
            .to_string()
            .contains("start immediately"));
        assert!(Error::MissingOpeningDelimiter { byte_order_mark: true }
            .to_string()
            .contains("byte-order mark"));
        assert!(Error::MissingClosingDelimiter.to_string().contains("not closed"));
        assert!(Error::InvalidYaml { message: "boom".to_string(), line: 3 }
            .to_string()
            .contains("line 3"));
        assert!(Error::RootNotMapping.to_string().contains("mapping"));
    }
}
