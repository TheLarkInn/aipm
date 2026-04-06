//! Platform detection and compatibility checking.
//!
//! Determines whether a plugin's declared platform requirements are satisfied
//! by the current runtime environment.
//!
//! - [`current_platforms`] — detect the current runtime platform
//! - [`check_platform_compatibility`] — verify a plugin is compatible

use serde::Deserialize;

/// A platform identifier for compatibility checking.
///
/// Supports `windows`, `linux`, and `macos`.  Unknown values are preserved
/// as [`Platform::Unknown`] for forward compatibility with newer plugin
/// schemas.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Platform {
    /// Any Windows variant.
    Windows,
    /// Any Linux variant.
    Linux,
    /// Any macOS variant.
    MacOs,
    /// An unrecognised value from a newer schema version.
    Unknown(String),
}

impl Platform {
    /// Parse a lowercase string into a [`Platform`].
    fn from_str_value(s: &str) -> Self {
        match s {
            "windows" => Self::Windows,
            "linux" => Self::Linux,
            "macos" => Self::MacOs,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// Return the canonical string for this platform.
    const fn as_str(&self) -> &str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::MacOs => "macos",
            Self::Unknown(s) => s.as_str(),
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Platform {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::from_str_value(&s))
    }
}

// ---------------------------------------------------------------------------
// Platform detection
// ---------------------------------------------------------------------------

/// Return the [`Platform`] identifier for the current runtime OS.
pub fn current_platforms() -> Vec<Platform> {
    let os = std::env::consts::OS;
    match os {
        "windows" => vec![Platform::Windows],
        "linux" => vec![Platform::Linux],
        "macos" => vec![Platform::MacOs],
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// Compatibility check
// ---------------------------------------------------------------------------

/// Result of a platform compatibility check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compatibility {
    /// The plugin has no platform restrictions (universal).
    Universal,
    /// The current platform is listed as supported.
    Compatible,
    /// The current platform is **not** in the supported list.
    Incompatible {
        /// Platform identifiers declared by the plugin.
        declared: Vec<Platform>,
        /// The current platform identifiers.
        current: Vec<Platform>,
    },
}

/// Check whether the given platform list is compatible with the current
/// runtime.
///
/// - `None` or empty → [`Compatibility::Universal`]
/// - Otherwise, compatible if **any** current platform appears in the list.
pub fn check_platform_compatibility(platforms: Option<&[Platform]>) -> Compatibility {
    let declared = match platforms {
        None | Some([]) => return Compatibility::Universal,
        Some(p) => p,
    };

    let current = current_platforms();
    let compatible = current.iter().any(|c| declared.contains(c));

    if compatible {
        Compatibility::Compatible
    } else {
        Compatibility::Incompatible { declared: declared.to_vec(), current }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_platforms_includes_known_os() {
        let platforms = current_platforms();
        let has_known = platforms
            .iter()
            .any(|p| matches!(p, Platform::Windows | Platform::Linux | Platform::MacOs));
        assert!(has_known, "current_platforms should include a known OS: {platforms:?}");
    }

    #[test]
    fn current_platforms_exactly_one() {
        let platforms = current_platforms();
        assert_eq!(
            platforms.len(),
            1,
            "current_platforms should return exactly one platform, got: {platforms:?}"
        );
    }

    #[test]
    fn compatibility_universal_when_no_platforms() {
        assert_eq!(check_platform_compatibility(None), Compatibility::Universal);
    }

    #[test]
    fn compatibility_universal_when_empty_platforms() {
        assert_eq!(check_platform_compatibility(Some(&[])), Compatibility::Universal);
    }

    #[test]
    fn compatibility_compatible_with_current_os() {
        let current = current_platforms();
        assert_eq!(check_platform_compatibility(Some(&current)), Compatibility::Compatible);
    }

    #[test]
    fn compatibility_incompatible_different_os() {
        let fake_platforms = match std::env::consts::OS {
            "windows" => vec![Platform::Linux],
            "linux" => vec![Platform::MacOs],
            "macos" => vec![Platform::Windows],
            _ => vec![Platform::Windows],
        };
        let result = check_platform_compatibility(Some(&fake_platforms));
        assert!(
            matches!(result, Compatibility::Incompatible { .. }),
            "Expected Incompatible, got {result:?}"
        );
    }

    #[test]
    fn compatibility_unknown_platform_no_match() {
        let result =
            check_platform_compatibility(Some(&[Platform::Unknown("future-only".to_string())]));
        assert!(
            matches!(result, Compatibility::Incompatible { .. }),
            "Expected Incompatible for unknown-only platform, got {result:?}"
        );
    }

    #[test]
    fn platform_display_known() {
        assert_eq!(Platform::Windows.to_string(), "windows");
        assert_eq!(Platform::Linux.to_string(), "linux");
        assert_eq!(Platform::MacOs.to_string(), "macos");
    }

    #[test]
    fn platform_display_unknown() {
        assert_eq!(Platform::Unknown("custom".to_string()).to_string(), "custom");
    }

    #[test]
    fn platform_roundtrip() {
        for input in &["windows", "linux", "macos"] {
            let parsed = Platform::from_str_value(input);
            assert_eq!(parsed.to_string(), *input);
        }
    }

    #[test]
    fn unknown_string_roundtrips() {
        let parsed = Platform::from_str_value("windows-x64");
        assert_eq!(parsed, Platform::Unknown("windows-x64".to_string()));
        assert_eq!(parsed.to_string(), "windows-x64");
    }
}
