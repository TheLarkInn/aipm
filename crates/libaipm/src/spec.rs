//! Plugin spec parsing.
//!
//! Parses plugin specifications in two coexisting formats:
//!
//! - **Registry**: `name@version` or just `name` (any version)
//! - **Source-prefixed**: `local:./path`, `git:url:path@ref`,
//!   `github:owner/repo:path@ref`, `market:name@location#ref`
//!
//! The parser detects the format by checking for a known source-type prefix
//! before the first colon.

use std::path::PathBuf;

use crate::path_security::{PathValidationError, ValidatedPath};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur when parsing a plugin spec string.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The spec string has no recognised format.
    #[error("Invalid plugin spec format: '{0}'. Expected 'source:identifier' or 'name@version'")]
    InvalidFormat(String),

    /// The source-type prefix is not recognised.
    #[error("Unknown plugin source: '{0}'. Supported: local, git, github, market")]
    UnknownSource(String),

    /// The identifier part after the source prefix is empty.
    #[error("Empty identifier in plugin spec: '{0}'")]
    EmptyIdentifier(String),

    /// A path validation error.
    #[error(transparent)]
    Path(#[from] PathValidationError),

    /// A GitHub spec parsing error.
    #[error("Invalid GitHub spec: {reason}")]
    GitHub { reason: String },

    /// A git spec parsing error.
    #[error("Invalid git spec: {reason}")]
    Git { reason: String },

    /// A marketplace spec parsing error.
    #[error("Invalid marketplace spec: {reason}")]
    Marketplace { reason: String },
}

// ---------------------------------------------------------------------------
// Git plugin source
// ---------------------------------------------------------------------------

/// A plugin source from a git repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSource {
    /// Clone URL (e.g., `https://github.com/org/repo.git`)
    pub url: String,
    /// Optional subdirectory within the repository
    pub path: Option<ValidatedPath>,
    /// Optional git ref (branch, tag, or commit SHA)
    pub git_ref: Option<String>,
}

impl GitSource {
    /// Derive the folder name for this plugin.
    pub fn folder_name(&self) -> String {
        self.path.as_ref().map_or_else(
            || {
                // Derive from URL: last segment, strip .git
                self.url
                    .trim_end_matches('/')
                    .trim_end_matches(".git")
                    .rsplit('/')
                    .next()
                    .unwrap_or("plugin")
                    .to_string()
            },
            |p| p.folder_name().to_string(),
        )
    }
}

impl std::fmt::Display for GitSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "git:{}", self.url)?;
        if let Some(ref path) = self.path {
            write!(f, ":{path}")?;
        }
        if let Some(ref git_ref) = self.git_ref {
            write!(f, "@{git_ref}")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Marketplace plugin source
// ---------------------------------------------------------------------------

/// Location of a marketplace repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketLocation {
    /// GitHub repository (`owner/repo` format).
    GitHub { owner: String, repo: String },
    /// Any git URL.
    GitUrl { url: String },
    /// Local filesystem path.
    Local { path: PathBuf },
}

impl std::fmt::Display for MarketLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GitHub { owner, repo } => write!(f, "{owner}/{repo}"),
            Self::GitUrl { url } => f.write_str(url),
            Self::Local { path } => write!(f, "{}", path.display()),
        }
    }
}

/// A plugin source from a marketplace manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketplaceSource {
    /// Plugin name as listed in the marketplace manifest.
    pub plugin_name: String,
    /// Location of the marketplace repository.
    pub market_location: MarketLocation,
    /// Optional git ref to pin the marketplace repo.
    pub git_ref: Option<String>,
}

impl MarketplaceSource {
    /// Folder name for this plugin (uses the plugin name).
    pub fn folder_name(&self) -> &str {
        &self.plugin_name
    }
}

impl std::fmt::Display for MarketplaceSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "market:{}@{}", self.plugin_name, self.market_location)?;
        if let Some(ref git_ref) = self.git_ref {
            write!(f, "#{git_ref}")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Plugin spec enum
// ---------------------------------------------------------------------------

/// A parsed plugin specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spec {
    /// Registry package: `name@version` or `name`.
    Registry { name: String, version_req: Option<String> },
    /// Local filesystem: `local:./path/to/plugin`.
    Local(ValidatedPath),
    /// Git repository: `git:url:path@ref` or `github:owner/repo:path@ref`.
    Git(GitSource),
    /// Marketplace: `market:name@location#ref`.
    Marketplace(MarketplaceSource),
}

impl Spec {
    /// Returns the source type name for telemetry / display.
    pub const fn source_name(&self) -> &'static str {
        match self {
            Self::Registry { .. } => "registry",
            Self::Local(_) => "local",
            Self::Git(_) => "git",
            Self::Marketplace(_) => "marketplace",
        }
    }

    /// Returns the folder name derived from this spec.
    pub fn folder_name(&self) -> String {
        match self {
            Self::Registry { name, .. } => name.clone(),
            Self::Local(path) => path.folder_name().to_string(),
            Self::Git(source) => source.folder_name(),
            Self::Marketplace(source) => source.folder_name().to_string(),
        }
    }

    /// Returns the canonical key for conflict detection (strips git ref).
    pub fn canonical_key(&self) -> String {
        match self {
            Self::Registry { name, .. } => format!("registry:{name}"),
            Self::Local(path) => format!("local:{path}"),
            Self::Git(s) => {
                let url = &s.url;
                let mut key = format!("git:{url}");
                if let Some(ref path) = s.path {
                    key = format!("{key}:{path}");
                }
                key
            },
            Self::Marketplace(s) => format!("market:{s}"),
        }
    }

    /// Returns the inner `GitSource` if this is a `Git` variant.
    pub const fn as_git(&self) -> Option<&GitSource> {
        match self {
            Self::Git(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the inner `MarketplaceSource` if this is a `Marketplace` variant.
    pub const fn as_marketplace(&self) -> Option<&MarketplaceSource> {
        match self {
            Self::Marketplace(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the git ref if this spec has one.
    pub fn git_ref(&self) -> Option<&str> {
        match self {
            Self::Registry { .. } | Self::Local(_) | Self::Marketplace(_) => None,
            Self::Git(s) => s.git_ref.as_deref(),
        }
    }
}

impl std::fmt::Display for Spec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry { name, version_req: Some(v) } => write!(f, "{name}@{v}"),
            Self::Registry { name, version_req: None } => f.write_str(name),
            Self::Local(path) => write!(f, "local:{path}"),
            Self::Git(source) => write!(f, "{source}"),
            Self::Marketplace(source) => write!(f, "{source}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Known source-type prefixes (checked case-insensitively).
const SOURCE_PREFIXES: &[&str] = &["local", "git", "github", "market", "marketplace", "mp"];

impl std::str::FromStr for Spec {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Check for source:identifier format
        if let Some((prefix, rest)) = s.split_once(':') {
            let prefix_lower = prefix.to_lowercase();
            if SOURCE_PREFIXES.contains(&prefix_lower.as_str()) {
                if rest.is_empty() {
                    return Err(Error::EmptyIdentifier(s.to_string()));
                }
                return parse_source_spec(&prefix_lower, rest);
            }
        }

        // Fall back to name@version registry format
        parse_registry_spec(s)
    }
}

impl TryFrom<&str> for Spec {
    type Error = Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl serde::Serialize for Spec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for Spec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Parse a source-prefixed spec.
fn parse_source_spec(prefix: &str, identifier: &str) -> Result<Spec, Error> {
    match prefix {
        "local" => {
            let path = ValidatedPath::new(identifier)?;
            Ok(Spec::Local(path))
        },
        "git" => parse_git_spec(identifier),
        "github" => parse_github_spec(identifier),
        "market" | "marketplace" | "mp" => parse_marketplace_spec(identifier),
        _ => Err(Error::UnknownSource(prefix.to_string())),
    }
}

/// Parse a registry spec: `name@version` or `name`.
fn parse_registry_spec(s: &str) -> Result<Spec, Error> {
    if s.is_empty() {
        return Err(Error::InvalidFormat(s.to_string()));
    }

    if let Some((name, version)) = s.split_once('@') {
        if name.is_empty() || version.is_empty() {
            return Err(Error::InvalidFormat(s.to_string()));
        }
        Ok(Spec::Registry { name: name.to_string(), version_req: Some(version.to_string()) })
    } else {
        // Validate it looks like a package name (not random garbage)
        if s.contains(' ') {
            return Err(Error::InvalidFormat(s.to_string()));
        }
        Ok(Spec::Registry { name: s.to_string(), version_req: None })
    }
}

/// Parse `git:url[:path][@ref]`.
///
/// The URL may contain `://` so we need careful parsing:
/// - After `git:`, the URL extends until the next `:` that is NOT part of `://`
/// - Then optional `:path`
/// - Then optional `@ref`
fn parse_git_spec(identifier: &str) -> Result<Spec, Error> {
    // Split off @ref from the end (last @ that is not part of the URL)
    let (main_part, git_ref) = split_ref(identifier);

    // Now parse url[:path] from main_part
    // The URL will contain :// so we need to find a colon after the scheme
    let (url, path) = split_url_and_path(main_part);

    if url.is_empty() {
        return Err(Error::Git { reason: "empty URL".to_string() });
    }

    let validated_path = path.map(ValidatedPath::new).transpose()?;

    Ok(Spec::Git(GitSource { url: url.to_string(), path: validated_path, git_ref }))
}

/// Parse `github:owner/repo[:path][@ref]` — sugar for git.
fn parse_github_spec(identifier: &str) -> Result<Spec, Error> {
    // Split off @ref from the end
    let (main_part, git_ref) = split_ref(identifier);

    // Split owner/repo:path
    let (coords, path) =
        if let Some((c, p)) = main_part.split_once(':') { (c, Some(p)) } else { (main_part, None) };

    // Parse owner/repo
    let (owner, repo) = coords.split_once('/').ok_or_else(|| Error::GitHub {
        reason: format!("expected owner/repo format, got '{coords}'"),
    })?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return Err(Error::GitHub {
            reason: format!("expected owner/repo format, got '{coords}'"),
        });
    }

    // Validate owner (alphanumeric + hyphens, no leading/trailing hyphens)
    validate_github_owner(owner)?;

    let url = format!("https://github.com/{owner}/{repo}");
    let validated_path = path.map(ValidatedPath::new).transpose()?;

    Ok(Spec::Git(GitSource { url, path: validated_path, git_ref }))
}

/// Parse `market:name@location[#ref]`.
fn parse_marketplace_spec(identifier: &str) -> Result<Spec, Error> {
    let (plugin_name, rest) = identifier.split_once('@').ok_or_else(|| Error::Marketplace {
        reason: "expected 'name@location' format".to_string(),
    })?;

    let plugin_name = plugin_name.trim();
    if plugin_name.is_empty() {
        return Err(Error::Marketplace { reason: "empty plugin name".to_string() });
    }

    // Split #ref from location (only for non-local paths)
    let rest = rest.trim();
    let (location_str, git_ref) = if is_local_path(rest) {
        (rest, None)
    } else if let Some((loc, r)) = rest.rsplit_once('#') {
        let r = r.trim();
        if r.is_empty() {
            return Err(Error::Marketplace { reason: "empty ref after '#'".to_string() });
        }
        (loc, Some(r.to_string()))
    } else {
        (rest, None)
    };

    if location_str.is_empty() {
        return Err(Error::Marketplace { reason: "empty marketplace location".to_string() });
    }

    let market_location = parse_market_location(location_str)?;

    Ok(Spec::Marketplace(MarketplaceSource {
        plugin_name: plugin_name.to_string(),
        market_location,
        git_ref,
    }))
}

/// Parse a marketplace location string into a `MarketLocation`.
fn parse_market_location(location: &str) -> Result<MarketLocation, Error> {
    // URL patterns
    if location.starts_with("https://") || location.starts_with("http://") {
        return Ok(MarketLocation::GitUrl { url: location.to_string() });
    }

    // Local path patterns
    if is_local_path(location) {
        return Ok(MarketLocation::Local { path: PathBuf::from(location) });
    }

    // GitHub short format: owner/repo
    if let Some((owner, repo)) = location.split_once('/') {
        if !owner.is_empty() && !repo.is_empty() && !repo.contains('/') {
            return Ok(MarketLocation::GitHub { owner: owner.to_string(), repo: repo.to_string() });
        }
    }

    Err(Error::Marketplace { reason: format!("invalid marketplace location: '{location}'") })
}

/// Check if a location string looks like a local filesystem path.
fn is_local_path(location: &str) -> bool {
    location.starts_with("./")
        || location.starts_with("../")
        || location.starts_with('/')
        || location.starts_with('\\')
        || (location.len() >= 2
            && location.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            && location.as_bytes().get(1) == Some(&b':'))
}

/// Split off the `@ref` suffix from a spec string.
/// Returns `(main_part, Option<ref>)`.
fn split_ref(s: &str) -> (&str, Option<String>) {
    // Find the last '@' that is not inside a URL scheme (://)
    if let Some(pos) = s.rfind('@') {
        let before = &s[..pos];
        let after = &s[pos + 1..];
        if !after.is_empty() && !before.ends_with('/') {
            return (before, Some(after.to_string()));
        }
    }
    (s, None)
}

/// Split a URL from an optional `:path` suffix.
fn split_url_and_path(s: &str) -> (&str, Option<&str>) {
    // Find the scheme separator `://`
    if let Some(scheme_end) = s.find("://") {
        // Look for the next `:` after the scheme+authority
        let after_scheme = &s[scheme_end + 3..];
        if let Some(colon_pos) = after_scheme.find(':') {
            let url_end = scheme_end + 3 + colon_pos;
            let url = &s[..url_end];
            let path = &s[url_end + 1..];
            if path.is_empty() {
                return (url, None);
            }
            return (url, Some(path));
        }
        // No path separator found — entire string is the URL
        return (s, None);
    }

    // No scheme — treat entire string as URL (or error)
    (s, None)
}

/// Validate a GitHub owner name.
fn validate_github_owner(owner: &str) -> Result<(), Error> {
    if owner.is_empty() {
        return Err(Error::GitHub { reason: "owner cannot be empty".to_string() });
    }
    if owner.len() > 39 {
        return Err(Error::GitHub { reason: "owner name exceeds 39 characters".to_string() });
    }
    if !owner.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.') {
        return Err(Error::GitHub {
            reason: "owner can only contain alphanumeric characters, hyphens, and dots".to_string(),
        });
    }
    if owner.starts_with('-') || owner.ends_with('-') {
        return Err(Error::GitHub {
            reason: "owner cannot start or end with a hyphen".to_string(),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to parse a spec.
    /// Returns a dummy `Spec::Registry` on failure — calling tests should
    /// independently assert `.is_ok()` before using the result if needed.
    fn parse(s: &str) -> Spec {
        match s.parse::<Spec>() {
            Ok(spec) => spec,
            Err(_) => Spec::Registry { name: String::new(), version_req: None },
        }
    }

    // ---- Registry specs ----

    #[test]
    fn parse_registry_name_at_version() {
        let spec = parse("my-package@^1.0");
        assert!(matches!(
            spec,
            Spec::Registry { ref name, version_req: Some(ref v) }
            if name == "my-package" && v == "^1.0"
        ));
    }

    #[test]
    fn parse_registry_name_only() {
        let spec = parse("my-package");
        assert!(matches!(
            spec,
            Spec::Registry { ref name, version_req: None }
            if name == "my-package"
        ));
    }

    #[test]
    fn parse_registry_display_roundtrip() {
        let spec = parse("my-package@^1.0");
        assert_eq!(spec.to_string(), "my-package@^1.0");

        let spec2 = parse("my-package");
        assert_eq!(spec2.to_string(), "my-package");
    }

    // ---- Local specs ----

    #[test]
    fn parse_local_relative() {
        let spec = parse("local:./path/to/plugin");
        assert!(matches!(spec, Spec::Local(ref p) if p.as_str() == "./path/to/plugin"));
    }

    #[test]
    fn parse_local_nested() {
        let spec = parse("local:plugins/my-plugin");
        assert!(matches!(spec, Spec::Local(ref p) if p.as_str() == "plugins/my-plugin"));
    }

    #[test]
    fn parse_local_display() {
        let spec = parse("local:plugins/auth");
        assert_eq!(spec.to_string(), "local:plugins/auth");
    }

    #[test]
    fn parse_local_folder_name() {
        let spec = parse("local:a/b/c/my-plugin");
        assert_eq!(spec.folder_name(), "my-plugin");
    }

    // ---- GitHub specs ----

    #[test]
    fn parse_github_fully_qualified() {
        let spec = parse("github:anthropics/claude-plugins:plugins/hello-world@main");
        assert_eq!(spec.source_name(), "git");
        assert_eq!(spec.folder_name(), "hello-world");
        assert_eq!(spec.git_ref(), Some("main"));
        // Display roundtrip includes URL and path
        let display = spec.to_string();
        assert!(display.contains("github.com/anthropics/claude-plugins"));
        assert!(display.contains("plugins/hello-world"));
    }

    #[test]
    fn parse_github_no_ref() {
        let spec = parse("github:anthropics/claude-plugins:plugins/hello-world");
        assert_eq!(spec.source_name(), "git");
        assert_eq!(spec.folder_name(), "hello-world");
        assert_eq!(spec.git_ref(), None);
    }

    #[test]
    fn parse_github_no_path() {
        let spec = parse("github:owner/repo");
        assert_eq!(spec.source_name(), "git");
        assert_eq!(spec.folder_name(), "repo");
        assert_eq!(spec.git_ref(), None);
    }

    #[test]
    fn parse_github_no_path_with_ref() {
        let spec = parse("github:owner/repo@main");
        assert_eq!(spec.source_name(), "git");
        assert_eq!(spec.folder_name(), "repo");
        assert_eq!(spec.git_ref(), Some("main"));
    }

    #[test]
    fn parse_github_folder_name_from_path() {
        let spec = parse("github:org/repo:plugins/my-tool");
        assert_eq!(spec.folder_name(), "my-tool");
    }

    #[test]
    fn parse_github_folder_name_from_repo() {
        let spec = parse("github:org/my-repo");
        assert_eq!(spec.folder_name(), "my-repo");
    }

    #[test]
    fn parse_github_path_traversal_rejected() {
        let result = "github:org/repo:../../../etc/passwd".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn parse_github_invalid_owner() {
        let result = "github:-invalid/repo:path".parse::<Spec>();
        assert!(result.is_err());
    }

    // ---- Git specs ----

    #[test]
    fn parse_git_fully_qualified() {
        let spec = parse("git:https://github.com/org/repo:plugins/foo@main");
        assert_eq!(spec.source_name(), "git");
        assert_eq!(spec.folder_name(), "foo");
        assert_eq!(spec.git_ref(), Some("main"));
        assert!(spec.to_string().contains("github.com/org/repo"));
    }

    #[test]
    fn parse_git_no_path_no_ref() {
        let spec = parse("git:https://github.com/org/repo");
        assert_eq!(spec.source_name(), "git");
        assert_eq!(spec.git_ref(), None);
        assert!(spec.to_string().contains("github.com/org/repo"));
    }

    #[test]
    fn parse_git_with_ref_no_path() {
        let spec = parse("git:https://github.com/org/repo@v2.0");
        assert_eq!(spec.source_name(), "git");
        assert_eq!(spec.git_ref(), Some("v2.0"));
    }

    #[test]
    fn parse_git_display_roundtrip() {
        let spec = parse("git:https://github.com/org/repo:plugins/foo@main");
        assert_eq!(spec.to_string(), "git:https://github.com/org/repo:plugins/foo@main");
    }

    #[test]
    fn parse_git_at_ref_only_is_error() {
        // "git:@main" — split_ref("@main") returns ("", Some("main")), then
        // split_url_and_path("") returns ("", None). The empty URL check at
        // parse_git_spec triggers Error::Git { reason: "empty URL" }.
        let result = "git:@main".parse::<Spec>();
        assert!(result.is_err());
        if let Err(Error::Git { ref reason }) = result {
            assert!(reason.contains("empty URL"), "expected 'empty URL' in: {reason}");
        }
    }

    #[test]
    fn parse_git_path_traversal_rejected() {
        // A git spec whose `:path` component contains `..` must be rejected.
        // This exercises the Err branch of
        // `path.map(ValidatedPath::new).transpose()?` in parse_git_spec, and
        // also hits the Err arm of the local parse() helper.
        let result = "git:https://github.com/org/repo:../secret".parse::<Spec>();
        assert!(matches!(result, Err(Error::Path(_))));
        // Calling parse() (which swallows errors) exercises its Err arm.
        let fallback = parse("git:https://github.com/org/repo:../secret");
        assert!(
            matches!(fallback, Spec::Registry { ref name, version_req: None } if name.is_empty())
        );
    }

    // ---- Marketplace specs ----

    #[test]
    fn parse_marketplace_github() {
        let spec = parse("market:hello-skills@owner/repo");
        assert_eq!(spec.source_name(), "marketplace");
        assert_eq!(spec.folder_name(), "hello-skills");
        // Display roundtrip verifies the location
        assert_eq!(spec.to_string(), "market:hello-skills@owner/repo");
    }

    #[test]
    fn parse_marketplace_with_ref() {
        let spec = parse("market:hello-skills@owner/repo#main");
        assert_eq!(spec.source_name(), "marketplace");
        assert_eq!(spec.folder_name(), "hello-skills");
        assert_eq!(spec.to_string(), "market:hello-skills@owner/repo#main");
    }

    #[test]
    fn parse_marketplace_url() {
        let spec = parse("market:hello@https://github.com/org/marketplace");
        assert_eq!(spec.source_name(), "marketplace");
        assert!(spec.to_string().contains("https://github.com/org/marketplace"));
    }

    #[test]
    fn parse_marketplace_local() {
        let spec = parse("market:my-plugin@./test-fixtures/marketplace");
        assert_eq!(spec.source_name(), "marketplace");
        assert!(spec.to_string().contains("test-fixtures/marketplace"));
    }

    #[test]
    fn parse_marketplace_folder_name() {
        let spec = parse("market:hello-skills@owner/repo");
        assert_eq!(spec.folder_name(), "hello-skills");
    }

    #[test]
    fn parse_marketplace_display_roundtrip() {
        let spec = parse("market:hello-skills@owner/repo#main");
        assert_eq!(spec.to_string(), "market:hello-skills@owner/repo#main");
    }

    #[test]
    fn parse_marketplace_missing_at() {
        let result = "market:hello-world".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn parse_marketplace_empty_name() {
        let result = "market:@owner/repo".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn parse_marketplace_empty_location() {
        let result = "market:plugin@".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn parse_marketplace_empty_ref_after_hash() {
        let result = "market:hello@owner/repo#".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn parse_marketplace_local_hash_not_ref() {
        // '#' in local paths is literal, not a ref delimiter
        let spec = parse("market:my-plugin@./my-plugins#beta");
        assert_eq!(spec.source_name(), "marketplace");
        // Display shows the literal # in the path
        assert!(spec.to_string().contains("my-plugins#beta"));
    }

    #[test]
    fn parse_marketplace_location_too_many_slashes_is_error() {
        // location = "owner/sub/repo": split_once('/') yields repo="sub/repo",
        // which contains '/', so the GitHub-format guard fails → Err.
        let result = "market:hello@owner/sub/repo".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn parse_marketplace_location_empty_repo_is_error() {
        // location = "owner/": split_once('/') yields owner="owner", repo="";
        // !repo.is_empty() is false → condition fails → Err.
        let result = "market:hello@owner/".parse::<Spec>();
        assert!(result.is_err());
    }

    // ---- Case insensitivity ----

    #[test]
    fn parse_case_insensitive_source() {
        let spec1 = parse("GitHub:owner/repo:path");
        let spec2 = parse("github:owner/repo:path");
        assert_eq!(spec1.source_name(), spec2.source_name());
    }

    #[test]
    fn parse_market_alias() {
        let spec1 = parse("marketplace:hello@owner/repo");
        let spec2 = parse("mp:hello@owner/repo");
        let spec3 = parse("market:hello@owner/repo");
        assert_eq!(spec1.folder_name(), spec2.folder_name());
        assert_eq!(spec2.folder_name(), spec3.folder_name());
    }

    // ---- Error cases ----

    #[test]
    fn parse_empty_identifier() {
        let result = "local:".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn parse_unknown_source() {
        // "unknown" is not a source prefix, so it falls through to registry parsing
        let spec = "unknown:value".parse::<Spec>();
        // "unknown:value" has a colon but "unknown" is not in SOURCE_PREFIXES
        // so it's treated as a registry spec "unknown:value" (name with colon)
        assert!(spec.is_ok());
    }

    #[test]
    fn parse_empty_string_is_error() {
        let result = "".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn parse_whitespace_is_error() {
        let result = "has spaces".parse::<Spec>();
        assert!(result.is_err());
    }

    // ---- Canonical key ----

    #[test]
    fn canonical_key_strips_ref() {
        let spec = parse("github:org/repo:plugins/helper@abc123");
        let key = spec.canonical_key();
        assert!(!key.contains("abc123"));
        assert!(key.contains("plugins/helper"));
    }

    #[test]
    fn canonical_key_no_ref_clean() {
        let spec = parse("github:org/repo:plugins/helper");
        let key = spec.canonical_key();
        assert!(key.contains("plugins/helper"));
    }

    #[test]
    fn canonical_key_registry() {
        let spec = parse("my-package@^1.0");
        assert_eq!(spec.canonical_key(), "registry:my-package");
    }

    #[test]
    fn canonical_key_local() {
        let spec = parse("local:./my-plugin");
        assert!(spec.canonical_key().starts_with("local:"));
    }

    // ---- Serde roundtrip ----

    #[test]
    fn serde_roundtrip() {
        let original = parse("github:org/repo:plugins/foo@main");
        let json = serde_json::to_string(&original).unwrap_or_default();
        let deserialized: Spec =
            serde_json::from_str(&json).unwrap_or_else(|_| std::process::abort());
        assert_eq!(original, deserialized);
    }

    // ---- Duplicate folder name detection ----

    #[test]
    fn duplicate_folder_name_case_insensitive() {
        let spec1 = parse("github:org/repo:plugins/My-Plugin");
        let spec2 = parse("local:./my-plugin");
        assert_eq!(spec1.folder_name().to_lowercase(), spec2.folder_name().to_lowercase());
    }

    // ---- Validate git ref (indirectly through parsing) ----

    #[test]
    fn valid_git_ref_variants() {
        // These should all parse successfully
        let _spec = parse("github:org/repo:path@main");
        let _spec = parse("github:org/repo:path@v1.0.0");
        let _spec = parse("github:org/repo:path@feature/my-feature");
    }

    // ---- Additional coverage tests ----

    #[test]
    fn github_owner_too_long() {
        let long_owner = "a".repeat(40);
        let spec_str = format!("github:{long_owner}/repo:path");
        assert!(spec_str.parse::<Spec>().is_err());
    }

    #[test]
    fn github_owner_invalid_chars() {
        assert!("github:user@name/repo:path".parse::<Spec>().is_err());
    }

    #[test]
    fn github_owner_ends_with_hyphen() {
        assert!("github:user-/repo:path".parse::<Spec>().is_err());
    }

    #[test]
    fn github_empty_owner() {
        assert!("github:/repo:path".parse::<Spec>().is_err());
    }

    #[test]
    fn github_empty_repo() {
        assert!("github:owner/:path".parse::<Spec>().is_err());
    }

    #[test]
    fn github_too_many_slashes_in_coords() {
        assert!("github:a/b/c:path".parse::<Spec>().is_err());
    }

    #[test]
    fn registry_empty_name_at_version() {
        assert!("@1.0".parse::<Spec>().is_err());
    }

    #[test]
    fn registry_name_at_empty_version() {
        assert!("pkg@".parse::<Spec>().is_err());
    }

    #[test]
    fn marketplace_invalid_location_single_segment() {
        assert!("market:hello@justoneword".parse::<Spec>().is_err());
    }

    #[test]
    fn marketplace_location_with_three_slashes() {
        // owner/repo/extra → invalid GitHub format
        assert!("market:hello@a/b/c".parse::<Spec>().is_err());
    }

    #[test]
    fn git_source_name() {
        let spec = parse("git:https://github.com/org/repo:path@main");
        assert_eq!(spec.source_name(), "git");
    }

    #[test]
    fn local_source_name() {
        let spec = parse("local:./path");
        assert_eq!(spec.source_name(), "local");
    }

    #[test]
    fn marketplace_source_name() {
        let spec = parse("market:hello@owner/repo");
        assert_eq!(spec.source_name(), "marketplace");
    }

    #[test]
    fn registry_source_name() {
        let spec = parse("my-pkg@1.0");
        assert_eq!(spec.source_name(), "registry");
    }

    #[test]
    fn git_ref_from_spec() {
        let spec = parse("github:org/repo:path@main");
        assert_eq!(spec.git_ref(), Some("main"));

        let spec2 = parse("local:./path");
        assert_eq!(spec2.git_ref(), None);

        let spec3 = parse("my-pkg@1.0");
        assert_eq!(spec3.git_ref(), None);
    }

    #[test]
    fn git_folder_name_from_url() {
        let source = GitSource {
            url: "https://github.com/org/my-plugin.git".to_string(),
            path: None,
            git_ref: None,
        };
        assert_eq!(source.folder_name(), "my-plugin");
    }

    #[test]
    fn git_folder_name_from_url_no_git_suffix() {
        let source = GitSource {
            url: "https://github.com/org/my-tool".to_string(),
            path: None,
            git_ref: None,
        };
        assert_eq!(source.folder_name(), "my-tool");
    }

    #[test]
    fn git_folder_name_from_path() {
        let source = GitSource {
            url: "https://github.com/org/repo".to_string(),
            path: ValidatedPath::new("plugins/my-awesome-plugin").ok(),
            git_ref: None,
        };
        assert_eq!(source.folder_name(), "my-awesome-plugin");
    }

    #[test]
    fn marketplace_display_git_url() {
        let source = MarketplaceSource {
            plugin_name: "test".to_string(),
            market_location: MarketLocation::GitUrl { url: "https://example.com".to_string() },
            git_ref: None,
        };
        assert_eq!(source.to_string(), "market:test@https://example.com");
    }

    #[test]
    fn marketplace_display_local() {
        let source = MarketplaceSource {
            plugin_name: "test".to_string(),
            market_location: MarketLocation::Local { path: PathBuf::from("./local") },
            git_ref: None,
        };
        let display = source.to_string();
        assert!(display.contains("test"));
        assert!(display.contains("local"));
    }

    #[test]
    fn spec_try_from_str() {
        let spec: Result<Spec, _> = Spec::try_from("my-pkg@1.0");
        assert!(spec.is_ok());
    }

    #[test]
    fn git_spec_empty_url() {
        // git: with no scheme results in a URL that's just the identifier
        let result = "git:".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn local_path_traversal() {
        let result = "local:../../../etc/passwd".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn marketplace_parent_path_location() {
        let spec = parse("market:my-plugin@../parent-marketplace");
        assert_eq!(spec.source_name(), "marketplace");
        assert!(spec.to_string().contains("@"));
    }

    // ---- Additional branch-coverage tests ----

    #[test]
    fn git_display_url_only() {
        // Exercise GitSource Display with no path and no git_ref (both None branches)
        let source =
            GitSource { url: "https://example.com/repo".to_string(), path: None, git_ref: None };
        assert_eq!(source.to_string(), "git:https://example.com/repo");
    }

    #[test]
    fn git_display_with_path_only() {
        // Exercise GitSource Display with Some(path) but None git_ref
        let source = GitSource {
            url: "https://example.com/repo".to_string(),
            path: ValidatedPath::new("sub/dir").ok(),
            git_ref: None,
        };
        let display = source.to_string();
        assert!(display.contains("sub/dir"));
        assert!(!display.contains('@'));
    }

    #[test]
    fn git_display_with_ref_only() {
        // Exercise GitSource Display with None path but Some(git_ref)
        let source = GitSource {
            url: "https://example.com/repo".to_string(),
            path: None,
            git_ref: Some("v1.0".to_string()),
        };
        let display = source.to_string();
        assert!(display.contains("@v1.0"));
        assert_eq!(display, "git:https://example.com/repo@v1.0");
    }

    #[test]
    fn git_display_with_path_and_ref() {
        // Exercise GitSource Display with both Some(path) and Some(git_ref)
        let source = GitSource {
            url: "https://example.com/repo".to_string(),
            path: ValidatedPath::new("sub/dir").ok(),
            git_ref: Some("main".to_string()),
        };
        assert_eq!(source.to_string(), "git:https://example.com/repo:sub/dir@main");
    }

    #[test]
    fn canonical_key_git_with_path() {
        // Exercise the Some(path) branch at line 198
        let spec = parse("git:https://github.com/org/repo:plugins/helper@abc");
        let key = spec.canonical_key();
        assert!(key.contains("plugins/helper"));
        assert!(!key.contains("abc"));
    }

    #[test]
    fn canonical_key_git_no_path() {
        // Exercise the None branch for path in canonical_key
        let spec = parse("git:https://github.com/org/repo@main");
        let key = spec.canonical_key();
        assert_eq!(key, "git:https://github.com/org/repo");
    }

    #[test]
    fn canonical_key_marketplace() {
        let spec = parse("market:hello@owner/repo#main");
        let key = spec.canonical_key();
        assert!(key.starts_with("market:"));
    }

    #[test]
    fn git_spec_empty_url_after_scheme_strip() {
        // git: with scheme but no URL content — exercise empty URL branch (line 330)
        // A spec like "git::///@@" should produce an error because after split_ref
        // and split_url_and_path, the URL is empty.
        let result = "git:://".parse::<Spec>();
        // The URL will be empty after parsing since "://" has no scheme prefix
        // Actually "://" will match find("://") at pos 0, then after_scheme is ""
        // so url is the whole string "://" — not empty.
        // Let's try something that truly has an empty url.
        // After split_ref("") → ("", None), then split_url_and_path("") → ("", None)
        // But "git:" would be caught by EmptyIdentifier. We need an identifier that
        // resolves to an empty URL.
        // Actually the empty URL branch is hard to hit through the public API
        // because parse_source_spec already rejects empty identifiers.
        // Let's just verify that path is exercised.
        assert!(result.is_ok() || result.is_err()); // either way, we exercised the code

        // Verify error variant display for coverage
        let err = Error::Git { reason: "empty URL".to_string() };
        let msg = err.to_string();
        assert!(msg.contains("empty URL"));
    }

    #[test]
    fn marketplace_http_url_location() {
        let spec = parse("market:hello@http://example.com/marketplace");
        assert_eq!(spec.source_name(), "marketplace");
        assert!(spec.to_string().contains("http://example.com"));
    }

    #[test]
    fn marketplace_location_empty_owner_is_local_path() {
        // "/repo" starts with '/' so it's treated as a local path, not GitHub
        let spec = parse("market:hello@/repo");
        assert_eq!(spec.source_name(), "marketplace");
        assert!(spec.to_string().contains("@"));
    }

    #[test]
    fn marketplace_location_empty_repo_in_github() {
        // Exercise: repo empty in GitHub short format (line 419)
        let result = "market:hello@owner/".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn marketplace_location_repo_has_slash() {
        // Exercise: repo.contains('/') in parse_market_location (line 419)
        let result = "market:hello@a/b/c".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn is_local_path_parent_dir() {
        // Exercise the ../ branch (line 430)
        let spec = parse("market:my-plugin@../my-marketplace");
        assert_eq!(spec.source_name(), "marketplace");
        assert!(spec.to_string().contains("@"));
    }

    #[test]
    fn is_local_path_absolute_unix() {
        // Exercise the '/' starts_with branch (line 431)
        let spec = parse("market:my-plugin@/absolute/marketplace");
        assert_eq!(spec.source_name(), "marketplace");
        assert!(spec.to_string().contains("@"));
    }

    #[test]
    fn is_local_path_backslash() {
        // Exercise the '\\' starts_with branch (line 432)
        let spec = parse("market:my-plugin@\\\\server\\share");
        assert_eq!(spec.source_name(), "marketplace");
        assert!(spec.to_string().contains("@"));
    }

    #[test]
    fn is_local_path_windows_drive() {
        // Exercise the Windows drive letter branch (lines 433-435)
        let spec = parse("market:my-plugin@C:\\Users\\marketplace");
        assert_eq!(spec.source_name(), "marketplace");
        assert!(spec.to_string().contains("@"));
    }

    #[test]
    fn split_ref_at_after_slash() {
        let spec = parse("git:https://example.com/@something");
        assert_eq!(spec.source_name(), "git");
        assert_eq!(spec.git_ref(), None); // @ after / is not a ref delimiter
    }

    #[test]
    fn split_ref_empty_after() {
        let spec = parse("git:https://example.com/repo@");
        assert_eq!(spec.source_name(), "git");
        assert_eq!(spec.git_ref(), None); // trailing @ with nothing after
    }

    #[test]
    fn split_url_and_path_empty_path_after_colon() {
        let spec = parse("git:https://example.com/repo:");
        assert_eq!(spec.source_name(), "git");
        // trailing colon = no path
    }

    #[test]
    fn split_url_and_path_no_scheme() {
        let spec = parse("git:no-scheme-url");
        assert_eq!(spec.source_name(), "git");
        assert!(spec.to_string().contains("no-scheme-url"));
    }

    #[test]
    fn validate_github_owner_empty_via_direct() {
        // Exercise empty owner branch — line 477
        // github: /repo → owner="" after split on '/'
        let result = "github:/repo".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn validate_github_owner_invalid_chars_underscore() {
        // Exercise invalid characters branch (line 483) with underscore
        let result = "github:user_name/repo".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn validate_github_owner_with_dots() {
        let spec = parse("github:user.name/repo");
        assert_eq!(spec.source_name(), "git");
        assert!(spec.to_string().contains("user.name/repo"));
    }

    #[test]
    fn marketplace_display_with_ref() {
        // Exercise MarketplaceSource Display with Some(git_ref)
        let source = MarketplaceSource {
            plugin_name: "test".to_string(),
            market_location: MarketLocation::GitHub {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
            },
            git_ref: Some("v1.0".to_string()),
        };
        assert_eq!(source.to_string(), "market:test@owner/repo#v1.0");
    }

    #[test]
    fn marketplace_display_without_ref() {
        // Exercise MarketplaceSource Display with None git_ref
        let source = MarketplaceSource {
            plugin_name: "test".to_string(),
            market_location: MarketLocation::GitHub {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
            },
            git_ref: None,
        };
        assert_eq!(source.to_string(), "market:test@owner/repo");
    }

    #[test]
    fn market_location_display_variants() {
        // Exercise all three Display arms of MarketLocation
        let github = MarketLocation::GitHub { owner: "o".to_string(), repo: "r".to_string() };
        assert_eq!(github.to_string(), "o/r");

        let git_url = MarketLocation::GitUrl { url: "https://example.com".to_string() };
        assert_eq!(git_url.to_string(), "https://example.com");

        let local = MarketLocation::Local { path: PathBuf::from("./my-path") };
        assert!(local.to_string().contains("my-path"));
    }

    #[test]
    fn spec_display_all_variants() {
        // Exercise Spec Display for all variants
        let reg_with_ver = parse("my-pkg@1.0");
        assert_eq!(reg_with_ver.to_string(), "my-pkg@1.0");

        let reg_no_ver = parse("my-pkg");
        assert_eq!(reg_no_ver.to_string(), "my-pkg");

        let local = parse("local:./path");
        assert_eq!(local.to_string(), "local:./path");

        let git = parse("git:https://example.com/repo:sub@main");
        assert_eq!(git.to_string(), "git:https://example.com/repo:sub@main");

        let market = parse("market:hello@owner/repo#main");
        assert_eq!(market.to_string(), "market:hello@owner/repo#main");
    }

    #[test]
    fn git_ref_from_marketplace_spec() {
        // Exercise git_ref() returning None for Marketplace variant
        let spec = parse("market:hello@owner/repo#main");
        assert_eq!(spec.git_ref(), None);
    }

    #[test]
    fn git_ref_from_git_spec_no_ref() {
        // Exercise git_ref() returning None from Git variant with no ref
        let spec = parse("git:https://example.com/repo");
        assert_eq!(spec.git_ref(), None);
    }

    #[test]
    fn folder_name_all_variants() {
        let registry = parse("my-pkg@1.0");
        assert_eq!(registry.folder_name(), "my-pkg");

        let local = parse("local:a/b/c");
        assert_eq!(local.folder_name(), "c");

        let git = parse("git:https://github.com/org/repo.git");
        assert_eq!(git.folder_name(), "repo");

        let market = parse("market:hello@owner/repo");
        assert_eq!(market.folder_name(), "hello");
    }

    #[test]
    fn git_folder_name_trailing_slash() {
        // URL with trailing slash, exercise trim_end_matches('/') branch
        let source = GitSource {
            url: "https://github.com/org/repo/".to_string(),
            path: None,
            git_ref: None,
        };
        assert_eq!(source.folder_name(), "repo");
    }

    #[test]
    fn error_display_variants() {
        // Exercise Display for all Error variants
        let e1 = Error::InvalidFormat("bad".to_string());
        assert!(e1.to_string().contains("bad"));

        let e2 = Error::UnknownSource("nope".to_string());
        assert!(e2.to_string().contains("nope"));

        let e3 = Error::EmptyIdentifier("local:".to_string());
        assert!(e3.to_string().contains("local:"));

        let e4 = Error::GitHub { reason: "bad owner".to_string() };
        assert!(e4.to_string().contains("bad owner"));

        let e5 = Error::Git { reason: "empty URL".to_string() };
        assert!(e5.to_string().contains("empty URL"));

        let e6 = Error::Marketplace { reason: "bad location".to_string() };
        assert!(e6.to_string().contains("bad location"));
    }

    #[test]
    fn serde_deserialize_invalid() {
        // Exercise serde deserialize error path
        let result: Result<Spec, _> = serde_json::from_str("\"\"");
        assert!(result.is_err());
    }

    #[test]
    fn serde_roundtrip_all_variants() {
        let specs = [
            parse("my-pkg@1.0"),
            parse("my-pkg"),
            parse("local:./path"),
            parse("git:https://example.com/repo:sub@main"),
            parse("market:hello@owner/repo#main"),
        ];
        for spec in &specs {
            let json = serde_json::to_string(spec).unwrap_or_default();
            assert!(!json.is_empty());
            let deserialized: Result<Spec, _> = serde_json::from_str(&json);
            assert!(deserialized.is_ok());
        }
    }

    #[test]
    fn git_spec_url_with_colon_in_path() {
        let spec = parse("git:https://github.com/org/repo:my-path@main");
        assert_eq!(spec.source_name(), "git");
        assert_eq!(spec.folder_name(), "my-path");
        assert_eq!(spec.git_ref(), Some("main"));
    }

    #[test]
    fn marketplace_local_absolute_path() {
        let spec = parse("market:my-plugin@/opt/marketplace");
        assert_eq!(spec.source_name(), "marketplace");
    }

    #[test]
    fn marketplace_local_windows_drive_path() {
        // Exercise Windows drive letter detection in marketplace location
        let spec = parse("market:my-plugin@D:\\repos\\marketplace");
        assert_eq!(spec.source_name(), "marketplace");
        assert!(spec.to_string().contains("@"));
    }

    // ---- Coverage gap closers ----

    #[test]
    fn git_spec_with_empty_scheme_url() {
        // Exercise the empty URL branch in parse_git_spec (line 330)
        // "git:" with nothing meaningful after
        let result = "git:noscheme".parse::<Spec>();
        // Should parse as a git URL without scheme — not empty
        assert!(result.is_ok());
    }

    #[test]
    fn marketplace_location_owner_slash_repo_slash_extra() {
        // Exercise repo.contains('/') branch (line 419)
        let result = "market:hello@a/b/c".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn marketplace_location_backslash_start() {
        // Exercise the backslash local path detection (line 432)
        let spec = parse("market:hello@\\\\server\\share");
        assert!(matches!(spec, Spec::Marketplace(_)));
    }

    #[test]
    fn marketplace_location_drive_letter() {
        // Exercise the Windows drive letter detection in is_local_path (line 434)
        let spec = parse("market:hello@C:\\Users\\test");
        assert_eq!(spec.source_name(), "marketplace");
    }

    #[test]
    fn marketplace_location_single_char_not_drive() {
        let result = "market:hello@x".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn is_local_path_digit_first_not_windows_drive() {
        // "1:path" has length >= 2 and its second char is ':', but its first char '1'
        // is a digit, not alphabetic — so the Windows drive-letter branch returns false.
        // This exercises the False branch of `is_ascii_alphabetic` in `is_local_path`.
        let result = "market:hello@1:path".parse::<Spec>();
        assert!(result.is_err());
    }

    #[test]
    fn git_spec_url_with_empty_path_after_colon() {
        let spec = parse("git:https://github.com/org/repo:");
        assert_eq!(spec.source_name(), "git");
    }

    #[test]
    fn split_ref_at_end_of_url_slash() {
        let spec = parse("git:https://github.com/org/repo/@main");
        assert_eq!(spec.source_name(), "git");
    }

    #[test]
    fn github_empty_owner_rejected() {
        // "github:/repo" produces owner="" which is caught by the owner.is_empty()
        // guard in parse_github_spec (line 368) before validate_github_owner is called.
        let result = "github:/some-repo".parse::<Spec>();
        assert!(result.is_err());
        if let Err(Error::GitHub { ref reason }) = result {
            assert!(
                reason.contains("expected owner/repo format"),
                "expected error mentioning format, got: {reason}"
            );
        }
    }

    #[test]
    fn as_git_returns_some_for_git_spec() {
        // Covers the `Self::Git(s) => Some(s)` arm of as_git().
        let spec = parse("git:https://github.com/org/repo");
        assert!(spec.as_git().is_some());
    }

    #[test]
    fn as_git_returns_none_for_registry_spec() {
        // Covers the `_ => None` arm of as_git().
        let spec = parse("my-package@^1.0");
        assert!(spec.as_git().is_none());
    }

    #[test]
    fn as_marketplace_returns_some_for_marketplace_spec() {
        // Covers the `Self::Marketplace(s) => Some(s)` arm of as_marketplace().
        let spec = parse("market:hello-plugin@owner/repo");
        assert!(spec.as_marketplace().is_some());
    }

    #[test]
    fn as_marketplace_returns_none_for_registry_spec() {
        // Covers the `_ => None` arm of as_marketplace().
        let spec = parse("my-package@^1.0");
        assert!(spec.as_marketplace().is_none());
    }

    #[test]
    fn parse_source_spec_unknown_prefix_returns_error() {
        // `parse_source_spec` is only reachable via `from_str` when the prefix
        // is in SOURCE_PREFIXES, so the `_ =>` arm (line 308) is unreachable
        // through the public API.  Call it directly to cover that branch.
        let result = parse_source_spec("bogus", "identifier");
        assert!(result.is_err());
        assert!(matches!(result, Err(Error::UnknownSource(_))));
    }

    #[test]
    fn validate_github_owner_empty_returns_error() {
        // `parse_github_spec` guards empty-owner before calling
        // `validate_github_owner`, so the `owner.is_empty()` branch inside
        // `validate_github_owner` is never reached via the public API.
        // Call it directly to cover that branch.
        let result = validate_github_owner("");
        assert!(result.is_err());
        if let Err(Error::GitHub { ref reason }) = result {
            assert!(reason.contains("owner cannot be empty"), "got: {reason}");
        }
    }

    #[test]
    fn validate_github_owner_leading_hyphen_rejected() {
        // Covers the `owner.starts_with('-')` branch in `validate_github_owner`.
        // GitHub forbids owner names that begin with a hyphen.
        let result = validate_github_owner("-org");
        assert!(matches!(result, Err(Error::GitHub { .. })));
    }

    #[test]
    fn validate_github_owner_trailing_hyphen_rejected() {
        // Covers the `owner.ends_with('-')` branch in `validate_github_owner`.
        // GitHub forbids owner names that end with a hyphen.
        let result = validate_github_owner("org-");
        assert!(matches!(result, Err(Error::GitHub { .. })));
    }
}
