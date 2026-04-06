//! Global installed plugin registry.
//!
//! Manages a persistent list of plugins "installed" globally via
//! `aipm install --global`.  Installed plugins are automatically provided
//! to the engine on every invocation.
//!
//! Registry file: `~/.aipm/installed.json`

use serde::{Deserialize, Serialize};

use crate::cache;
use crate::engine::Engine;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Registry of globally installed plugins.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Registry {
    /// Installed plugin entries.
    pub plugins: Vec<Plugin>,
}

/// A single globally installed plugin entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Plugin {
    /// Plugin spec string (e.g., `github:owner/repo:path@ref`).
    pub spec: String,
    /// Engine restriction.  Empty = all engines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub engines: Vec<String>,
    /// Per-plugin cache policy override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_policy: Option<cache::Policy>,
    /// Per-plugin cache TTL override (seconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_ttl_secs: Option<u64>,
}

impl Plugin {
    /// Derive the plugin folder name from the spec.
    pub fn folder_name(&self) -> Option<String> {
        self.spec.parse::<crate::spec::Spec>().ok().map(|s| s.folder_name())
    }

    /// Check if this plugin applies to the given engine.
    pub fn applies_to(&self, engine: Engine) -> bool {
        if self.engines.is_empty() {
            return true;
        }
        let engine_name = engine.name().to_lowercase();
        self.engines.iter().any(|e| e.to_lowercase() == engine_name)
    }
}

// ---------------------------------------------------------------------------
// Registry operations
// ---------------------------------------------------------------------------

impl Registry {
    /// Add or update a plugin in the registry.
    ///
    /// - Empty `engines` → reset to "all engines".
    /// - Non-empty `engines` on existing plugin → additive merge.
    /// - Checks for folder name conflicts before modifying.
    ///
    /// Returns `Ok(true)` if newly added, `Ok(false)` if updated.
    pub fn install(
        &mut self,
        spec: String,
        engines: &[String],
        cache_policy: Option<cache::Policy>,
        cache_ttl_secs: Option<u64>,
    ) -> Result<bool, Error> {
        let effective_engines = self.effective_engines(&spec, engines);
        self.check_name_conflicts(&spec, &effective_engines)?;

        if let Some(existing) = self.plugins.iter_mut().find(|p| p.spec == spec) {
            existing.engines = effective_engines;
            if let Some(policy) = cache_policy {
                existing.cache_policy = Some(policy);
            }
            if let Some(ttl) = cache_ttl_secs {
                existing.cache_ttl_secs = Some(ttl);
            }
            Ok(false)
        } else {
            self.plugins.push(Plugin {
                spec,
                engines: effective_engines,
                cache_policy,
                cache_ttl_secs,
            });
            Ok(true)
        }
    }

    /// Remove a plugin entirely.  Returns `true` if found and removed.
    pub fn uninstall(&mut self, spec: &str) -> bool {
        let len_before = self.plugins.len();
        self.plugins.retain(|p| p.spec != spec);
        self.plugins.len() < len_before
    }

    /// Remove specific engine(s) from a plugin.
    ///
    /// If "all engines" (`[]`), expands to explicit list first, then removes.
    /// If no engines remain → full uninstall.
    ///
    /// Returns `true` if modified/removed, `false` if not found.
    pub fn uninstall_engine(&mut self, spec: &str, engines_to_remove: &[String]) -> bool {
        let Some(plugin) = self.plugins.iter_mut().find(|p| p.spec == spec) else {
            return false;
        };

        // Expand "all" to explicit list
        if plugin.engines.is_empty() {
            plugin.engines = Engine::all_names().iter().map(|s| (*s).to_string()).collect();
        }

        for engine in engines_to_remove {
            let lower = engine.to_lowercase();
            plugin.engines.retain(|e| e.to_lowercase() != lower);
        }

        // Full uninstall if no engines remain
        if plugin.engines.is_empty() {
            self.plugins.retain(|p| p.spec != spec);
        }

        true
    }

    /// Resolve a plugin identifier to a spec string.
    ///
    /// Supports exact spec match and folder-name shorthand with engine disambiguation.
    pub fn resolve_spec(
        &self,
        identifier: &str,
        engine_filter: &[String],
    ) -> Result<String, Error> {
        if !is_name_shorthand(identifier) {
            if self.plugins.iter().any(|p| p.spec == identifier) {
                return Ok(identifier.to_string());
            }
            return Err(Error::NotFound { identifier: identifier.to_string() });
        }

        // Folder name shorthand (case-insensitive)
        let id_lower = identifier.to_lowercase();
        let mut candidates: Vec<&Plugin> = self
            .plugins
            .iter()
            .filter(|p| p.folder_name().is_some_and(|n| n.to_lowercase() == id_lower))
            .collect();

        if !engine_filter.is_empty() {
            candidates.retain(|p| {
                engine_filter.iter().any(|ef| {
                    let ef_lower = ef.to_lowercase();
                    p.engines.is_empty() || p.engines.iter().any(|e| e.to_lowercase() == ef_lower)
                })
            });
        }

        match candidates.len() {
            0 => Err(Error::NotFound { identifier: identifier.to_string() }),
            1 => Ok(candidates.first().map_or_else(String::new, |p| p.spec.clone())),
            _ => {
                let specs: Vec<String> = candidates.iter().map(|p| p.spec.clone()).collect();
                Err(Error::Ambiguous { identifier: identifier.to_string(), candidates: specs })
            },
        }
    }

    /// Get all installed plugins that apply to the given engine.
    pub fn plugins_for_engine(&self, engine: Engine) -> Vec<&Plugin> {
        self.plugins.iter().filter(|p| p.applies_to(engine)).collect()
    }

    /// Compute effective engine list after install.
    fn effective_engines(&self, spec: &str, engines: &[String]) -> Vec<String> {
        self.plugins.iter().find(|p| p.spec == spec).map_or_else(
            || engines.to_vec(),
            |existing| {
                if engines.is_empty() || existing.engines.is_empty() {
                    vec![]
                } else {
                    let mut merged = existing.engines.clone();
                    for engine in engines {
                        let lower = engine.to_lowercase();
                        if !merged.iter().any(|e| e.to_lowercase() == lower) {
                            merged.push(engine.clone());
                        }
                    }
                    merged
                }
            },
        )
    }

    /// Check for folder name conflicts.
    fn check_name_conflicts(&self, spec: &str, engines: &[String]) -> Result<(), Error> {
        let new_spec: crate::spec::Spec = match spec.parse() {
            Ok(s) => s,
            Err(_) => return Ok(()),
        };
        let new_name = new_spec.folder_name();
        let name_lower = new_name.to_lowercase();

        let conflicts: Vec<&Plugin> = self
            .plugins
            .iter()
            .filter(|p| {
                p.spec != spec && p.folder_name().is_some_and(|n| n.to_lowercase() == name_lower)
            })
            .filter(|p| engines_overlap(engines, &p.engines))
            .collect();

        if conflicts.is_empty() {
            return Ok(());
        }

        let conflict_specs: Vec<String> = conflicts.iter().map(|p| p.spec.clone()).collect();
        Err(Error::NameConflict { name: new_name, existing: conflict_specs })
    }
}

/// A spec always contains `:` or `@`; if neither, treat as name shorthand.
fn is_name_shorthand(identifier: &str) -> bool {
    !identifier.contains(':') && !identifier.contains('@')
}

/// Check whether two engine lists overlap (empty = all engines).
fn engines_overlap(a: &[String], b: &[String]) -> bool {
    if a.is_empty() || b.is_empty() {
        return true;
    }
    a.iter().any(|ae| b.iter().any(|be| ae.eq_ignore_ascii_case(be)))
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from the installed registry.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Plugin not found.
    #[error("Plugin '{identifier}' not found")]
    NotFound { identifier: String },
    /// Ambiguous name shorthand.
    #[error("Ambiguous plugin name '{identifier}', matches: {candidates:?}")]
    Ambiguous { identifier: String, candidates: Vec<String> },
    /// Name conflict with existing plugin.
    #[error("Plugin name conflict for '{name}' with existing: {existing:?}")]
    NameConflict { name: String, existing: Vec<String> },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_new_plugin() {
        let mut registry = Registry::default();
        let added = registry
            .install("github:owner/repo:plugin@main".to_string(), &[], None, None)
            .unwrap_or(false);
        assert!(added);
        assert_eq!(registry.plugins.len(), 1);
    }

    #[test]
    fn install_additive_engines() {
        let mut registry = Registry::default();
        let _ =
            registry.install("local:./my-plugin".to_string(), &["claude".to_string()], None, None);
        let added = registry
            .install("local:./my-plugin".to_string(), &["copilot".to_string()], None, None)
            .unwrap_or(true);
        assert!(!added);
        assert_eq!(registry.plugins.len(), 1);
        assert_eq!(registry.plugins.first().map(|p| p.engines.len()), Some(2));
    }

    #[test]
    fn install_no_engine_resets_to_all() {
        let mut registry = Registry::default();
        let _ =
            registry.install("local:./my-plugin".to_string(), &["claude".to_string()], None, None);
        let _ = registry.install("local:./my-plugin".to_string(), &[], None, None);
        assert!(registry.plugins.first().map_or(false, |p| p.engines.is_empty()));
    }

    #[test]
    fn install_additive_deduplicates() {
        let mut registry = Registry::default();
        let _ =
            registry.install("local:./my-plugin".to_string(), &["claude".to_string()], None, None);
        let _ =
            registry.install("local:./my-plugin".to_string(), &["claude".to_string()], None, None);
        assert_eq!(registry.plugins.first().map(|p| p.engines.len()), Some(1));
    }

    #[test]
    fn install_specific_engine_when_all_is_noop() {
        let mut registry = Registry::default();
        let _ = registry.install("local:./my-plugin".to_string(), &[], None, None);
        let _ =
            registry.install("local:./my-plugin".to_string(), &["claude".to_string()], None, None);
        assert!(registry.plugins.first().map_or(false, |p| p.engines.is_empty()));
    }

    #[test]
    fn uninstall_existing() {
        let mut registry = Registry::default();
        let _ = registry.install("local:./plugin-a".to_string(), &[], None, None);
        let _ = registry.install("local:./plugin-b".to_string(), &[], None, None);
        assert!(registry.uninstall("local:./plugin-a"));
        assert_eq!(registry.plugins.len(), 1);
    }

    #[test]
    fn uninstall_missing() {
        let mut registry = Registry::default();
        assert!(!registry.uninstall("nonexistent"));
    }

    #[test]
    fn uninstall_engine_from_explicit_list() {
        let mut registry = Registry::default();
        let _ = registry.install(
            "local:./my-plugin".to_string(),
            &["claude".to_string(), "copilot".to_string()],
            None,
            None,
        );
        assert!(registry.uninstall_engine("local:./my-plugin", &["copilot".to_string()]));
        assert_eq!(registry.plugins.first().map(|p| p.engines.len()), Some(1));
    }

    #[test]
    fn uninstall_engine_from_all_engines() {
        let mut registry = Registry::default();
        let _ = registry.install("local:./my-plugin".to_string(), &[], None, None);
        assert!(registry.uninstall_engine("local:./my-plugin", &["claude".to_string()]));
        assert_eq!(registry.plugins.len(), 1);
        assert_eq!(
            registry.plugins.first().map(|p| &p.engines),
            Some(&vec!["copilot".to_string()])
        );
    }

    #[test]
    fn uninstall_last_engine_removes_plugin() {
        let mut registry = Registry::default();
        let _ =
            registry.install("local:./my-plugin".to_string(), &["claude".to_string()], None, None);
        assert!(registry.uninstall_engine("local:./my-plugin", &["claude".to_string()]));
        assert!(registry.plugins.is_empty());
    }

    #[test]
    fn uninstall_engine_missing_plugin() {
        let mut registry = Registry::default();
        assert!(!registry.uninstall_engine("nonexistent", &["claude".to_string()]));
    }

    #[test]
    fn applies_to_all_engines() {
        let plugin = Plugin {
            spec: "local:./plugin".to_string(),
            engines: vec![],
            cache_policy: None,
            cache_ttl_secs: None,
        };
        assert!(plugin.applies_to(Engine::Claude));
        assert!(plugin.applies_to(Engine::Copilot));
    }

    #[test]
    fn applies_to_specific_engine() {
        let plugin = Plugin {
            spec: "local:./plugin".to_string(),
            engines: vec!["claude".to_string()],
            cache_policy: None,
            cache_ttl_secs: None,
        };
        assert!(plugin.applies_to(Engine::Claude));
        assert!(!plugin.applies_to(Engine::Copilot));
    }

    #[test]
    fn plugins_for_engine() {
        let mut registry = Registry::default();
        let _ = registry.install("local:./all-engines".to_string(), &[], None, None);
        let _ = registry.install(
            "local:./claude-only".to_string(),
            &["claude".to_string()],
            None,
            None,
        );
        let _ = registry.install(
            "local:./copilot-only".to_string(),
            &["copilot".to_string()],
            None,
            None,
        );
        assert_eq!(registry.plugins_for_engine(Engine::Claude).len(), 2);
        assert_eq!(registry.plugins_for_engine(Engine::Copilot).len(), 2);
    }

    #[test]
    fn serialization_roundtrip() {
        let mut registry = Registry::default();
        let _ = registry.install("local:./plugin-a".to_string(), &[], None, None);
        let _ = registry.install(
            "local:./plugin-b".to_string(),
            &["claude".to_string(), "copilot".to_string()],
            None,
            None,
        );
        let json = serde_json::to_string(&registry).unwrap_or_default();
        let deserialized: Registry = serde_json::from_str(&json).unwrap_or_default();
        assert_eq!(deserialized.plugins.len(), 2);
    }

    #[test]
    fn name_conflict_same_name_all_engines() {
        let mut registry = Registry::default();
        let _ = registry.install("github:owner/repo:my-plugin".to_string(), &[], None, None);
        let result = registry.install("local:./my-plugin".to_string(), &[], None, None);
        assert!(result.is_err());
    }

    #[test]
    fn name_conflict_allowed_non_overlapping_engines() {
        let mut registry = Registry::default();
        let _ = registry.install(
            "github:owner/repo:my-plugin".to_string(),
            &["claude".to_string()],
            None,
            None,
        );
        let result =
            registry.install("local:./my-plugin".to_string(), &["copilot".to_string()], None, None);
        assert!(result.is_ok());
        assert_eq!(registry.plugins.len(), 2);
    }

    #[test]
    fn name_conflict_adding_engine_causes_overlap() {
        let mut registry = Registry::default();
        let _ = registry.install(
            "github:owner/repo:my-plugin".to_string(),
            &["claude".to_string()],
            None,
            None,
        );
        let _ =
            registry.install("local:./my-plugin".to_string(), &["copilot".to_string()], None, None);
        // Adding claude to local spec causes overlap
        let result =
            registry.install("local:./my-plugin".to_string(), &["claude".to_string()], None, None);
        assert!(result.is_err());
    }

    #[test]
    fn name_conflict_all_engines_vs_specific() {
        let mut registry = Registry::default();
        let _ = registry.install("github:owner/repo:my-plugin".to_string(), &[], None, None);
        let result =
            registry.install("local:./my-plugin".to_string(), &["claude".to_string()], None, None);
        assert!(result.is_err());
    }

    #[test]
    fn no_conflict_same_spec_reinstall() {
        let mut registry = Registry::default();
        let _ = registry.install(
            "github:owner/repo:my-plugin".to_string(),
            &["claude".to_string()],
            None,
            None,
        );
        let result = registry.install("github:owner/repo:my-plugin".to_string(), &[], None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn no_conflict_different_names() {
        let mut registry = Registry::default();
        let _ = registry.install("local:./plugin-a".to_string(), &[], None, None);
        let result = registry.install("local:./plugin-b".to_string(), &[], None, None);
        assert!(result.is_ok());
        assert_eq!(registry.plugins.len(), 2);
    }

    #[test]
    fn name_conflict_case_insensitive() {
        let mut registry = Registry::default();
        let _ = registry.install("github:owner/repo:My-Plugin".to_string(), &[], None, None);
        let result = registry.install("local:./my-plugin".to_string(), &[], None, None);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_spec_exact_match() {
        let mut registry = Registry::default();
        let _ = registry.install("local:./my-plugin".to_string(), &[], None, None);
        let resolved = registry.resolve_spec("local:./my-plugin", &[]);
        assert!(resolved.is_ok());
        assert_eq!(resolved.unwrap_or_default(), "local:./my-plugin");
    }

    #[test]
    fn resolve_spec_by_name_unique() {
        let mut registry = Registry::default();
        let _ = registry.install("github:owner/repo:my-plugin".to_string(), &[], None, None);
        let resolved = registry.resolve_spec("my-plugin", &[]);
        assert!(resolved.is_ok());
        assert_eq!(resolved.unwrap_or_default(), "github:owner/repo:my-plugin");
    }

    #[test]
    fn resolve_spec_by_name_ambiguous() {
        let mut registry = Registry::default();
        let _ = registry.install(
            "github:owner/repo:my-plugin".to_string(),
            &["claude".to_string()],
            None,
            None,
        );
        let _ =
            registry.install("local:./my-plugin".to_string(), &["copilot".to_string()], None, None);
        let result = registry.resolve_spec("my-plugin", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_spec_by_name_disambiguated_by_engine() {
        let mut registry = Registry::default();
        let _ = registry.install(
            "github:owner/repo:my-plugin".to_string(),
            &["claude".to_string()],
            None,
            None,
        );
        let _ =
            registry.install("local:./my-plugin".to_string(), &["copilot".to_string()], None, None);
        let resolved = registry.resolve_spec("my-plugin", &["claude".to_string()]);
        assert!(resolved.is_ok());
        assert_eq!(resolved.unwrap_or_default(), "github:owner/repo:my-plugin");
    }

    #[test]
    fn resolve_spec_not_found() {
        let registry = Registry::default();
        let result = registry.resolve_spec("nonexistent", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn engines_overlap_both_empty() {
        assert!(engines_overlap(&[], &[]));
    }

    #[test]
    fn engines_overlap_one_empty() {
        assert!(engines_overlap(&[], &["claude".to_string()]));
        assert!(engines_overlap(&["claude".to_string()], &[]));
    }

    #[test]
    fn engines_overlap_matching() {
        assert!(engines_overlap(&["claude".to_string()], &["claude".to_string()]));
    }

    #[test]
    fn engines_overlap_no_match() {
        assert!(!engines_overlap(&["claude".to_string()], &["copilot".to_string()]));
    }

    #[test]
    fn engines_overlap_case_insensitive() {
        assert!(engines_overlap(&["Claude".to_string()], &["claude".to_string()]));
    }

    #[test]
    fn install_stores_cache_policy() {
        let mut registry = Registry::default();
        let _ = registry.install(
            "local:./my-plugin".to_string(),
            &[],
            Some(cache::Policy::Auto),
            Some(3600),
        );
        assert_eq!(
            registry.plugins.first().and_then(|p| p.cache_policy),
            Some(cache::Policy::Auto)
        );
        assert_eq!(registry.plugins.first().and_then(|p| p.cache_ttl_secs), Some(3600));
    }

    #[test]
    fn reinstall_updates_cache_policy() {
        let mut registry = Registry::default();
        let _ = registry.install("local:./my-plugin".to_string(), &[], None, None);
        let _ = registry.install(
            "local:./my-plugin".to_string(),
            &[],
            Some(cache::Policy::Auto),
            Some(7200),
        );
        assert_eq!(
            registry.plugins.first().and_then(|p| p.cache_policy),
            Some(cache::Policy::Auto)
        );
        assert_eq!(registry.plugins.first().and_then(|p| p.cache_ttl_secs), Some(7200));
    }

    #[test]
    fn reinstall_none_preserves_existing_cache_policy() {
        let mut registry = Registry::default();
        let _ = registry.install(
            "local:./my-plugin".to_string(),
            &[],
            Some(cache::Policy::Auto),
            Some(3600),
        );
        let _ = registry.install("local:./my-plugin".to_string(), &[], None, None);
        assert_eq!(
            registry.plugins.first().and_then(|p| p.cache_policy),
            Some(cache::Policy::Auto)
        );
        assert_eq!(registry.plugins.first().and_then(|p| p.cache_ttl_secs), Some(3600));
    }

    #[test]
    fn cache_policy_serialization_roundtrip() {
        let mut registry = Registry::default();
        let _ = registry.install(
            "local:./my-plugin".to_string(),
            &[],
            Some(cache::Policy::CacheNoRefresh),
            Some(86400),
        );
        let json = serde_json::to_string(&registry).unwrap_or_default();
        let deserialized: Registry = serde_json::from_str(&json).unwrap_or_default();
        assert_eq!(
            deserialized.plugins.first().and_then(|p| p.cache_policy),
            Some(cache::Policy::CacheNoRefresh)
        );
        assert_eq!(deserialized.plugins.first().and_then(|p| p.cache_ttl_secs), Some(86400));
    }

    #[test]
    fn ttl_stored_with_non_auto_policy() {
        let mut registry = Registry::default();
        let _ = registry.install(
            "local:./my-plugin".to_string(),
            &[],
            Some(cache::Policy::CacheNoRefresh),
            Some(3600),
        );
        assert_eq!(
            registry.plugins.first().and_then(|p| p.cache_policy),
            Some(cache::Policy::CacheNoRefresh)
        );
        assert_eq!(registry.plugins.first().and_then(|p| p.cache_ttl_secs), Some(3600));
    }
}
