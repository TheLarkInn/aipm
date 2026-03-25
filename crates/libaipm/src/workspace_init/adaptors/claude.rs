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

        if fs.exists(&settings_path) {
            return merge_claude_settings(&settings_path, no_starter, marketplace_name, fs);
        }

        fs.create_dir_all(&settings_dir)?;

        let marketplace_entry = serde_json::json!({
            "source": { "source": "directory", "path": "./.ai" }
        });

        let mut ekm = serde_json::Map::new();
        ekm.insert(marketplace_name.to_string(), marketplace_entry);

        let mut settings = serde_json::Map::new();
        settings.insert("extraKnownMarketplaces".to_string(), serde_json::Value::Object(ekm));

        if !no_starter {
            let plugin_key = format!("starter-aipm-plugin@{marketplace_name}");
            let mut ep = serde_json::Map::new();
            ep.insert(plugin_key, serde_json::json!(true));
            settings.insert("enabledPlugins".to_string(), serde_json::Value::Object(ep));
        }

        let obj = serde_json::Value::Object(settings);
        let mut output = serde_json::to_string_pretty(&obj).unwrap_or_default();
        output.push('\n');

        crate::workspace_init::write_file(&settings_path, &output, fs)?;
        Ok(true)
    }
}

fn merge_claude_settings(
    settings_path: &Path,
    no_starter: bool,
    marketplace_name: &str,
    fs: &dyn Fs,
) -> Result<bool, Error> {
    let content = fs.read_to_string(settings_path)?;
    let mut json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|source| Error::JsonParse { path: settings_path.to_path_buf(), source })?;

    let obj = json.as_object_mut().ok_or_else(|| Error::JsonParse {
        path: settings_path.to_path_buf(),
        source: serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "expected JSON object",
        )),
    })?;

    // Check if already correctly configured
    let has_marketplace =
        obj.get("extraKnownMarketplaces").and_then(|ekm| ekm.get(marketplace_name)).is_some();

    let starter_key = format!("starter-aipm-plugin@{marketplace_name}");

    if no_starter {
        if has_marketplace {
            return Ok(false);
        }
    } else {
        let has_enabled = obj
            .get("enabledPlugins")
            .and_then(|ep| ep.as_object())
            .is_some_and(|ep| ep.contains_key(&starter_key));
        if has_marketplace && has_enabled {
            return Ok(false);
        }
    }

    // Ensure marketplace entry exists
    let marketplace_entry = serde_json::json!({
        "source": {
            "source": "directory",
            "path": "./.ai"
        }
    });

    if let Some(ekm) = obj.get_mut("extraKnownMarketplaces") {
        if let Some(ekm_obj) = ekm.as_object_mut() {
            ekm_obj.entry(marketplace_name).or_insert(marketplace_entry);
        }
    } else {
        let mut ekm = serde_json::Map::new();
        ekm.insert(marketplace_name.to_string(), marketplace_entry);
        obj.insert("extraKnownMarketplaces".to_string(), serde_json::Value::Object(ekm));
    }

    // Add enabledPlugins only when starter plugin is requested
    if !no_starter {
        let enabled = obj.entry("enabledPlugins").or_insert_with(|| serde_json::json!({}));
        if let Some(enabled_obj) = enabled.as_object_mut() {
            enabled_obj.entry(&starter_key).or_insert(serde_json::json!(true));
        }
    }

    let mut output = serde_json::to_string_pretty(&json)
        .map_err(|source| Error::JsonParse { path: settings_path.to_path_buf(), source })?;
    output.push('\n');
    fs.write_file(settings_path, output.as_bytes())?;

    Ok(true)
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
}
