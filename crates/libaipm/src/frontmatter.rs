//! Shared YAML frontmatter parser for markdown files.
//!
//! Extracts key-value pairs from YAML frontmatter blocks delimited by `---` lines.
//! Used by both the migrate detectors and lint rules.
//!
//! The parser uses simple line-by-line scanning (no full YAML library) to match
//! the approach used by Claude Code CLI and Copilot CLI, which both use regex-based
//! frontmatter extraction before YAML parsing.

use std::collections::BTreeMap;

/// Parsed frontmatter from a markdown file.
#[derive(Debug, Clone)]
pub struct Frontmatter {
    /// Raw key-value pairs extracted from the frontmatter block.
    /// Multi-line values (like `hooks:` blocks) are joined with newlines.
    pub fields: BTreeMap<String, String>,
    /// Maps each field key to its 1-based line number in the source file.
    pub field_lines: BTreeMap<String, usize>,
    /// 1-based line number where frontmatter starts (the opening `---`).
    pub start_line: usize,
    /// 1-based line number where frontmatter ends (the closing `---`).
    pub end_line: usize,
    /// The body content after the closing `---` delimiter.
    pub body: String,
}

/// Parse YAML frontmatter from a markdown string.
///
/// Returns `Some(Frontmatter)` if a valid frontmatter block is found (opening
/// and closing `---` delimiters). Returns `None` if the content does not start
/// with `---`.
///
/// Returns `Err` if an opening `---` is found but the closing `---` is missing.
///
/// # Errors
///
/// Returns a `String` describing the parse error if the closing delimiter is missing.
pub fn parse(content: &str) -> Result<Option<Frontmatter>, String> {
    // Count leading blank lines to find the opening ---
    let mut start_line: usize = 1;
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Ok(None);
    }

    // Count how many characters were trimmed to find the actual start line
    let leading = &content[..content.len() - trimmed.len()];
    for ch in leading.chars() {
        if ch == '\n' {
            start_line += 1;
        }
    }

    // Skip past the opening --- and any trailing whitespace on that line
    let after_first = &trimmed[3..];
    let rest = after_first.trim_start_matches(['\r', '\n']);
    let lines_skipped_after_delim = after_first.len() - rest.len();
    let newlines_in_skip =
        after_first[..lines_skipped_after_delim].chars().filter(|&c| c == '\n').count();

    let closing = rest.find("\n---");
    let yaml_block = match closing {
        Some(pos) => &rest[..pos],
        None => {
            return Err("no closing --- delimiter found".to_string());
        },
    };

    // Count lines in the yaml block to determine end_line
    let yaml_line_count = yaml_block.lines().count();
    let end_line = start_line + newlines_in_skip + yaml_line_count;

    // Extract body after the closing ---
    let after_yaml = &rest[closing.unwrap_or(0) + 1..]; // skip the \n
    let after_closing_delim = after_yaml.strip_prefix("---").unwrap_or(after_yaml);
    // Skip the rest of the closing --- line (e.g., trailing whitespace/newline)
    let body = after_closing_delim
        .strip_prefix('\r')
        .or(Some(after_closing_delim))
        .unwrap_or(after_closing_delim);
    let body = body.strip_prefix('\n').unwrap_or(body);

    // Parse key-value pairs line by line
    let mut fields = BTreeMap::new();
    let mut field_lines = BTreeMap::new();
    let mut current_key: Option<String> = None;
    let mut current_multiline: Vec<String> = Vec::new();

    // The first yaml content line starts after the opening --- and any
    // newlines stripped between it and the yaml block.
    let yaml_base_line = start_line + newlines_in_skip;

    for (line_index, line) in yaml_block.lines().enumerate() {
        let trimmed_line = line.trim();

        // Check if this is a continuation of a multi-line value (indented)
        if current_key.is_some() && (line.starts_with(' ') || line.starts_with('\t')) {
            let stripped =
                line.strip_prefix("  ").or_else(|| line.strip_prefix('\t')).unwrap_or(line);
            current_multiline.push(stripped.to_string());
            continue;
        }

        // Flush any pending multi-line value
        if let Some(key) = current_key.take() {
            if current_multiline.is_empty() {
                // Key with empty value and no continuation (e.g., `name:`)
                fields.insert(key, String::new());
            } else {
                fields.insert(key, current_multiline.join("\n"));
                current_multiline.clear();
            }
        }

        // Parse key: value
        if let Some(colon_pos) = trimmed_line.find(':') {
            let key = trimmed_line[..colon_pos].trim().to_string();
            let value = trimmed_line[colon_pos + 1..].trim();
            let value = strip_yaml_quotes(value);

            let abs_line = yaml_base_line + line_index;

            field_lines.insert(key.clone(), abs_line);
            if value.is_empty() {
                // This might be a multi-line block (e.g., `hooks:`)
                current_key = Some(key);
            } else {
                fields.insert(key, value.to_string());
            }
        }
    }

    // Flush final multi-line value
    if let Some(key) = current_key.take() {
        if current_multiline.is_empty() {
            // Key with empty value and no continuation (e.g., `name:`)
            fields.insert(key, String::new());
        } else {
            fields.insert(key, current_multiline.join("\n"));
        }
    }

    Ok(Some(Frontmatter { fields, field_lines, start_line, end_line, body: body.to_string() }))
}

/// Strip matching surrounding YAML quote delimiters from a scalar value.
///
/// Handles both double-quoted (`"..."`) and single-quoted (`'...'`) YAML scalars.
/// Returns the inner content if delimiters match, otherwise returns the input unchanged.
pub fn strip_yaml_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    match (bytes.first(), bytes.last()) {
        (Some(b'"'), Some(b'"')) | (Some(b'\''), Some(b'\'')) if bytes.len() >= 2 => {
            &s[1..s.len() - 1]
        },
        _ => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_frontmatter() {
        let content = "---\nname: deploy\ndescription: Deploy app\n---\nbody text";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o).unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        assert_eq!(fm.fields.get("name").map(String::as_str), Some("deploy"));
        assert_eq!(fm.fields.get("description").map(String::as_str), Some("Deploy app"));
        assert_eq!(fm.start_line, 1);
        assert_eq!(fm.body, "body text");
    }

    #[test]
    fn parse_no_frontmatter() {
        let content = "just plain text";
        let result = parse(content);
        assert!(result.is_ok());
        assert!(result.ok().and_then(|o| o).is_none());
    }

    #[test]
    fn parse_missing_closing_delimiter() {
        let content = "---\nname: test\nno closing";
        let result = parse(content);
        assert!(result.is_err());
    }

    #[test]
    fn parse_empty_content() {
        let content = "";
        let result = parse(content);
        assert!(result.is_ok());
        assert!(result.ok().and_then(|o| o).is_none());
    }

    #[test]
    fn parse_multiline_hooks_block() {
        let content = "---\nhooks:\n  PreToolUse: check\n  PostToolUse: done\n---\nbody";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        let hooks = fm.fields.get("hooks").map(String::as_str);
        assert!(hooks.is_some());
        assert!(hooks.unwrap_or_default().contains("PreToolUse: check"));
    }

    #[test]
    fn parse_multiline_hooks_with_tab_indent() {
        let content = "---\nhooks:\n\tPreToolUse: check\n---\nbody";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let hooks = fm.as_ref().and_then(|f| f.fields.get("hooks"));
        assert!(hooks.is_some());
    }

    #[test]
    fn parse_quoted_values_stripped() {
        let content = "---\nname: \"my-skill\"\ndescription: 'A skill'\n---\n";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        assert_eq!(fm.fields.get("name").map(String::as_str), Some("my-skill"));
        assert_eq!(fm.fields.get("description").map(String::as_str), Some("A skill"));
    }

    #[test]
    fn parse_empty_value_key() {
        let content = "---\nname:\ndescription: test\n---\nbody";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        // `name:` with no value and no continuation => key with empty string
        assert_eq!(fm.fields.get("name").map(String::as_str), Some(""));
        assert_eq!(fm.fields.get("description").map(String::as_str), Some("test"));
    }

    #[test]
    fn parse_unknown_keys_preserved() {
        let content = "---\nunknown-key: value\nname: test\n---\nbody";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        assert_eq!(fm.fields.get("unknown-key").map(String::as_str), Some("value"));
        assert_eq!(fm.fields.get("name").map(String::as_str), Some("test"));
    }

    #[test]
    fn parse_hooks_block_exits_on_non_indented_line() {
        let content = "---\nhooks:\n\tPreToolUse: check\nname: my-skill\n---\nbody";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        assert_eq!(fm.fields.get("name").map(String::as_str), Some("my-skill"));
        assert!(fm.fields.get("hooks").is_some());
    }

    #[test]
    fn parse_hooks_single_space_indent_fallback() {
        let content = "---\nhooks:\n PreToolUse: check\n---\nbody";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        assert!(fm.as_ref().and_then(|f| f.fields.get("hooks")).is_some());
    }

    #[test]
    fn parse_disable_model_invocation() {
        let content = "---\ndisable-model-invocation: true\n---\nbody";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        assert_eq!(fm.fields.get("disable-model-invocation").map(String::as_str), Some("true"));
    }

    #[test]
    fn strip_yaml_quotes_double() {
        assert_eq!(strip_yaml_quotes("\"hello\""), "hello");
    }

    #[test]
    fn strip_yaml_quotes_single() {
        assert_eq!(strip_yaml_quotes("'hello'"), "hello");
    }

    #[test]
    fn strip_yaml_quotes_no_quotes() {
        assert_eq!(strip_yaml_quotes("hello"), "hello");
    }

    #[test]
    fn strip_yaml_quotes_empty() {
        assert_eq!(strip_yaml_quotes(""), "");
    }

    #[test]
    fn strip_yaml_quotes_mismatched() {
        assert_eq!(strip_yaml_quotes("\"hello'"), "\"hello'");
    }

    #[test]
    fn parse_hooks_inline_value() {
        let content = "---\nhooks: inline-value\n---\nbody";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        assert_eq!(
            fm.as_ref().and_then(|f| f.fields.get("hooks")).map(String::as_str),
            Some("inline-value")
        );
    }

    #[test]
    fn parse_body_is_content_after_closing() {
        let content = "---\nname: test\n---\nLine 1\nLine 2\n";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        assert!(fm.body.contains("Line 1"));
        assert!(fm.body.contains("Line 2"));
    }

    #[test]
    fn parse_end_line_correct() {
        let content = "---\nname: test\ndescription: hello\n---\nbody";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        assert_eq!(fm.start_line, 1);
        // Two fields on lines 2 and 3, closing --- on line 4
        assert_eq!(fm.end_line, 4);
    }

    #[test]
    fn parse_multiline_key_then_eof_no_continuation() {
        // A key with empty value at the end of frontmatter (no continuation lines)
        let content = "---\nname:\n---\nbody";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        // Key exists with empty value
        assert!(fm.fields.contains_key("name"));
    }

    #[test]
    fn parse_crlf_line_endings() {
        // Windows-style \r\n after closing ---
        let content = "---\r\nname: test\r\n---\r\nbody text";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        assert_eq!(fm.fields.get("name").map(String::as_str), Some("test"));
        assert!(fm.body.contains("body text"));
    }

    #[test]
    fn parse_no_body_after_closing() {
        // Content ends immediately after closing ---
        let content = "---\nname: test\n---";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        assert_eq!(fm.fields.get("name").map(String::as_str), Some("test"));
    }

    #[test]
    fn parse_leading_blank_lines() {
        // Leading whitespace before ---
        let content = "\n\n---\nname: test\n---\nbody";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        // Start line should account for the blank lines
        assert_eq!(fm.start_line, 3);
        assert_eq!(fm.fields.get("name").map(String::as_str), Some("test"));
    }

    #[test]
    fn parse_value_with_colon() {
        // Value containing a colon (e.g., URL or timestamp)
        let content = "---\nname: http://example.com\n---\nbody";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        // Only the first colon splits key:value; rest is the value
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        assert_eq!(fm.fields.get("name").map(String::as_str), Some("http://example.com"));
    }

    #[test]
    fn parse_line_without_colon_ignored() {
        let content = "---\nname: test\njust text no colon\n---\nbody";
        let result = parse(content);
        assert!(result.is_ok());
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        assert_eq!(fm.fields.len(), 1);
        assert_eq!(fm.fields.get("name").map(String::as_str), Some("test"));
    }

    #[test]
    fn field_lines_tracks_each_key_line() {
        let content = "---\nname: deploy\ndescription: Deploy app\nshell: bash\n---\nbody";
        let result = parse(content);
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        // --- is line 1, name is line 2, description is line 3, shell is line 4
        assert_eq!(fm.field_lines.get("name").copied(), Some(2));
        assert_eq!(fm.field_lines.get("description").copied(), Some(3));
        assert_eq!(fm.field_lines.get("shell").copied(), Some(4));
    }

    #[test]
    fn field_lines_accounts_for_leading_blank_lines() {
        let content = "\n\n---\nname: test\n---\nbody";
        let result = parse(content);
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        // Two leading blank lines: --- is line 3, name is line 4
        assert_eq!(fm.start_line, 3);
        assert_eq!(fm.field_lines.get("name").copied(), Some(4));
    }

    #[test]
    fn field_lines_multiline_value_records_starting_line() {
        let content = "---\nhooks:\n  PreToolUse: check\n  PostToolUse: done\n---\nbody";
        let result = parse(content);
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        // hooks: is on line 2 (the key line, not continuation lines)
        assert_eq!(fm.field_lines.get("hooks").copied(), Some(2));
    }

    #[test]
    fn field_lines_empty_frontmatter_no_fields() {
        // Frontmatter with a single key but no field_lines of interest
        let content = "---\nname: test\n---\nbody";
        let result = parse(content);
        let fm = result.ok().and_then(|o| o);
        assert!(fm.is_some());
        let fm = fm.unwrap_or_else(|| Frontmatter {
            fields: BTreeMap::new(),
            field_lines: BTreeMap::new(),
            start_line: 0,
            end_line: 0,
            body: String::new(),
        });
        // field_lines should have exactly the keys that were parsed
        assert_eq!(fm.field_lines.len(), fm.fields.len());
        // Querying a non-existent key returns None
        assert_eq!(fm.field_lines.get("nonexistent"), None);
    }
}
