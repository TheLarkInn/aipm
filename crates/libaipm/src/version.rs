//! Semantic versioning utilities for AIPM.
//!
//! Wraps the [`semver`] crate to provide version parsing, requirement matching,
//! and candidate selection for dependency resolution.

use std::fmt;

/// A parsed semantic version (major.minor.patch with optional pre-release and build metadata).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version(semver::Version);

/// A parsed version requirement (caret, tilde, exact, wildcard, or compound range).
#[derive(Debug, Clone)]
pub struct Requirement(semver::VersionReq);

/// Errors that can occur during version parsing.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    /// Failed to parse a version string.
    #[error("invalid semver version: {input}")]
    InvalidVersion {
        /// The input that failed to parse.
        input: String,
    },
    /// Failed to parse a version requirement string.
    #[error("invalid version requirement: {input}")]
    InvalidRequirement {
        /// The input that failed to parse.
        input: String,
    },
}

// =========================================================================
// Version
// =========================================================================

impl Version {
    /// Parse a version string in `MAJOR.MINOR.PATCH[-pre][+build]` format.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidVersion`] if the string is not valid semver.
    pub fn parse(input: &str) -> Result<Self, Error> {
        semver::Version::parse(input)
            .map(Self)
            .map_err(|_| Error::InvalidVersion { input: input.to_string() })
    }

    /// Returns `true` if this version has a pre-release component (e.g. `1.0.0-alpha.1`).
    #[must_use]
    pub fn is_prerelease(&self) -> bool {
        !self.0.pre.is_empty()
    }

    /// Returns the inner `semver::Version` reference.
    #[must_use]
    pub const fn inner(&self) -> &semver::Version {
        &self.0
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

// =========================================================================
// Requirement
// =========================================================================

impl Requirement {
    /// Parse a version requirement string.
    ///
    /// Supports caret (`^1.0`), tilde (`~1.0`), exact (`=1.0.0`),
    /// wildcard (`*`), and compound ranges (`>=1.0, <2.0`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidRequirement`] if the string is not a valid requirement.
    pub fn parse(input: &str) -> Result<Self, Error> {
        semver::VersionReq::parse(input)
            .map(Self)
            .map_err(|_| Error::InvalidRequirement { input: input.to_string() })
    }

    /// Returns `true` if the given version satisfies this requirement.
    #[must_use]
    pub fn matches(&self, version: &Version) -> bool {
        self.0.matches(&version.0)
    }

    /// Select the highest version from `candidates` that satisfies this requirement.
    ///
    /// Pre-release versions are excluded unless the requirement itself targets
    /// a pre-release (e.g. `=2.0.0-alpha.1`). This follows Cargo's behavior:
    /// `^1.0.0` will not match `2.0.0-alpha.1`.
    #[must_use]
    pub fn select_best<'a>(&self, candidates: &'a [Version]) -> Option<&'a Version> {
        candidates.iter().filter(|v| !v.is_prerelease()).filter(|v| self.matches(v)).max()
    }

    /// Returns the inner `semver::VersionReq` reference.
    #[must_use]
    pub const fn inner(&self) -> &semver::VersionReq {
        &self.0
    }
}

impl fmt::Display for Requirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- Version parsing ---

    #[test]
    fn parse_valid_versions() {
        assert!(Version::parse("0.1.0").is_ok());
        assert!(Version::parse("1.0.0").is_ok());
        assert!(Version::parse("1.2.3").is_ok());
        assert!(Version::parse("0.0.1").is_ok());
    }

    #[test]
    fn parse_prerelease_versions() {
        let v = Version::parse("1.0.0-alpha.1").ok();
        assert!(v.is_some());
        assert!(v.as_ref().is_some_and(Version::is_prerelease));

        let v = Version::parse("2.0.0-beta.3+build").ok();
        assert!(v.is_some());
        assert!(v.as_ref().is_some_and(Version::is_prerelease));
    }

    #[test]
    fn reject_invalid_versions() {
        assert!(Version::parse("1").is_err());
        assert!(Version::parse("1.0").is_err());
        assert!(Version::parse("v1.0.0").is_err());
        assert!(Version::parse("latest").is_err());
        assert!(Version::parse("1.0.0.0").is_err());
    }

    // --- Version ordering ---

    #[test]
    fn version_ordering() {
        let v1 = Version::parse("1.0.0").ok();
        let v2 = Version::parse("1.1.0").ok();
        let v3 = Version::parse("2.0.0").ok();
        assert!(v1 < v2);
        assert!(v2 < v3);
    }

    #[test]
    fn prerelease_ordering() {
        let stable = Version::parse("1.0.0").ok();
        let pre = Version::parse("1.0.0-alpha.1").ok();
        // Pre-release is less than stable with same version
        assert!(pre < stable);
    }

    // --- Requirement parsing ---

    #[test]
    fn parse_caret_req() {
        assert!(Requirement::parse("^1.0").is_ok());
        assert!(Requirement::parse("^1.2.3").is_ok());
        assert!(Requirement::parse("^0.2.3").is_ok());
    }

    #[test]
    fn parse_tilde_req() {
        assert!(Requirement::parse("~1.0").is_ok());
        assert!(Requirement::parse("~1.2.3").is_ok());
    }

    #[test]
    fn parse_exact_req() {
        assert!(Requirement::parse("=1.0.0").is_ok());
        assert!(Requirement::parse("=0.2.3").is_ok());
    }

    #[test]
    fn parse_wildcard_req() {
        assert!(Requirement::parse("*").is_ok());
    }

    #[test]
    fn parse_compound_range() {
        assert!(Requirement::parse(">=1.0, <2.0").is_ok());
        assert!(Requirement::parse(">=1.0.0, <2.0.0").is_ok());
    }

    #[test]
    fn reject_invalid_reqs() {
        assert!(Requirement::parse("???").is_err());
        assert!(Requirement::parse("not-a-version").is_err());
        assert!(Requirement::parse("").is_err());
    }

    // --- Requirement matching ---

    /// Helper: assert requirement matches a version string.
    fn assert_matches(req_str: &str, ver_str: &str) {
        let req = Requirement::parse(req_str);
        let ver = Version::parse(ver_str);
        assert!(req.is_ok(), "failed to parse requirement: {req_str}");
        assert!(ver.is_ok(), "failed to parse version: {ver_str}");
        if let (Ok(r), Ok(v)) = (&req, &ver) {
            assert!(r.matches(v), "expected '{req_str}' to match '{ver_str}'");
        }
    }

    /// Helper: assert requirement does NOT match a version string.
    fn assert_no_match(req_str: &str, ver_str: &str) {
        let req = Requirement::parse(req_str);
        let ver = Version::parse(ver_str);
        assert!(req.is_ok(), "failed to parse requirement: {req_str}");
        assert!(ver.is_ok(), "failed to parse version: {ver_str}");
        if let (Ok(r), Ok(v)) = (&req, &ver) {
            assert!(!r.matches(v), "expected '{req_str}' to NOT match '{ver_str}'");
        }
    }

    #[test]
    fn caret_matches() {
        assert_matches("^1.2.3", "1.3.0");
        assert_no_match("^1.2.3", "2.0.0");
    }

    #[test]
    fn tilde_matches() {
        assert_matches("~1.2.3", "1.2.5");
        assert_no_match("~1.2.3", "1.3.0");
    }

    #[test]
    fn exact_matches() {
        assert_matches("=1.2.3", "1.2.3");
        assert_no_match("=1.2.3", "1.2.4");
    }

    #[test]
    fn wildcard_matches_everything() {
        assert_matches("*", "3.0.0");
        assert_matches("*", "0.0.1");
    }

    #[test]
    fn compound_range_matches() {
        assert_matches(">=1.0, <2.0", "1.5.0");
        assert_no_match(">=1.0, <2.0", "2.0.0");
    }

    // --- Pre-1.0 caret ranges ---

    #[test]
    fn pre_1_0_caret_treats_minor_as_breaking() {
        // ^0.2.3 = >=0.2.3, <0.3.0
        assert_matches("^0.2.3", "0.2.5");
        assert_no_match("^0.2.3", "0.3.0");
    }

    // --- select_best ---

    #[test]
    fn select_best_picks_highest_matching() {
        let req = Requirement::parse("^1.0.0").ok();
        let candidates =
            vec![Version::parse("1.0.0"), Version::parse("1.1.0"), Version::parse("2.0.0-alpha.1")];
        let candidates: Vec<Version> = candidates.into_iter().filter_map(Result::ok).collect();
        let best = req.as_ref().and_then(|r| r.select_best(&candidates));
        assert_eq!(best.map(ToString::to_string).as_deref(), Some("1.1.0"));
    }

    #[test]
    fn select_best_excludes_prerelease() {
        let req = Requirement::parse("^1.0.0").ok();
        let candidates =
            vec![Version::parse("1.0.0"), Version::parse("1.1.0"), Version::parse("2.0.0-alpha.1")];
        let candidates: Vec<Version> = candidates.into_iter().filter_map(Result::ok).collect();
        let best = req.as_ref().and_then(|r| r.select_best(&candidates));
        // Should not select 2.0.0-alpha.1
        assert_ne!(best.map(ToString::to_string).as_deref(), Some("2.0.0-alpha.1"));
    }

    #[test]
    fn select_best_pre_1_0_caret() {
        let req = Requirement::parse("^0.2.3").ok();
        let candidates =
            vec![Version::parse("0.2.3"), Version::parse("0.2.5"), Version::parse("0.3.0")];
        let candidates: Vec<Version> = candidates.into_iter().filter_map(Result::ok).collect();
        let best = req.as_ref().and_then(|r| r.select_best(&candidates));
        assert_eq!(best.map(ToString::to_string).as_deref(), Some("0.2.5"));
    }
}
