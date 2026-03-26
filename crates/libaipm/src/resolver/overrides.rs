//! Override application for the dependency resolver.
//!
//! Overrides from `[overrides]` in the manifest are applied **before** resolution.
//! Three types are supported:
//! - **Global**: Replace the version requirement for a package everywhere.
//! - **Scoped** (`parent>child`): Replace only when the dep is under a named parent.
//! - **Replacement** (`aipm:other@version`): Swap the package name entirely.

use std::collections::BTreeMap;

use super::Dependency;

/// A parsed override rule.
#[derive(Debug, Clone)]
pub enum Override {
    /// Replace the version requirement everywhere in the graph.
    Global {
        /// The package name to override.
        name: String,
        /// The new version requirement.
        req: String,
    },
    /// Replace the version requirement only when the dep is a child of `parent`.
    Scoped {
        /// The parent package name.
        parent: String,
        /// The child package name to override.
        child: String,
        /// The new version requirement.
        req: String,
    },
    /// Replace the package with a different package entirely.
    Replacement {
        /// The original package name to replace.
        original: String,
        /// The replacement package name.
        replacement: String,
        /// The version requirement for the replacement.
        req: String,
    },
}

/// Parse override entries from the manifest `[overrides]` table.
///
/// Returns a list of parsed override rules.
pub fn parse(overrides: &BTreeMap<String, String>) -> Vec<Override> {
    let mut result = Vec::new();
    for (key, value) in overrides {
        result.push(parse_single_override(key, value));
    }
    result
}

/// Parse a single override entry.
fn parse_single_override(key: &str, value: &str) -> Override {
    // Check for scoped override: "parent>child"
    if let Some((parent, child)) = key.split_once('>') {
        return if let Some(replacement_info) = parse_replacement_value(value) {
            // Scoped replacement (unusual but technically valid)
            Override::Replacement {
                original: child.trim().to_string(),
                replacement: replacement_info.0,
                req: replacement_info.1,
            }
        } else {
            Override::Scoped {
                parent: parent.trim().to_string(),
                child: child.trim().to_string(),
                req: value.to_string(),
            }
        };
    }

    // Check for replacement: value starts with "aipm:"
    if let Some(replacement_info) = parse_replacement_value(value) {
        return Override::Replacement {
            original: key.to_string(),
            replacement: replacement_info.0,
            req: replacement_info.1,
        };
    }

    // Global override
    Override::Global { name: key.to_string(), req: value.to_string() }
}

/// Parse a replacement value like `"aipm:fixed-lib@^1.0"`.
///
/// Returns `Some((replacement_name, req))` if the value is a replacement.
fn parse_replacement_value(value: &str) -> Option<(String, String)> {
    let stripped = value.strip_prefix("aipm:")?;
    let (name, req) = stripped.split_once('@')?;
    Some((name.to_string(), req.to_string()))
}

/// Apply overrides to a list of dependencies.
///
/// This modifies the dependencies in place, replacing version requirements
/// or package names according to the override rules.
pub fn apply(deps: &mut [Dependency], overrides: &[Override]) {
    for dep in deps.iter_mut() {
        for ovr in overrides {
            match ovr {
                Override::Global { name, req } => {
                    if dep.name == *name {
                        dep.req.clone_from(req);
                    }
                },
                Override::Scoped { parent, child, req } => {
                    if dep.name == *child && dep.source == *parent {
                        dep.req.clone_from(req);
                    }
                },
                Override::Replacement { original, replacement, req } => {
                    if dep.name == *original {
                        dep.name.clone_from(replacement);
                        dep.req.clone_from(req);
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dep(name: &str, req: &str, source: &str) -> Dependency {
        Dependency {
            name: name.to_string(),
            req: req.to_string(),
            source: source.to_string(),
            features: vec![],
            default_features: true,
        }
    }

    #[test]
    fn parse_global_override() {
        let mut overrides = BTreeMap::new();
        overrides.insert("vulnerable-lib".to_string(), "^2.0.0".to_string());

        let parsed = parse(&overrides);
        assert_eq!(parsed.len(), 1);
        assert!(matches!(&parsed[0], Override::Global { name, req }
            if name == "vulnerable-lib" && req == "^2.0.0"));
    }

    #[test]
    fn parse_scoped_override() {
        let mut overrides = BTreeMap::new();
        overrides.insert("skill-a>common-util".to_string(), "=2.1.0".to_string());

        let parsed = parse(&overrides);
        assert_eq!(parsed.len(), 1);
        assert!(matches!(&parsed[0], Override::Scoped { parent, child, req }
            if parent == "skill-a" && child == "common-util" && req == "=2.1.0"));
    }

    #[test]
    fn parse_replacement_override() {
        let mut overrides = BTreeMap::new();
        overrides.insert("broken-lib".to_string(), "aipm:fixed-lib@^1.0".to_string());

        let parsed = parse(&overrides);
        assert_eq!(parsed.len(), 1);
        assert!(matches!(&parsed[0], Override::Replacement { original, replacement, req }
            if original == "broken-lib" && replacement == "fixed-lib" && req == "^1.0"));
    }

    #[test]
    fn apply_global_override() {
        let overrides = vec![Override::Global {
            name: "vulnerable-lib".to_string(),
            req: "^2.0.0".to_string(),
        }];

        let mut deps = vec![
            make_dep("vulnerable-lib", "^1.0", "skill-a"),
            make_dep("other-lib", "^1.0", "skill-a"),
        ];

        apply(&mut deps, &overrides);
        assert_eq!(deps[0].req, "^2.0.0"); // overridden
        assert_eq!(deps[1].req, "^1.0"); // untouched
    }

    #[test]
    fn apply_scoped_override() {
        let overrides = vec![Override::Scoped {
            parent: "skill-a".to_string(),
            child: "common-util".to_string(),
            req: "=2.1.0".to_string(),
        }];

        let mut deps = vec![
            make_dep("common-util", "^2.0", "skill-a"), // should be overridden
            make_dep("common-util", "^2.0", "skill-b"), // should NOT be overridden
        ];

        apply(&mut deps, &overrides);
        assert_eq!(deps[0].req, "=2.1.0"); // overridden (under skill-a)
        assert_eq!(deps[1].req, "^2.0"); // untouched (under skill-b)
    }

    #[test]
    fn apply_replacement_override() {
        let overrides = vec![Override::Replacement {
            original: "broken-lib".to_string(),
            replacement: "fixed-lib".to_string(),
            req: "^1.0".to_string(),
        }];

        let mut deps = vec![make_dep("broken-lib", "^1.0", "root")];

        apply(&mut deps, &overrides);
        assert_eq!(deps[0].name, "fixed-lib"); // name replaced
        assert_eq!(deps[0].req, "^1.0"); // req replaced
    }

    #[test]
    fn parse_multiple_overrides() {
        let mut overrides = BTreeMap::new();
        overrides.insert("lib-a".to_string(), "^2.0".to_string());
        overrides.insert("parent>child".to_string(), "=1.0.0".to_string());
        overrides.insert("old-pkg".to_string(), "aipm:new-pkg@^3.0".to_string());

        let parsed = parse(&overrides);
        assert_eq!(parsed.len(), 3);
    }

    #[test]
    fn replacement_value_without_at_is_global() {
        // "aipm:something" without @version is not a valid replacement → treated as global
        let mut overrides = BTreeMap::new();
        overrides.insert("pkg".to_string(), "aipm:nope".to_string());

        let parsed = parse(&overrides);
        assert_eq!(parsed.len(), 1);
        assert!(matches!(&parsed[0], Override::Global { .. }));
    }

    #[test]
    fn apply_empty() {
        let mut deps = vec![make_dep("foo", "^1.0", "root")];
        apply(&mut deps, &[]);
        assert_eq!(deps[0].req, "^1.0"); // unchanged
    }
}
