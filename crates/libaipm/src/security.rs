//! Source trust and allowlist enforcement.
//!
//! Controls which plugin sources are permitted.  In development, all sources
//! are allowed with a warning for untrusted ones.  In CI/CD, enforcement can
//! be enabled to reject non-allowlisted sources.
//!
//! Configuration lives in `~/.aipm/config.toml`:
//!
//! ```toml
//! [security]
//! allowed_sources = ["github.com/my-org/*", "git.company.com/*"]
//! enforce_allowlist = false
//! ```
//!
//! The `AIPM_ENFORCE_ALLOWLIST=1` environment variable overrides
//! `enforce_allowlist` to `true`.

/// Name of the environment variable that forces allowlist enforcement.
const ENFORCE_ENV_VAR: &str = "AIPM_ENFORCE_ALLOWLIST";

/// Check whether a source URL is allowed by the given allowlist patterns.
///
/// - Local sources (empty URL or `local:` prefix) are always allowed.
/// - Registry sources are always allowed (trusted by definition).
/// - If enforcement is active and the source does not match, returns `Err`.
/// - If enforcement is not active and the source does not match, returns `Ok`
///   (callers should emit a warning).
pub fn check_source_allowed(
    source_url: &str,
    allowed_patterns: &[String],
    enforce: bool,
) -> Result<AllowResult, Error> {
    // Local sources are always trusted
    if source_url.is_empty() || source_url.starts_with("local:") {
        return Ok(AllowResult::Allowed);
    }

    // Check enforcement override from environment
    let enforcing = enforce || is_env_enforced();

    // Check against allowlist patterns
    if matches_any_pattern(source_url, allowed_patterns) {
        return Ok(AllowResult::Allowed);
    }

    // Empty allowlist with no enforcement: everything is allowed
    if allowed_patterns.is_empty() && !enforcing {
        return Ok(AllowResult::Allowed);
    }

    if enforcing {
        Err(Error::SourceNotAllowed {
            url: source_url.to_string(),
            allowed: allowed_patterns.to_vec(),
        })
    } else {
        Ok(AllowResult::Warned { url: source_url.to_string() })
    }
}

/// Result of an allowlist check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllowResult {
    /// Source is explicitly allowed.
    Allowed,
    /// Source is not in allowlist but enforcement is off — callers should warn.
    Warned { url: String },
}

/// Errors from source security checks.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Source URL is not in the allowlist and enforcement is active.
    #[error(
        "Source '{url}' is not allowed.\nAllowed sources: {allowed:?}\n\
         This restriction is enforced by AIPM_ENFORCE_ALLOWLIST=1 or config enforce_allowlist=true."
    )]
    SourceNotAllowed { url: String, allowed: Vec<String> },
}

/// Check if the environment variable forces enforcement.
fn is_env_enforced() -> bool {
    std::env::var(ENFORCE_ENV_VAR).ok().is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

/// Check if a URL matches any of the given glob-style patterns.
///
/// Patterns use `*` as a wildcard matching any sequence of characters.
/// Matching is case-insensitive.
fn matches_any_pattern(url: &str, patterns: &[String]) -> bool {
    let url_lower = url.to_lowercase();
    patterns.iter().any(|pattern| {
        let pattern_lower = pattern.to_lowercase();
        glob_match(&pattern_lower, &url_lower)
    })
}

/// Simple glob matching: `*` matches any sequence of characters.
fn glob_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();

    if parts.len() == 1 {
        // No wildcards — exact match
        return pattern == text;
    }

    let mut pos = 0;

    // First part must match at the start
    if let Some(first) = parts.first() {
        if !first.is_empty() {
            if !text.starts_with(*first) {
                return false;
            }
            pos = first.len();
        }
    }

    // Last part must match at the end
    if let Some(last) = parts.last() {
        if !last.is_empty() && !text.ends_with(*last) {
            return false;
        }
    }

    // Middle parts must appear in order
    for part in parts.iter().skip(1) {
        if part.is_empty() {
            continue;
        }
        if let Some(found) = text.get(pos..).and_then(|t| t.find(*part)) {
            pos += found + part.len();
        } else {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_source_matches_pattern() {
        let patterns = vec!["github.com/my-org/*".to_string()];
        let result = check_source_allowed("github.com/my-org/repo", &patterns, false);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap_or(AllowResult::Warned { url: String::new() }),
            AllowResult::Allowed
        );
    }

    #[test]
    fn unknown_source_allowed_when_not_enforced() {
        let patterns = vec!["github.com/my-org/*".to_string()];
        let result = check_source_allowed("github.com/other-org/repo", &patterns, false);
        assert!(result.is_ok());
        let result = result.unwrap_or(AllowResult::Allowed);
        assert!(matches!(result, AllowResult::Warned { .. }));
    }

    #[test]
    fn unknown_source_rejected_when_enforced() {
        let patterns = vec!["github.com/my-org/*".to_string()];
        let result = check_source_allowed("github.com/other-org/repo", &patterns, true);
        assert!(result.is_err());
    }

    #[test]
    fn case_insensitive_matching() {
        let patterns = vec!["GitHub.com/My-Org/*".to_string()];
        let result = check_source_allowed("github.com/my-org/repo", &patterns, true);
        assert!(result.is_ok());
    }

    #[test]
    fn local_sources_always_allowed() {
        let patterns = vec!["github.com/my-org/*".to_string()];
        // Empty string = local
        let result = check_source_allowed("", &patterns, true);
        assert!(result.is_ok());
        // local: prefix
        let result = check_source_allowed("local:./my-plugin", &patterns, true);
        assert!(result.is_ok());
    }

    #[test]
    fn empty_allowlist_no_enforcement_allows_all() {
        let patterns: Vec<String> = vec![];
        let result = check_source_allowed("github.com/any/repo", &patterns, false);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap_or(AllowResult::Warned { url: String::new() }),
            AllowResult::Allowed
        );
    }

    #[test]
    fn glob_wildcard_at_end() {
        assert!(glob_match("github.com/*", "github.com/anything/here"));
    }

    #[test]
    fn glob_exact_match() {
        assert!(glob_match("github.com/org/repo", "github.com/org/repo"));
        assert!(!glob_match("github.com/org/repo", "github.com/org/other"));
    }

    #[test]
    fn glob_wildcard_at_start() {
        assert!(glob_match("*/repo", "github.com/repo"));
    }

    #[test]
    fn glob_wildcard_in_middle() {
        assert!(glob_match("github.com/*/repo", "github.com/org/repo"));
    }

    #[test]
    fn glob_double_wildcard() {
        assert!(glob_match("*/*", "a/b"));
    }

    #[test]
    fn glob_no_match_start() {
        assert!(!glob_match("gitlab.com/*", "github.com/repo"));
    }

    #[test]
    fn glob_no_match_end() {
        assert!(!glob_match("*.org", "github.com"));
    }

    #[test]
    fn glob_middle_part_not_found() {
        assert!(!glob_match("a*z*m", "axyz"));
    }

    #[test]
    fn glob_middle_part_absent_when_end_matches() {
        // First part "a" and last part "m" both match, but the middle part "q"
        // is not present — exercises the else-branch in the middle-parts loop.
        assert!(!glob_match("a*q*m", "axyzm"));
    }

    #[test]
    fn empty_allowlist_enforced_rejects() {
        let patterns: Vec<String> = vec![];
        let result = check_source_allowed("github.com/org/repo", &patterns, true);
        assert!(result.is_err());
    }

    #[test]
    fn error_message_contains_source_and_allowed() {
        let patterns = vec!["github.com/trusted/*".to_string()];
        let result = check_source_allowed("github.com/other/repo", &patterns, true);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("github.com/other/repo"));
        assert!(msg.contains("trusted"));
    }
}
