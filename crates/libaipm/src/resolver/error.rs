//! Error types for dependency resolution.

/// Errors that can occur during dependency resolution.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No compatible version found for a dependency.
    #[error("no compatible version found for '{name}' matching {requirement}")]
    NoMatch {
        /// The package name.
        name: String,
        /// The version requirement that could not be satisfied.
        requirement: String,
    },

    /// A version conflict between two requirements for the same package.
    #[error("{0}")]
    Conflict(Box<ConflictDetail>),

    /// Registry lookup failed.
    #[error("registry error: {reason}")]
    Registry {
        /// Description of the registry error.
        reason: String,
    },

    /// Version parsing error.
    #[error("version error: {reason}")]
    Version {
        /// Description of the version error.
        reason: String,
    },
}

/// Detailed information about a version conflict.
#[derive(Debug)]
pub struct ConflictDetail {
    /// The package with conflicting requirements.
    pub name: String,
    /// The already-activated version or requirement.
    pub existing_req: String,
    /// Which package introduced the existing requirement.
    pub existing_source: String,
    /// The new conflicting requirement.
    pub new_req: String,
    /// Which package introduced the new requirement.
    pub new_source: String,
    /// Dependency chain from root to the existing requirement.
    pub existing_chain: Vec<String>,
    /// Dependency chain from root to the new requirement.
    pub new_chain: Vec<String>,
}

impl std::fmt::Display for ConflictDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "version conflict for '{}': {} (from {}) vs {} (from {})",
            self.name, self.existing_req, self.existing_source, self.new_req, self.new_source
        )?;

        if !self.existing_chain.is_empty() || !self.new_chain.is_empty() {
            write!(f, "\n  dependency chains:")?;
            if !self.existing_chain.is_empty() {
                write!(f, "\n    ")?;
                format_chain_to(f, &self.existing_chain)?;
                write!(f, " -> {}", self.name)?;
            }
            if !self.new_chain.is_empty() {
                write!(f, "\n    ")?;
                format_chain_to(f, &self.new_chain)?;
                write!(f, " -> {}", self.name)?;
            }
        }

        Ok(())
    }
}

/// Write a dependency chain as "a -> b -> c" to a formatter.
fn format_chain_to(f: &mut std::fmt::Formatter<'_>, chain: &[String]) -> std::fmt::Result {
    for (i, item) in chain.iter().enumerate() {
        if i > 0 {
            write!(f, " -> ")?;
        }
        write!(f, "{item}")?;
    }
    Ok(())
}

/// Helper to build a dependency chain by tracing the `source` fields.
///
/// Given a package name and a map of `name -> source`, traces back to `"root"`.
pub fn build_chain(
    pkg_name: &str,
    source_map: &std::collections::BTreeMap<String, String>,
) -> Vec<String> {
    let mut chain = Vec::new();
    let mut current = pkg_name.to_string();

    // Trace back through sources to root (limit iterations to prevent infinite loops)
    for _ in 0..100 {
        if let Some(source) = source_map.get(&current) {
            chain.push(source.clone());
            if source == "root" || source == "lockfile" {
                break;
            }
            current.clone_from(source);
        } else {
            break;
        }
    }

    chain.reverse();
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_error_basic_format() {
        let err = Error::Conflict(Box::new(ConflictDetail {
            name: "common-util".to_string(),
            existing_req: "1.0.0".to_string(),
            existing_source: "skill-a".to_string(),
            new_req: "=2.0.0".to_string(),
            new_source: "skill-b".to_string(),
            existing_chain: vec![],
            new_chain: vec![],
        }));

        let msg = err.to_string();
        assert!(msg.contains("common-util"));
        assert!(msg.contains("skill-a"));
        assert!(msg.contains("skill-b"));
    }

    #[test]
    fn conflict_error_with_chains() {
        let err = Error::Conflict(Box::new(ConflictDetail {
            name: "common-util".to_string(),
            existing_req: "1.0.0".to_string(),
            existing_source: "skill-a".to_string(),
            new_req: "=2.0.0".to_string(),
            new_source: "skill-b".to_string(),
            existing_chain: vec!["root".to_string(), "app".to_string(), "skill-a".to_string()],
            new_chain: vec!["root".to_string(), "app".to_string(), "skill-b".to_string()],
        }));

        let msg = err.to_string();
        assert!(msg.contains("dependency chains:"));
        assert!(msg.contains("root -> app -> skill-a -> common-util"));
        assert!(msg.contains("root -> app -> skill-b -> common-util"));
    }

    #[test]
    fn build_chain_simple() {
        let mut source_map = std::collections::BTreeMap::new();
        source_map.insert("skill-a".to_string(), "root".to_string());
        source_map.insert("common-util".to_string(), "skill-a".to_string());

        let chain = build_chain("common-util", &source_map);
        assert_eq!(chain, vec!["root", "skill-a"]);
    }

    #[test]
    fn build_chain_deep() {
        let mut source_map = std::collections::BTreeMap::new();
        source_map.insert("a".to_string(), "root".to_string());
        source_map.insert("b".to_string(), "a".to_string());
        source_map.insert("c".to_string(), "b".to_string());

        let chain = build_chain("c", &source_map);
        assert_eq!(chain, vec!["root", "a", "b"]);
    }

    #[test]
    fn no_match_error_format() {
        let err = Error::NoMatch { name: "foo".to_string(), requirement: "^5.0".to_string() };
        assert_eq!(err.to_string(), "no compatible version found for 'foo' matching ^5.0");
    }

    #[test]
    fn registry_error_format() {
        let err = Error::Registry { reason: "timeout".to_string() };
        assert_eq!(err.to_string(), "registry error: timeout");
    }

    #[test]
    fn version_error_format() {
        let err = Error::Version { reason: "bad semver".to_string() };
        assert_eq!(err.to_string(), "version error: bad semver");
    }

    #[test]
    fn build_chain_lockfile_source_breaks_early() {
        let mut source_map = std::collections::BTreeMap::new();
        source_map.insert("pkg-a".to_string(), "lockfile".to_string());

        let chain = build_chain("pkg-a", &source_map);
        assert_eq!(chain, vec!["lockfile"]);
    }

    #[test]
    fn build_chain_missing_source_breaks() {
        // When source_map doesn't have the package, chain is empty
        let source_map = std::collections::BTreeMap::new();
        let chain = build_chain("nonexistent", &source_map);
        assert!(chain.is_empty());
    }

    #[test]
    fn conflict_error_only_existing_chain() {
        // existing_chain has entries, new_chain is empty — tests the inner if branches
        let err = Error::Conflict(Box::new(ConflictDetail {
            name: "pkg".to_string(),
            existing_req: "1.0.0".to_string(),
            existing_source: "src-a".to_string(),
            new_req: "=2.0.0".to_string(),
            new_source: "src-b".to_string(),
            existing_chain: vec!["root".to_string(), "src-a".to_string()],
            new_chain: vec![],
        }));

        let msg = err.to_string();
        assert!(msg.contains("dependency chains:"));
        assert!(msg.contains("root -> src-a -> pkg"));
    }

    #[test]
    fn conflict_error_only_new_chain() {
        // new_chain has entries, existing_chain is empty
        let err = Error::Conflict(Box::new(ConflictDetail {
            name: "pkg".to_string(),
            existing_req: "1.0.0".to_string(),
            existing_source: "src-a".to_string(),
            new_req: "=2.0.0".to_string(),
            new_source: "src-b".to_string(),
            existing_chain: vec![],
            new_chain: vec!["root".to_string(), "src-b".to_string()],
        }));

        let msg = err.to_string();
        assert!(msg.contains("dependency chains:"));
        assert!(msg.contains("root -> src-b -> pkg"));
    }

    #[test]
    fn format_chain_single_element() {
        // Single-element chain should not include " -> " separators
        let err = Error::Conflict(Box::new(ConflictDetail {
            name: "pkg".to_string(),
            existing_req: "1.0.0".to_string(),
            existing_source: "root".to_string(),
            new_req: "2.0.0".to_string(),
            new_source: "root".to_string(),
            existing_chain: vec!["root".to_string()],
            new_chain: vec![],
        }));

        let msg = err.to_string();
        assert!(msg.contains("root -> pkg"));
    }
}
