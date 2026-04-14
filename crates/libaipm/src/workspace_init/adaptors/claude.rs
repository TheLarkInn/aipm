//! Claude Code tool adaptor for `aipm init`.
//!
//! Creates or merges `.claude/settings.json` with `extraKnownMarketplaces`
//! pointing to the `.ai/` local marketplace directory.

use std::path::Path;

use crate::fs::Fs;
use crate::workspace_init::{Error, ToolAdaptor};

/// Configures Claude Code to discover the `.ai/` local marketplace.
pub struct Adaptor;

impl ToolAdaptor for Adaptor {
    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn apply(
        &self,
        dir: &Path,
        no_starter: bool,
        marketplace_name: &str,
        fs: &dyn Fs,
    ) -> Result<bool, Error> {
        let settings_dir = dir.join(".claude");
        let settings_path = settings_dir.join("settings.json");

        fs.create_dir_all(&settings_dir)?;

        let mut settings =
            crate::generate::settings::read_or_create(fs, &settings_path).map_err(|e| {
                Error::JsonParse { path: settings_path.clone(), source: serde_json::Error::io(e) }
            })?;

        // For merge path: reject non-object root
        if !settings.is_object() {
            return Err(Error::JsonParse {
                path: settings_path,
                source: serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "expected JSON object",
                )),
            });
        }

        let mp_changed =
            crate::generate::settings::add_known_marketplace(&mut settings, marketplace_name);

        let ep_changed = if no_starter {
            false
        } else {
            let starter_key = format!("starter-aipm-plugin@{marketplace_name}");
            crate::generate::settings::enable_plugin(&mut settings, &starter_key)
        };

        if mp_changed || ep_changed {
            crate::generate::settings::write(fs, &settings_path, &settings)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Real;

    fn make_temp_dir(name: &str) -> std::path::PathBuf {
        let tmp = std::env::temp_dir().join(format!("aipm-test-claude-{name}"));
        if tmp.exists() {
            let _ = std::fs::remove_dir_all(&tmp);
        }
        std::fs::create_dir_all(&tmp).ok();
        tmp
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn claude_settings_created_fresh() {
        let tmp = make_temp_dir("fresh");
        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp, false, "local-repo-plugins", &Real);
        assert!(result.is_ok_and(|v| v));
        assert!(tmp.join(".claude/settings.json").exists());

        let content = std::fs::read_to_string(tmp.join(".claude/settings.json"));
        assert!(content.is_ok());
        let v: serde_json::Value =
            serde_json::from_str(content.as_deref().unwrap_or("")).ok().unwrap_or_default();

        // extraKnownMarketplaces with correct path
        assert!(v["extraKnownMarketplaces"]["local-repo-plugins"].is_object());
        assert_eq!(v["extraKnownMarketplaces"]["local-repo-plugins"]["source"]["path"], "./.ai");

        // enabledPlugins is a top-level sibling, not nested
        assert!(v["enabledPlugins"].is_object());
        assert_eq!(v["enabledPlugins"]["starter-aipm-plugin@local-repo-plugins"], true);

        cleanup(&tmp);
    }

    #[test]
    fn claude_settings_created_fresh_no_starter() {
        let tmp = make_temp_dir("fresh-no-starter");
        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp, true, "local-repo-plugins", &Real);
        assert!(result.is_ok_and(|v| v));
        assert!(tmp.join(".claude/settings.json").exists());

        let content = std::fs::read_to_string(tmp.join(".claude/settings.json"));
        assert!(content.is_ok());
        let v: serde_json::Value =
            serde_json::from_str(content.as_deref().unwrap_or("")).ok().unwrap_or_default();

        // extraKnownMarketplaces should still be present
        assert!(v["extraKnownMarketplaces"]["local-repo-plugins"].is_object());
        assert_eq!(v["extraKnownMarketplaces"]["local-repo-plugins"]["source"]["path"], "./.ai");

        // enabledPlugins should NOT exist when no_starter is true
        assert!(
            v.get("enabledPlugins").is_none(),
            "enabledPlugins should not exist when no_starter is true"
        );

        cleanup(&tmp);
    }

    #[test]
    fn claude_settings_merge_existing() {
        let tmp = make_temp_dir("merge");
        std::fs::create_dir_all(tmp.join(".claude")).ok();
        std::fs::write(
            tmp.join(".claude/settings.json"),
            "{\"permissions\": {\"allow\": [\"Read\"]}}",
        )
        .ok();

        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp, false, "local-repo-plugins", &Real);
        assert!(result.is_ok_and(|v| v));

        let content = std::fs::read_to_string(tmp.join(".claude/settings.json"));
        assert!(content.is_ok());
        let v: serde_json::Value =
            serde_json::from_str(content.as_deref().unwrap_or("")).ok().unwrap_or_default();

        // Preserves existing content
        assert!(v["permissions"]["allow"].is_array());
        // Adds marketplace
        assert!(v["extraKnownMarketplaces"]["local-repo-plugins"].is_object());
        assert_eq!(v["extraKnownMarketplaces"]["local-repo-plugins"]["source"]["path"], "./.ai");
        // enabledPlugins at top level
        assert_eq!(v["enabledPlugins"]["starter-aipm-plugin@local-repo-plugins"], true);

        cleanup(&tmp);
    }

    #[test]
    fn claude_settings_skip_if_fully_configured() {
        let tmp = make_temp_dir("skip");
        std::fs::create_dir_all(tmp.join(".claude")).ok();
        std::fs::write(
            tmp.join(".claude/settings.json"),
            "{\"extraKnownMarketplaces\": {\"local-repo-plugins\": {\"source\": {\"source\": \"directory\", \"path\": \"./.ai\"}}}, \"enabledPlugins\": {\"starter-aipm-plugin@local-repo-plugins\": true}}",
        ).ok();

        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp, false, "local-repo-plugins", &Real);
        assert!(result.is_ok_and(|v| !v));

        cleanup(&tmp);
    }

    #[test]
    fn claude_settings_adds_enabled_plugins_when_marketplace_exists() {
        let tmp = make_temp_dir("add-enabled");
        std::fs::create_dir_all(tmp.join(".claude")).ok();
        std::fs::write(
            tmp.join(".claude/settings.json"),
            "{\"extraKnownMarketplaces\": {\"local-repo-plugins\": {\"source\": {\"source\": \"directory\", \"path\": \"./.ai\"}}}}",
        ).ok();

        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp, false, "local-repo-plugins", &Real);
        assert!(result.is_ok_and(|v| v));

        let content = std::fs::read_to_string(tmp.join(".claude/settings.json"));
        assert!(content.is_ok());
        let v: serde_json::Value =
            serde_json::from_str(content.as_deref().unwrap_or("")).ok().unwrap_or_default();
        assert_eq!(v["enabledPlugins"]["starter-aipm-plugin@local-repo-plugins"], true);

        cleanup(&tmp);
    }

    #[test]
    fn claude_settings_rejects_invalid_json() {
        let tmp = make_temp_dir("invalid-json");
        std::fs::create_dir_all(tmp.join(".claude")).ok();
        std::fs::write(tmp.join(".claude/settings.json"), "{{invalid json").ok();

        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp, false, "local-repo-plugins", &Real);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("JSON parse")));

        cleanup(&tmp);
    }

    #[test]
    fn claude_settings_rejects_non_object_root() {
        let tmp = make_temp_dir("array-root");
        std::fs::create_dir_all(tmp.join(".claude")).ok();
        std::fs::write(tmp.join(".claude/settings.json"), "[1, 2, 3]").ok();

        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp, false, "local-repo-plugins", &Real);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("expected JSON object")));

        cleanup(&tmp);
    }

    #[test]
    fn claude_settings_handles_non_object_marketplace_value() {
        let tmp = make_temp_dir("bad-ekm");
        std::fs::create_dir_all(tmp.join(".claude")).ok();
        std::fs::write(tmp.join(".claude/settings.json"), r#"{"extraKnownMarketplaces": 42}"#).ok();

        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp, false, "local-repo-plugins", &Real);
        // Should succeed — silently skips non-object mutation, still writes enabledPlugins
        assert!(result.is_ok());

        cleanup(&tmp);
    }

    #[test]
    fn claude_settings_handles_non_object_enabled_plugins() {
        let tmp = make_temp_dir("bad-enabled");
        std::fs::create_dir_all(tmp.join(".claude")).ok();
        std::fs::write(tmp.join(".claude/settings.json"), r#"{"enabledPlugins": "not-an-object"}"#)
            .ok();

        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp, false, "local-repo-plugins", &Real);
        // Should succeed — skips non-object enabledPlugins, still writes marketplace
        assert!(result.is_ok());

        cleanup(&tmp);
    }

    #[test]
    fn claude_settings_no_starter_already_configured() {
        // no_starter=true + marketplace already present → return Ok(false) early (lines 86-88)
        let tmp = make_temp_dir("no-starter-skip");
        std::fs::create_dir_all(tmp.join(".claude")).ok();
        std::fs::write(
            tmp.join(".claude/settings.json"),
            r#"{"extraKnownMarketplaces": {"local-repo-plugins": {"source": {"source": "directory", "path": "./.ai"}}}}"#,
        )
        .ok();

        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp, true, "local-repo-plugins", &Real);
        // Already has marketplace and no_starter=true → nothing to add, return false
        assert!(result.is_ok_and(|v| !v));

        cleanup(&tmp);
    }

    #[test]
    fn make_temp_dir_removes_existing_dir() {
        // Pre-create the temp dir so that `if tmp.exists()` at line 140 takes the True branch.
        let name = "pre-existing";
        let tmp_path = std::env::temp_dir().join(format!("aipm-test-claude-{name}"));
        std::fs::create_dir_all(&tmp_path).ok();
        // Write a sentinel file; make_temp_dir must remove it when cleaning up.
        std::fs::write(tmp_path.join("sentinel.txt"), b"old").ok();

        let tmp = make_temp_dir(name);
        assert!(tmp.exists(), "make_temp_dir should recreate the directory");
        assert!(
            !tmp.join("sentinel.txt").exists(),
            "old contents should be removed by make_temp_dir"
        );

        cleanup(&tmp);
    }

    #[test]
    fn claude_settings_no_starter_merge_adds_marketplace_only() {
        // no_starter=true + no marketplace yet → add marketplace but skip enabledPlugins (line 123)
        let tmp = make_temp_dir("no-starter-add");
        std::fs::create_dir_all(tmp.join(".claude")).ok();
        std::fs::write(
            tmp.join(".claude/settings.json"),
            r#"{"permissions": {"allow": ["Read"]}}"#,
        )
        .ok();

        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp, true, "local-repo-plugins", &Real);
        assert!(result.is_ok_and(|v| v));

        let content = std::fs::read_to_string(tmp.join(".claude/settings.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        // Marketplace added
        assert!(v["extraKnownMarketplaces"]["local-repo-plugins"].is_object());
        // enabledPlugins NOT added (no_starter=true)
        assert!(v.get("enabledPlugins").is_none());

        cleanup(&tmp);
    }
}
