//! Claude Code tool adaptor for `aipm init`.
//!
//! Creates or merges `.claude/settings.json` with `extraKnownMarketplaces`
//! pointing to the `.ai/` local marketplace directory.

use std::io::Write;
use std::path::Path;

use crate::workspace_init::{Error, ToolAdaptor};

/// Configures Claude Code to discover the `.ai/` local marketplace.
pub struct Adaptor;

impl ToolAdaptor for Adaptor {
    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn apply(&self, dir: &Path) -> Result<bool, Error> {
        let settings_dir = dir.join(".claude");
        let settings_path = settings_dir.join("settings.json");

        if settings_path.exists() {
            return merge_claude_settings(&settings_path);
        }

        std::fs::create_dir_all(&settings_dir)?;
        crate::workspace_init::write_file(
            &settings_path,
            "{\n\
             \x20 \"extraKnownMarketplaces\": {\n\
             \x20   \"local\": {\n\
             \x20     \"source\": {\n\
             \x20       \"source\": \"directory\",\n\
             \x20       \"path\": \".ai\"\n\
             \x20     }\n\
             \x20   }\n\
             \x20 }\n\
             }\n",
        )?;
        Ok(true)
    }
}

fn merge_claude_settings(settings_path: &Path) -> Result<bool, Error> {
    let content = std::fs::read_to_string(settings_path)?;
    let mut json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|source| Error::JsonParse { path: settings_path.to_path_buf(), source })?;

    let obj = json.as_object_mut().ok_or_else(|| Error::JsonParse {
        path: settings_path.to_path_buf(),
        source: serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "expected JSON object",
        )),
    })?;

    if let Some(ekm) = obj.get("extraKnownMarketplaces") {
        if ekm.get("local").is_some() {
            return Ok(false);
        }
    }

    let marketplace_entry = serde_json::json!({
        "source": {
            "source": "directory",
            "path": ".ai"
        }
    });

    if let Some(ekm) = obj.get_mut("extraKnownMarketplaces") {
        if let Some(ekm_obj) = ekm.as_object_mut() {
            ekm_obj.insert("local".to_string(), marketplace_entry);
        }
    } else {
        obj.insert(
            "extraKnownMarketplaces".to_string(),
            serde_json::json!({ "local": marketplace_entry }),
        );
    }

    let output = serde_json::to_string_pretty(&json)
        .map_err(|source| Error::JsonParse { path: settings_path.to_path_buf(), source })?;
    let mut file = std::fs::File::create(settings_path)?;
    file.write_all(output.as_bytes())?;
    file.write_all(b"\n")?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let result = adaptor.apply(&tmp);
        assert!(result.is_ok_and(|v| v));
        assert!(tmp.join(".claude/settings.json").exists());

        let content = std::fs::read_to_string(tmp.join(".claude/settings.json"));
        assert!(content.is_ok_and(|c| c.contains("extraKnownMarketplaces")));

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
        let result = adaptor.apply(&tmp);
        assert!(result.is_ok_and(|v| v));

        let content = std::fs::read_to_string(tmp.join(".claude/settings.json"));
        assert!(content.as_ref().is_ok_and(|c| c.contains("extraKnownMarketplaces")));
        assert!(content.is_ok_and(|c| c.contains("allow")));

        cleanup(&tmp);
    }

    #[test]
    fn claude_settings_skip_if_present() {
        let tmp = make_temp_dir("skip");
        std::fs::create_dir_all(tmp.join(".claude")).ok();
        std::fs::write(
            tmp.join(".claude/settings.json"),
            "{\"extraKnownMarketplaces\": {\"local\": {\"source\": {\"source\": \"directory\", \"path\": \".ai\"}}}}",
        ).ok();

        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp);
        assert!(result.is_ok_and(|v| !v));

        cleanup(&tmp);
    }
}
