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

    fn apply(&self, dir: &Path, fs: &dyn Fs) -> Result<bool, Error> {
        let settings_dir = dir.join(".claude");
        let settings_path = settings_dir.join("settings.json");

        if fs.exists(&settings_path) {
            return merge_claude_settings(&settings_path, fs);
        }

        fs.create_dir_all(&settings_dir)?;
        crate::workspace_init::write_file(
            &settings_path,
            "{\n\
             \x20 \"extraKnownMarketplaces\": {\n\
             \x20   \"local-repo-plugins\": {\n\
             \x20     \"source\": {\n\
             \x20       \"source\": \"directory\",\n\
             \x20       \"path\": \"./.ai\"\n\
             \x20     }\n\
             \x20   }\n\
             \x20 },\n\
             \x20 \"enabledPlugins\": {\n\
             \x20   \"starter-aipm-plugin@local-repo-plugins\": true\n\
             \x20 }\n\
             }\n",
            fs,
        )?;
        Ok(true)
    }
}

fn merge_claude_settings(settings_path: &Path, fs: &dyn Fs) -> Result<bool, Error> {
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

    // Check if both marketplace and enabledPlugins are already correctly configured
    let has_marketplace =
        obj.get("extraKnownMarketplaces").and_then(|ekm| ekm.get("local-repo-plugins")).is_some();
    let has_enabled = obj
        .get("enabledPlugins")
        .and_then(|ep| ep.as_object())
        .is_some_and(|ep| ep.contains_key("starter-aipm-plugin@local-repo-plugins"));
    if has_marketplace && has_enabled {
        return Ok(false);
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
            ekm_obj.entry("local-repo-plugins").or_insert(marketplace_entry);
        }
    } else {
        obj.insert(
            "extraKnownMarketplaces".to_string(),
            serde_json::json!({ "local-repo-plugins": marketplace_entry }),
        );
    }

    // Add enabledPlugins at the top level (sibling of extraKnownMarketplaces)
    let enabled = obj.entry("enabledPlugins").or_insert_with(|| serde_json::json!({}));
    if let Some(enabled_obj) = enabled.as_object_mut() {
        enabled_obj
            .entry("starter-aipm-plugin@local-repo-plugins")
            .or_insert(serde_json::json!(true));
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
        let result = adaptor.apply(&tmp, &Real);
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
    fn claude_settings_merge_existing() {
        let tmp = make_temp_dir("merge");
        std::fs::create_dir_all(tmp.join(".claude")).ok();
        std::fs::write(
            tmp.join(".claude/settings.json"),
            "{\"permissions\": {\"allow\": [\"Read\"]}}",
        )
        .ok();

        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp, &Real);
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
        let result = adaptor.apply(&tmp, &Real);
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
        let result = adaptor.apply(&tmp, &Real);
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
        let result = adaptor.apply(&tmp, &Real);
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
        let result = adaptor.apply(&tmp, &Real);
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
        let result = adaptor.apply(&tmp, &Real);
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
        let result = adaptor.apply(&tmp, &Real);
        // Should succeed — skips non-object enabledPlugins, still writes marketplace
        assert!(result.is_ok());

        cleanup(&tmp);
    }
}
