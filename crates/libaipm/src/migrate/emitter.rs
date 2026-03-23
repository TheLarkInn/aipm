//! Emitter: converts detected artifacts into plugin directories under `.ai/`.

use std::collections::HashSet;
use std::hash::BuildHasher;
use std::path::Path;

use crate::fs::Fs;
use crate::workspace_init::write_file;

use super::{Action, Artifact, ArtifactKind, ArtifactMetadata, Error};

/// Emit a single artifact as a plugin directory.
///
/// Returns the final plugin name (may differ from artifact name if renamed)
/// and the list of actions taken.
pub fn emit_plugin<S: BuildHasher>(
    artifact: &Artifact,
    ai_dir: &Path,
    existing_names: &HashSet<String, S>,
    rename_counter: &mut u32,
    fs: &dyn Fs,
) -> Result<(String, Vec<Action>), Error> {
    let mut actions = Vec::new();

    // 1. Resolve name (handle conflicts)
    let plugin_name =
        resolve_plugin_name(&artifact.name, existing_names, rename_counter, &mut actions);

    let plugin_dir = ai_dir.join(&plugin_name);

    // 2. Create directory structure
    fs.create_dir_all(&plugin_dir)?;
    fs.create_dir_all(&plugin_dir.join(".claude-plugin"))?;
    fs.create_dir_all(&plugin_dir.join("skills").join(&artifact.name))?;

    // 3. Handle skill vs command artifact types
    match artifact.kind {
        ArtifactKind::Skill => {
            emit_skill_files(artifact, &plugin_dir, fs)?;
        },
        ArtifactKind::Command => {
            emit_command_as_skill(artifact, &plugin_dir, fs)?;
        },
    }

    // 4. Copy referenced scripts
    if !artifact.referenced_scripts.is_empty() {
        let scripts_dir = plugin_dir.join("scripts");
        fs.create_dir_all(&scripts_dir)?;
        for script in &artifact.referenced_scripts {
            let source = artifact.source_path.join(script);
            if fs.exists(&source) {
                if let Some(file_name) = script.file_name() {
                    let dest = scripts_dir.join(file_name);
                    let content = fs.read_to_string(&source)?;
                    fs.write_file(&dest, content.as_bytes())?;
                }
            }
        }
    }

    // 5. Extract hooks (if any) into hooks/hooks.json
    if let Some(ref hooks_yaml) = artifact.metadata.hooks {
        let hooks_dir = plugin_dir.join("hooks");
        fs.create_dir_all(&hooks_dir)?;
        let hooks_json = convert_hooks_yaml_to_json(hooks_yaml);
        write_file(&hooks_dir.join("hooks.json"), &hooks_json, fs)?;
    }

    // 6. Generate aipm.toml
    let manifest = generate_plugin_manifest(artifact, &plugin_name);
    write_file(&plugin_dir.join("aipm.toml"), &manifest, fs)?;

    // 7. Generate .claude-plugin/plugin.json
    let plugin_json = generate_plugin_json(&plugin_name, &artifact.metadata);
    write_file(&plugin_dir.join(".claude-plugin").join("plugin.json"), &plugin_json, fs)?;

    actions.push(Action::PluginCreated {
        name: plugin_name.clone(),
        source: artifact.source_path.clone(),
        plugin_type: artifact.kind.to_type_string().to_string(),
    });

    Ok((plugin_name, actions))
}

/// Copy skill files from artifact source to plugin directory, rewriting paths.
fn emit_skill_files(artifact: &Artifact, plugin_dir: &Path, fs: &dyn Fs) -> Result<(), Error> {
    for file in &artifact.files {
        let source = artifact.source_path.join(file);
        let dest = plugin_dir.join("skills").join(&artifact.name).join(file);
        if let Some(parent) = dest.parent() {
            fs.create_dir_all(parent)?;
        }
        let content = fs.read_to_string(&source)?;

        let final_content =
            if file_is_skill_md(file) { rewrite_skill_dir_paths(&content) } else { content };

        fs.write_file(&dest, final_content.as_bytes())?;
    }
    Ok(())
}

/// Convert a command artifact into a skill within the plugin directory.
fn emit_command_as_skill(artifact: &Artifact, plugin_dir: &Path, fs: &dyn Fs) -> Result<(), Error> {
    let skill_md_path = plugin_dir.join("skills").join(&artifact.name).join("SKILL.md");

    // Read the original command content
    let content = fs.read_to_string(&artifact.source_path)?;

    // Wrap with frontmatter if not present, or add disable-model-invocation
    let skill_content = if content.trim_start().starts_with("---") {
        // Has frontmatter — inject disable-model-invocation
        inject_disable_model_invocation(&content)
    } else {
        // No frontmatter — wrap with new frontmatter
        format!("---\nname: {}\ndisable-model-invocation: true\n---\n{}", artifact.name, content)
    };

    fs.write_file(&skill_md_path, skill_content.as_bytes())?;
    Ok(())
}

/// Inject `disable-model-invocation: true` into existing frontmatter.
fn inject_disable_model_invocation(content: &str) -> String {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return content.to_string();
    }

    let after_first = &trimmed[3..];
    let rest = after_first.trim_start_matches(['\r', '\n']);
    rest.find("\n---").map_or_else(
        || content.to_string(),
        |pos| {
            let yaml_block = &rest[..pos];
            let after_closing = &rest[pos + 4..]; // skip \n---
            format!("---\n{yaml_block}\ndisable-model-invocation: true\n---{after_closing}")
        },
    )
}

/// Resolve plugin name, auto-renaming on conflict.
pub fn resolve_plugin_name<S: BuildHasher>(
    name: &str,
    existing: &HashSet<String, S>,
    counter: &mut u32,
    actions: &mut Vec<Action>,
) -> String {
    if !existing.contains(name) {
        return name.to_string();
    }

    *counter += 1;
    let new_name = format!("{name}-renamed-{counter}");
    actions.push(Action::Renamed {
        original_name: name.to_string(),
        new_name: new_name.clone(),
        reason: format!("plugin '{name}' already exists in .ai/"),
    });
    new_name
}

/// Check if a file path refers to a `SKILL.md` file.
fn file_is_skill_md(path: &Path) -> bool {
    path.file_name().and_then(|f| f.to_str()).is_some_and(|f| f == "SKILL.md")
}

/// Rewrite `${CLAUDE_SKILL_DIR}/scripts/` paths in SKILL.md content.
fn rewrite_skill_dir_paths(content: &str) -> String {
    content.replace("${CLAUDE_SKILL_DIR}/scripts/", "${CLAUDE_SKILL_DIR}/../../scripts/")
}

/// Convert hooks YAML block to JSON format.
fn convert_hooks_yaml_to_json(hooks_yaml: &str) -> String {
    // Simple conversion: parse key-value pairs from YAML-like format
    let mut json_parts = Vec::new();
    let mut current_key: Option<String> = None;
    let mut current_value: Option<String> = None;

    for line in hooks_yaml.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check if this is a top-level key (no leading whitespace in original, or key: value)
        if !line.starts_with(' ') && !line.starts_with('\t') {
            // Save previous key-value if any
            if let (Some(k), Some(v)) = (current_key.take(), current_value.take()) {
                json_parts.push(format!("  \"{k}\": \"{v}\""));
            }
            if let Some(pos) = trimmed.find(':') {
                let key = trimmed[..pos].trim();
                let val = trimmed[pos + 1..].trim();
                current_key = Some(key.to_string());
                if val.is_empty() {
                    current_value = None;
                } else {
                    current_value = Some(val.to_string());
                }
            }
        } else if let Some(ref _key) = current_key {
            // Indented continuation — treat as value
            if current_value.is_none() {
                current_value = Some(trimmed.to_string());
            } else if let Some(ref mut v) = current_value {
                v.push(' ');
                v.push_str(trimmed);
            }
        }
    }

    // Save last key-value
    if let (Some(k), Some(v)) = (current_key, current_value) {
        json_parts.push(format!("  \"{k}\": \"{v}\""));
    }

    if json_parts.is_empty() {
        "{}".to_string()
    } else {
        format!("{{\n{}\n}}", json_parts.join(",\n"))
    }
}

/// Generate `aipm.toml` for a migrated plugin.
fn generate_plugin_manifest(artifact: &Artifact, plugin_name: &str) -> String {
    let type_str = artifact.kind.to_type_string();

    let description =
        artifact.metadata.description.as_deref().unwrap_or("Migrated from .claude/ configuration");

    let mut components = Vec::new();

    // Skills component
    components.push(format!("skills = [\"skills/{}/SKILL.md\"]", artifact.name));

    // Scripts component (if any)
    if !artifact.referenced_scripts.is_empty() {
        let scripts: Vec<String> = artifact
            .referenced_scripts
            .iter()
            .filter_map(|p| p.file_name())
            .map(|f| format!("\"scripts/{}\"", f.to_string_lossy()))
            .collect();
        components.push(format!("scripts = [{}]", scripts.join(", ")));
    }

    // Hooks component (if extracted)
    if artifact.metadata.hooks.is_some() {
        components.push("hooks = [\"hooks/hooks.json\"]".to_string());
    }

    let components_section = components.join("\n");

    format!(
        "[package]\n\
         name = \"{plugin_name}\"\n\
         version = \"0.1.0\"\n\
         type = \"{type_str}\"\n\
         edition = \"2024\"\n\
         description = \"{description}\"\n\
         \n\
         [components]\n\
         {components_section}\n"
    )
}

/// Generate `.claude-plugin/plugin.json` for a migrated plugin.
fn generate_plugin_json(name: &str, metadata: &ArtifactMetadata) -> String {
    let description =
        metadata.description.as_deref().unwrap_or("Migrated from .claude/ configuration");

    format!(
        "{{\n  \"name\": \"{name}\",\n  \"version\": \"0.1.0\",\n  \
         \"description\": \"{description}\"\n}}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::PathBuf;

    struct MockFs {
        exists: HashSet<PathBuf>,
        dirs: HashMap<PathBuf, Vec<crate::fs::DirEntry>>,
        files: HashMap<PathBuf, String>,
        written: RefCell<HashMap<PathBuf, Vec<u8>>>,
    }

    impl MockFs {
        fn new() -> Self {
            Self {
                exists: HashSet::new(),
                dirs: HashMap::new(),
                files: HashMap::new(),
                written: RefCell::new(HashMap::new()),
            }
        }

        fn get_written(&self, path: &Path) -> Option<String> {
            self.written.borrow().get(path).and_then(|b| String::from_utf8(b.clone()).ok())
        }
    }

    impl crate::fs::Fs for MockFs {
        fn exists(&self, path: &Path) -> bool {
            self.exists.contains(path)
        }

        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            Ok(())
        }

        fn write_file(&self, path: &Path, content: &[u8]) -> std::io::Result<()> {
            self.written.borrow_mut().insert(path.to_path_buf(), content.to_vec());
            Ok(())
        }

        fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
            self.files.get(path).cloned().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("not found: {}", path.display()),
                )
            })
        }

        fn read_dir(&self, path: &Path) -> std::io::Result<Vec<crate::fs::DirEntry>> {
            self.dirs.get(path).cloned().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("dir not found: {}", path.display()),
                )
            })
        }
    }

    fn make_skill_artifact() -> Artifact {
        Artifact {
            kind: ArtifactKind::Skill,
            name: "deploy".to_string(),
            source_path: PathBuf::from("/src/skills/deploy"),
            files: vec![PathBuf::from("SKILL.md")],
            referenced_scripts: Vec::new(),
            metadata: ArtifactMetadata {
                name: Some("deploy".to_string()),
                description: Some("Deploy app".to_string()),
                hooks: None,
                model_invocation_disabled: false,
            },
        }
    }

    fn make_command_artifact() -> Artifact {
        Artifact {
            kind: ArtifactKind::Command,
            name: "review".to_string(),
            source_path: PathBuf::from("/src/commands/review.md"),
            files: vec![PathBuf::from("review.md")],
            referenced_scripts: Vec::new(),
            metadata: ArtifactMetadata {
                name: None,
                description: None,
                hooks: None,
                model_invocation_disabled: true,
            },
        }
    }

    #[test]
    fn emit_creates_plugin_directory_structure() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_skill_artifact();
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, &fs);
        assert!(result.is_ok());
        // Check that aipm.toml was written
        assert!(fs.get_written(Path::new("/ai/deploy/aipm.toml")).is_some());
        // Check that plugin.json was written
        assert!(fs.get_written(Path::new("/ai/deploy/.claude-plugin/plugin.json")).is_some());
    }

    #[test]
    fn emit_generates_valid_aipm_toml() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_skill_artifact();
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, &fs);

        let toml_content = fs.get_written(Path::new("/ai/deploy/aipm.toml"));
        assert!(toml_content.is_some());
        if let Some(content) = toml_content {
            assert!(content.contains("name = \"deploy\""));
            assert!(content.contains("type = \"skill\""));
            assert!(content.contains("version = \"0.1.0\""));
        }
    }

    #[test]
    fn emit_generates_plugin_json() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_skill_artifact();
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, &fs);

        let json_content = fs.get_written(Path::new("/ai/deploy/.claude-plugin/plugin.json"));
        assert!(json_content.is_some());
        if let Some(content) = json_content {
            assert!(content.contains("\"name\": \"deploy\""));
            assert!(content.contains("\"version\": \"0.1.0\""));
        }
    }

    #[test]
    fn emit_copies_skill_files() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_skill_artifact();
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, &fs);

        let skill_content = fs.get_written(Path::new("/ai/deploy/skills/deploy/SKILL.md"));
        assert!(skill_content.is_some_and(|c| c == "Deploy content"));
    }

    #[test]
    fn emit_copies_referenced_scripts() {
        let mut fs = MockFs::new();
        fs.files.insert(
            PathBuf::from("/src/skills/deploy/SKILL.md"),
            "Run ${CLAUDE_SKILL_DIR}/scripts/deploy.sh".to_string(),
        );
        fs.files.insert(
            PathBuf::from("/src/skills/deploy/scripts/deploy.sh"),
            "#!/bin/bash\necho deploy".to_string(),
        );
        fs.exists.insert(PathBuf::from("/src/skills/deploy/scripts/deploy.sh"));

        let existing = HashSet::new();
        let mut counter = 0;
        let mut artifact = make_skill_artifact();
        artifact.referenced_scripts = vec![PathBuf::from("scripts/deploy.sh")];
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, &fs);

        let script_content = fs.get_written(Path::new("/ai/deploy/scripts/deploy.sh"));
        assert!(script_content.is_some());
    }

    #[test]
    fn emit_rewrites_claude_skill_dir() {
        let mut fs = MockFs::new();
        fs.files.insert(
            PathBuf::from("/src/skills/deploy/SKILL.md"),
            "Run ${CLAUDE_SKILL_DIR}/scripts/deploy.sh here".to_string(),
        );

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_skill_artifact();
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, &fs);

        let content = fs.get_written(Path::new("/ai/deploy/skills/deploy/SKILL.md"));
        assert!(content.as_ref().is_some_and(|c| c.contains("${CLAUDE_SKILL_DIR}/../../scripts/")));
        assert!(content.as_ref().is_some_and(|c| !c.contains("${CLAUDE_SKILL_DIR}/scripts/")));
    }

    #[test]
    fn emit_extracts_hooks_to_json() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());

        let existing = HashSet::new();
        let mut counter = 0;
        let mut artifact = make_skill_artifact();
        artifact.metadata.hooks = Some("PreToolUse: check_deploy".to_string());
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, &fs);

        let hooks_content = fs.get_written(Path::new("/ai/deploy/hooks/hooks.json"));
        assert!(hooks_content.is_some());
        if let Some(content) = hooks_content {
            assert!(content.contains("PreToolUse"));
        }
    }

    #[test]
    fn emit_command_as_skill() {
        let mut fs = MockFs::new();
        fs.files.insert(
            PathBuf::from("/src/commands/review.md"),
            "Review the code carefully".to_string(),
        );

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_command_artifact();
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, &fs);

        let skill_content = fs.get_written(Path::new("/ai/review/skills/review/SKILL.md"));
        assert!(skill_content
            .as_ref()
            .is_some_and(|c| c.contains("disable-model-invocation: true")));
    }

    #[test]
    fn emit_command_wraps_with_frontmatter() {
        let mut fs = MockFs::new();
        fs.files
            .insert(PathBuf::from("/src/commands/review.md"), "Plain markdown content".to_string());

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_command_artifact();
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, &fs);

        let skill_content = fs.get_written(Path::new("/ai/review/skills/review/SKILL.md"));
        assert!(skill_content.as_ref().is_some_and(|c| c.starts_with("---\n")));
        assert!(skill_content.as_ref().is_some_and(|c| c.contains("name: review")));
    }

    #[test]
    fn resolve_name_no_conflict() {
        let existing = HashSet::new();
        let mut counter = 0;
        let mut actions = Vec::new();
        let name = resolve_plugin_name("deploy", &existing, &mut counter, &mut actions);
        assert_eq!(name, "deploy");
        assert!(actions.is_empty());
    }

    #[test]
    fn resolve_name_conflict_renames() {
        let mut existing = HashSet::new();
        existing.insert("deploy".to_string());
        let mut counter = 0;
        let mut actions = Vec::new();
        let name = resolve_plugin_name("deploy", &existing, &mut counter, &mut actions);
        assert_eq!(name, "deploy-renamed-1");
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Renamed { .. }));
    }

    #[test]
    fn resolve_name_multiple_conflicts() {
        let mut existing = HashSet::new();
        existing.insert("deploy".to_string());
        existing.insert("lint".to_string());
        let mut counter = 0;
        let mut actions = Vec::new();

        let name1 = resolve_plugin_name("deploy", &existing, &mut counter, &mut actions);
        let name2 = resolve_plugin_name("lint", &existing, &mut counter, &mut actions);

        assert_eq!(name1, "deploy-renamed-1");
        assert_eq!(name2, "lint-renamed-2");
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn convert_hooks_yaml_basic() {
        let result = convert_hooks_yaml_to_json("PreToolUse: check_deploy");
        assert!(result.contains("PreToolUse"));
        assert!(result.contains("check_deploy"));
    }

    #[test]
    fn convert_hooks_yaml_empty() {
        let result = convert_hooks_yaml_to_json("");
        assert_eq!(result, "{}");
    }

    #[test]
    fn convert_hooks_yaml_multiline() {
        let result =
            convert_hooks_yaml_to_json("PreToolUse:\n  check_deploy\nPostToolUse:\n  log_result");
        assert!(result.contains("PreToolUse"));
        assert!(result.contains("PostToolUse"));
    }

    #[test]
    fn convert_hooks_yaml_with_blank_lines() {
        let result = convert_hooks_yaml_to_json("PreToolUse: check\n\nPostToolUse: log");
        assert!(result.contains("PreToolUse"));
        assert!(result.contains("PostToolUse"));
    }

    #[test]
    fn inject_disable_no_frontmatter() {
        let result = inject_disable_model_invocation("just plain text");
        assert_eq!(result, "just plain text");
    }

    #[test]
    fn inject_disable_with_frontmatter() {
        let result = inject_disable_model_invocation("---\nname: test\n---\nbody");
        assert!(result.contains("disable-model-invocation: true"));
        assert!(result.contains("name: test"));
    }

    #[test]
    fn inject_disable_no_closing_delimiter() {
        let result = inject_disable_model_invocation("---\nname: test\nno closing");
        assert_eq!(result, "---\nname: test\nno closing");
    }

    #[test]
    fn rewrite_paths_no_scripts() {
        let result = rewrite_skill_dir_paths("no script references here");
        assert_eq!(result, "no script references here");
    }

    #[test]
    fn rewrite_paths_with_scripts() {
        let result = rewrite_skill_dir_paths("run ${CLAUDE_SKILL_DIR}/scripts/deploy.sh");
        assert!(result.contains("${CLAUDE_SKILL_DIR}/../../scripts/deploy.sh"));
    }

    #[test]
    fn file_is_skill_md_true() {
        assert!(file_is_skill_md(Path::new("SKILL.md")));
        assert!(file_is_skill_md(Path::new("dir/SKILL.md")));
    }

    #[test]
    fn file_is_skill_md_false() {
        assert!(!file_is_skill_md(Path::new("readme.md")));
        assert!(!file_is_skill_md(Path::new("skill.md")));
    }

    #[test]
    fn generate_manifest_with_scripts_and_hooks() {
        let artifact = Artifact {
            kind: ArtifactKind::Skill,
            name: "deploy".to_string(),
            source_path: PathBuf::from("/src"),
            files: vec![PathBuf::from("SKILL.md")],
            referenced_scripts: vec![PathBuf::from("scripts/deploy.sh")],
            metadata: ArtifactMetadata {
                name: Some("deploy".to_string()),
                description: Some("Deploy app".to_string()),
                hooks: Some("PreToolUse: check".to_string()),
                model_invocation_disabled: false,
            },
        };
        let manifest = generate_plugin_manifest(&artifact, "deploy");
        assert!(manifest.contains("scripts = [\"scripts/deploy.sh\"]"));
        assert!(manifest.contains("hooks = [\"hooks/hooks.json\"]"));
    }

    #[test]
    fn generate_manifest_no_description() {
        let artifact = Artifact {
            kind: ArtifactKind::Command,
            name: "review".to_string(),
            source_path: PathBuf::from("/src"),
            files: vec![],
            referenced_scripts: Vec::new(),
            metadata: ArtifactMetadata::default(),
        };
        let manifest = generate_plugin_manifest(&artifact, "review");
        assert!(manifest.contains("Migrated from .claude/ configuration"));
    }

    #[test]
    fn generate_plugin_json_with_description() {
        let metadata = ArtifactMetadata {
            description: Some("Test desc".to_string()),
            ..ArtifactMetadata::default()
        };
        let json = generate_plugin_json("test", &metadata);
        assert!(json.contains("Test desc"));
    }

    #[test]
    fn generate_plugin_json_no_description() {
        let json = generate_plugin_json("test", &ArtifactMetadata::default());
        assert!(json.contains("Migrated from .claude/ configuration"));
    }

    #[test]
    fn emit_command_with_existing_frontmatter() {
        let mut fs = MockFs::new();
        fs.files.insert(
            PathBuf::from("/src/commands/review.md"),
            "---\nname: review\ndescription: Code review\n---\nReview body".to_string(),
        );

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_command_artifact();
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, &fs);

        let skill_content = fs.get_written(Path::new("/ai/review/skills/review/SKILL.md"));
        assert!(skill_content
            .as_ref()
            .is_some_and(|c| c.contains("disable-model-invocation: true")));
        assert!(skill_content.as_ref().is_some_and(|c| c.contains("name: review")));
    }

    #[test]
    fn convert_hooks_yaml_indented_continuation() {
        let result = convert_hooks_yaml_to_json("PreToolUse:\n  first_value\n  second_value");
        assert!(result.contains("PreToolUse"));
        assert!(result.contains("first_value second_value"));
    }

    #[test]
    fn convert_hooks_yaml_no_colon() {
        let result = convert_hooks_yaml_to_json("no-colon-here");
        assert_eq!(result, "{}");
    }

    #[test]
    fn convert_hooks_yaml_key_with_empty_value_then_non_indented() {
        let result = convert_hooks_yaml_to_json("Key1:\nKey2: value2");
        assert!(result.contains("Key2"));
    }

    #[test]
    fn convert_hooks_yaml_tab_indented() {
        let result = convert_hooks_yaml_to_json("PreToolUse:\n\ttab_value");
        assert!(result.contains("PreToolUse"));
    }

    #[test]
    fn emit_skill_with_missing_script() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());
        // Script referenced but does NOT exist on disk
        let existing = HashSet::new();
        let mut counter = 0;
        let mut artifact = make_skill_artifact();
        artifact.referenced_scripts = vec![PathBuf::from("scripts/missing.sh")];
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, &fs);
        assert!(result.is_ok());
        // Script should not be written
        assert!(fs.get_written(Path::new("/ai/deploy/scripts/missing.sh")).is_none());
    }

    #[test]
    fn convert_hooks_indented_line_without_key() {
        // Indented line before any key is set — should be ignored
        let result = convert_hooks_yaml_to_json("  indented_no_key\nKey: value");
        assert!(result.contains("Key"));
    }

    #[test]
    fn emit_skill_with_non_skill_md_files() {
        let mut fs = MockFs::new();
        fs.files.insert(
            PathBuf::from("/src/skills/deploy/SKILL.md"),
            "Deploy content with ${CLAUDE_SKILL_DIR}/scripts/run.sh".to_string(),
        );
        fs.files.insert(PathBuf::from("/src/skills/deploy/README.md"), "readme".to_string());

        let existing = HashSet::new();
        let mut counter = 0;
        let mut artifact = make_skill_artifact();
        artifact.files = vec![PathBuf::from("SKILL.md"), PathBuf::from("README.md")];
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, &fs);

        // SKILL.md should have rewritten paths
        let skill = fs.get_written(Path::new("/ai/deploy/skills/deploy/SKILL.md"));
        assert!(skill.as_ref().is_some_and(|c| c.contains("${CLAUDE_SKILL_DIR}/../../scripts/")));
        // README.md should be copied as-is
        let readme = fs.get_written(Path::new("/ai/deploy/skills/deploy/README.md"));
        assert!(readme.is_some_and(|c| c == "readme"));
    }
}
