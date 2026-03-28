//! Emitter: converts detected artifacts into plugin directories under `.ai/`.

use std::collections::HashSet;
use std::hash::BuildHasher;
use std::path::Path;

use crate::fs::Fs;
use crate::workspace_init::write_file;

use serde::Serialize;

use super::{Action, Artifact, ArtifactKind, ArtifactMetadata, Error};

use std::path::PathBuf;

/// Validate that a name is a safe single path segment (no traversal, no separators).
fn is_safe_path_segment(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !Path::new(name).is_absolute()
}

/// Emit a single artifact as a plugin directory.
///
/// Returns the final plugin name (may differ from artifact name if renamed)
/// and the list of actions taken.
pub fn emit_plugin<S: BuildHasher>(
    artifact: &Artifact,
    ai_dir: &Path,
    existing_names: &HashSet<String, S>,
    rename_counter: &mut u32,
    manifest: bool,
    fs: &dyn Fs,
) -> Result<(String, Vec<Action>), Error> {
    let mut actions = Vec::new();

    // Validate artifact name to prevent path traversal
    if !is_safe_path_segment(&artifact.name) {
        actions.push(Action::Skipped {
            name: artifact.name.clone(),
            reason: format!(
                "unsafe artifact name '{}': must be a single path segment without separators or '..'",
                artifact.name
            ),
        });
        return Ok((artifact.name.clone(), actions));
    }

    // 1. Resolve name (handle conflicts)
    let plugin_name =
        resolve_plugin_name(&artifact.name, existing_names, rename_counter, &mut actions);

    let plugin_dir = ai_dir.join(&plugin_name);

    // 2. Create directory structure
    fs.create_dir_all(&plugin_dir)?;
    fs.create_dir_all(&plugin_dir.join(".claude-plugin"))?;

    // 3. Handle artifact types
    match artifact.kind {
        ArtifactKind::Skill => {
            fs.create_dir_all(&plugin_dir.join("skills").join(&artifact.name))?;
            emit_skill_files(artifact, &plugin_dir, fs)?;
        },
        ArtifactKind::Command => {
            fs.create_dir_all(&plugin_dir.join("skills").join(&artifact.name))?;
            emit_command_as_skill(artifact, &plugin_dir, fs)?;
        },
        ArtifactKind::Agent => {
            emit_agent_files(artifact, &plugin_dir, fs)?;
        },
        ArtifactKind::McpServer => {
            emit_mcp_config(artifact, &plugin_dir, fs)?;
        },
        ArtifactKind::Hook => {
            emit_hooks_config(artifact, &plugin_dir, fs)?;
        },
        ArtifactKind::OutputStyle => {
            emit_output_style(artifact, &plugin_dir, fs)?;
        },
    }

    // 4. Copy referenced scripts, preserving relative path structure
    if !artifact.referenced_scripts.is_empty() {
        copy_referenced_scripts(artifact, &plugin_dir, fs)?;
    }

    // 5. Extract hooks (if any) into hooks/hooks.json (for skill/command artifacts with hooks)
    if let Some(ref hooks_yaml) = artifact.metadata.hooks {
        if artifact.kind != ArtifactKind::Hook {
            let hooks_dir = plugin_dir.join("hooks");
            fs.create_dir_all(&hooks_dir)?;
            let hooks_json = convert_hooks_yaml_to_json(hooks_yaml);
            write_file(&hooks_dir.join("hooks.json"), &hooks_json, fs)?;
        }
    }

    // 6. Generate aipm.toml (only when --manifest is requested)
    if manifest {
        let manifest_toml = generate_plugin_manifest(artifact, &plugin_name);
        write_file(&plugin_dir.join("aipm.toml"), &manifest_toml, fs)?;
    }

    // 7. Generate .claude-plugin/plugin.json
    let plugin_json = generate_plugin_json(&plugin_name, &artifact.metadata, &artifact.kind);
    write_file(&plugin_dir.join(".claude-plugin").join("plugin.json"), &plugin_json, fs)?;

    actions.push(Action::PluginCreated {
        name: plugin_name.clone(),
        source: artifact.source_path.clone(),
        plugin_type: artifact.kind.to_type_string().to_string(),
    });

    Ok((plugin_name, actions))
}

/// Copy skill files from artifact source to plugin directory, rewriting paths.
/// Excludes files under `scripts/` that are also in `referenced_scripts` to avoid
/// duplicates (those are copied to the plugin root `scripts/` directory separately).
fn emit_skill_files(artifact: &Artifact, plugin_dir: &Path, fs: &dyn Fs) -> Result<(), Error> {
    let scripts_prefix = Path::new("scripts");
    let referenced: HashSet<&Path> =
        artifact.referenced_scripts.iter().map(PathBuf::as_path).collect();

    for file in &artifact.files {
        // Skip files that are referenced scripts — they're copied to the root scripts/ dir
        if file.starts_with(scripts_prefix) && referenced.contains(file.as_path()) {
            continue;
        }

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
/// If the key already exists, rewrites its value to `true` instead of duplicating.
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

            // Check if key already exists and rewrite it; otherwise append
            let mut found_key = false;
            let mut new_yaml = String::new();
            for line in yaml_block.lines() {
                let trimmed_line = line.trim_start();
                if trimmed_line.starts_with("disable-model-invocation:") {
                    let indent_len = line.len() - trimmed_line.len();
                    let indent = &line[..indent_len];
                    new_yaml.push_str(indent);
                    new_yaml.push_str("disable-model-invocation: true\n");
                    found_key = true;
                } else {
                    new_yaml.push_str(line);
                    new_yaml.push('\n');
                }
            }
            if !found_key {
                new_yaml.push_str("disable-model-invocation: true\n");
            }

            format!("---\n{new_yaml}---{after_closing}")
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

/// Emit a single artifact as a plugin with a pre-resolved name.
///
/// Similar to `emit_plugin` but the name is already resolved (no rename logic).
/// Used by the recursive migration path where names are pre-resolved sequentially.
pub fn emit_plugin_with_name(
    artifact: &Artifact,
    plugin_name: &str,
    ai_dir: &Path,
    manifest: bool,
    fs: &dyn Fs,
) -> Result<Vec<Action>, Error> {
    let mut actions = Vec::new();

    if !is_safe_path_segment(plugin_name) {
        actions.push(Action::Skipped {
            name: plugin_name.to_string(),
            reason: format!(
                "unsafe plugin name '{plugin_name}': must be a single path segment without separators or '..'"
            ),
        });
        return Ok(actions);
    }
    if !is_safe_path_segment(&artifact.name) {
        actions.push(Action::Skipped {
            name: artifact.name.clone(),
            reason: format!(
                "unsafe artifact name '{}': must be a single path segment without separators or '..'",
                artifact.name
            ),
        });
        return Ok(actions);
    }

    let plugin_dir = ai_dir.join(plugin_name);

    fs.create_dir_all(&plugin_dir)?;
    fs.create_dir_all(&plugin_dir.join(".claude-plugin"))?;

    match artifact.kind {
        ArtifactKind::Skill => {
            fs.create_dir_all(&plugin_dir.join("skills").join(&artifact.name))?;
            emit_skill_files(artifact, &plugin_dir, fs)?;
        },
        ArtifactKind::Command => {
            fs.create_dir_all(&plugin_dir.join("skills").join(&artifact.name))?;
            emit_command_as_skill(artifact, &plugin_dir, fs)?;
        },
        ArtifactKind::Agent => {
            emit_agent_files(artifact, &plugin_dir, fs)?;
        },
        ArtifactKind::McpServer => {
            emit_mcp_config(artifact, &plugin_dir, fs)?;
        },
        ArtifactKind::Hook => {
            emit_hooks_config(artifact, &plugin_dir, fs)?;
        },
        ArtifactKind::OutputStyle => {
            emit_output_style(artifact, &plugin_dir, fs)?;
        },
    }

    if !artifact.referenced_scripts.is_empty() {
        copy_referenced_scripts(artifact, &plugin_dir, fs)?;
    }

    if let Some(ref hooks_yaml) = artifact.metadata.hooks {
        if artifact.kind != ArtifactKind::Hook {
            let hooks_dir = plugin_dir.join("hooks");
            fs.create_dir_all(&hooks_dir)?;
            let hooks_json = convert_hooks_yaml_to_json(hooks_yaml);
            write_file(&hooks_dir.join("hooks.json"), &hooks_json, fs)?;
        }
    }

    if manifest {
        let manifest_toml = generate_plugin_manifest(artifact, plugin_name);
        write_file(&plugin_dir.join("aipm.toml"), &manifest_toml, fs)?;
    }

    let plugin_json = generate_plugin_json(plugin_name, &artifact.metadata, &artifact.kind);
    write_file(&plugin_dir.join(".claude-plugin").join("plugin.json"), &plugin_json, fs)?;

    actions.push(Action::PluginCreated {
        name: plugin_name.to_string(),
        source: artifact.source_path.clone(),
        plugin_type: artifact.kind.to_type_string().to_string(),
    });

    Ok(actions)
}

/// Emit a package-scoped plugin containing multiple artifacts.
///
/// All artifacts are placed under a single plugin directory named `plugin_name`.
/// Skills retain their original names as subdirectories under `skills/`.
/// Commands are converted to skills.
pub fn emit_package_plugin(
    plugin_name: &str,
    artifacts: &[Artifact],
    ai_dir: &Path,
    manifest: bool,
    fs: &dyn Fs,
) -> Result<Vec<Action>, Error> {
    let mut actions = Vec::new();

    if !is_safe_path_segment(plugin_name) {
        actions.push(Action::Skipped {
            name: plugin_name.to_string(),
            reason: format!(
                "unsafe plugin name '{plugin_name}': must be a single path segment without separators or '..'"
            ),
        });
        return Ok(actions);
    }

    // Validate all artifact names for path safety
    for artifact in artifacts {
        if !is_safe_path_segment(&artifact.name) {
            actions.push(Action::Skipped {
                name: artifact.name.clone(),
                reason: format!(
                    "unsafe artifact name '{}': must be a single path segment without separators or '..'",
                    artifact.name
                ),
            });
            return Ok(actions);
        }
    }

    let plugin_dir = ai_dir.join(plugin_name);

    fs.create_dir_all(&plugin_dir)?;
    fs.create_dir_all(&plugin_dir.join(".claude-plugin"))?;

    let emit_result = emit_package_artifacts(artifacts, &plugin_dir, fs)?;

    let has_multiple_types = emit_result.distinct_kind_count > 1;

    // Merge hooks from skill/command artifacts into one hooks.json
    if !emit_result.hooks_yaml_parts.is_empty() {
        let hooks_dir = plugin_dir.join("hooks");
        fs.create_dir_all(&hooks_dir)?;
        let merged_hooks = emit_result.hooks_yaml_parts.join("\n");
        let hooks_json = convert_hooks_yaml_to_json(&merged_hooks);
        write_file(&hooks_dir.join("hooks.json"), &hooks_json, fs)?;
    }

    if manifest {
        let manifest_toml = generate_package_manifest(
            plugin_name,
            artifacts,
            &emit_result.component_paths,
            has_multiple_types,
            !emit_result.hooks_yaml_parts.is_empty(),
        );
        write_file(&plugin_dir.join("aipm.toml"), &manifest_toml, fs)?;
    }

    let first_metadata =
        artifacts.first().map_or_else(ArtifactMetadata::default, |a| a.metadata.clone());
    let all_kinds: Vec<ArtifactKind> = artifacts.iter().map(|a| a.kind.clone()).collect();
    let plugin_json = generate_plugin_json_multi(plugin_name, &first_metadata, &all_kinds);
    write_file(&plugin_dir.join(".claude-plugin").join("plugin.json"), &plugin_json, fs)?;

    let plugin_type = if has_multiple_types {
        "composite"
    } else {
        artifacts.first().map_or("composite", |a| a.kind.to_type_string())
    };
    if let Some(first) = artifacts.first() {
        actions.push(Action::PluginCreated {
            name: plugin_name.to_string(),
            source: first.source_path.clone(),
            plugin_type: plugin_type.to_string(),
        });
    }

    Ok(actions)
}

/// Result of emitting package artifacts: component paths, hooks YAML parts, distinct kind count.
struct PackageEmitResult {
    component_paths: Vec<String>,
    hooks_yaml_parts: Vec<String>,
    distinct_kind_count: usize,
}

/// Emit individual artifacts within a package plugin.
fn emit_package_artifacts(
    artifacts: &[Artifact],
    plugin_dir: &Path,
    fs: &dyn Fs,
) -> Result<PackageEmitResult, Error> {
    let mut all_component_paths = Vec::new();
    let mut merged_hooks_parts = Vec::new();
    let mut distinct_kinds: HashSet<&ArtifactKind> = HashSet::new();

    for artifact in artifacts {
        distinct_kinds.insert(&artifact.kind);

        match artifact.kind {
            ArtifactKind::Skill => {
                fs.create_dir_all(&plugin_dir.join("skills").join(&artifact.name))?;
                emit_skill_files(artifact, plugin_dir, fs)?;
                all_component_paths.push(format!("skills/{}/SKILL.md", artifact.name));
            },
            ArtifactKind::Command => {
                fs.create_dir_all(&plugin_dir.join("skills").join(&artifact.name))?;
                emit_command_as_skill(artifact, plugin_dir, fs)?;
                all_component_paths.push(format!("skills/{}/SKILL.md", artifact.name));
            },
            ArtifactKind::Agent => {
                emit_agent_files(artifact, plugin_dir, fs)?;
                all_component_paths.push(format!("agents/{}.md", artifact.name));
            },
            ArtifactKind::McpServer => {
                emit_mcp_config(artifact, plugin_dir, fs)?;
                all_component_paths.push(".mcp.json".to_string());
            },
            ArtifactKind::Hook => {
                emit_hooks_config(artifact, plugin_dir, fs)?;
                all_component_paths.push("hooks/hooks.json".to_string());
            },
            ArtifactKind::OutputStyle => {
                emit_output_style(artifact, plugin_dir, fs)?;
                all_component_paths.push(format!("{}.md", artifact.name));
            },
        }

        if !artifact.referenced_scripts.is_empty() {
            copy_referenced_scripts(artifact, plugin_dir, fs)?;
        }

        if artifact.kind != ArtifactKind::Hook {
            if let Some(ref hooks_yaml) = artifact.metadata.hooks {
                merged_hooks_parts.push(hooks_yaml.clone());
            }
        }
    }

    Ok(PackageEmitResult {
        component_paths: all_component_paths,
        hooks_yaml_parts: merged_hooks_parts,
        distinct_kind_count: distinct_kinds.len(),
    })
}

/// Copy agent `.md` file to `agents/<artifact.name>.md` inside the plugin directory.
///
/// The destination filename is always derived from `artifact.name` (not the original
/// filename) so that manifest component paths (`agents/<name>.md`) are consistent.
fn emit_agent_files(artifact: &Artifact, plugin_dir: &Path, fs: &dyn Fs) -> Result<(), Error> {
    let agents_dir = plugin_dir.join("agents");
    fs.create_dir_all(&agents_dir)?;

    // Read from source_path (full path to the .md file)
    let content = fs.read_to_string(&artifact.source_path)?;
    // Always emit as <artifact.name>.md to match manifest/component paths
    let dest = agents_dir.join(format!("{}.md", artifact.name));
    fs.write_file(&dest, content.as_bytes())?;

    Ok(())
}

/// Write MCP config to `.mcp.json` at the plugin root.
///
/// Uses `raw_content` if available, otherwise falls back to reading from `source_path`.
fn emit_mcp_config(artifact: &Artifact, plugin_dir: &Path, fs: &dyn Fs) -> Result<(), Error> {
    let content = match artifact.metadata.raw_content {
        Some(ref c) => c.clone(),
        None => fs.read_to_string(&artifact.source_path)?,
    };
    let dest = plugin_dir.join(".mcp.json");
    fs.write_file(&dest, content.as_bytes())?;
    Ok(())
}

/// Write hooks config to `hooks/hooks.json` inside the plugin directory.
///
/// Uses `raw_content` if available, otherwise falls back to reading from `source_path`.
/// Rewrites relative `command` paths in hook handlers to absolute paths.
fn emit_hooks_config(artifact: &Artifact, plugin_dir: &Path, fs: &dyn Fs) -> Result<(), Error> {
    let content = match artifact.metadata.raw_content {
        Some(ref c) => c.clone(),
        None => fs.read_to_string(&artifact.source_path)?,
    };

    // Rewrite relative command paths to absolute paths (Fix #6)
    let rewritten = rewrite_hook_command_paths(&content, &artifact.source_path);

    let hooks_dir = plugin_dir.join("hooks");
    fs.create_dir_all(&hooks_dir)?;
    write_file(&hooks_dir.join("hooks.json"), &rewritten, fs)?;
    Ok(())
}

/// Copy output style `.md` file to the plugin root as `<artifact.name>.md`.
fn emit_output_style(artifact: &Artifact, plugin_dir: &Path, fs: &dyn Fs) -> Result<(), Error> {
    let content = fs.read_to_string(&artifact.source_path)?;
    let dest = plugin_dir.join(format!("{}.md", artifact.name));
    fs.write_file(&dest, content.as_bytes())?;
    Ok(())
}

/// Copy referenced scripts to the plugin's `scripts/` directory.
fn copy_referenced_scripts(
    artifact: &Artifact,
    plugin_dir: &Path,
    fs: &dyn Fs,
) -> Result<(), Error> {
    let scripts_dir = plugin_dir.join("scripts");
    fs.create_dir_all(&scripts_dir)?;
    let scripts_root = Path::new("scripts");
    for script in &artifact.referenced_scripts {
        // Normalize the script path: strip leading "./" prefix
        let normalized = script
            .to_string_lossy()
            .strip_prefix("./")
            .map_or_else(|| script.clone(), PathBuf::from);

        // For hook artifacts, resolve against project root (grandparent of settings.json)
        // source_path is .claude/settings.json → parent is .claude/ → parent is project root
        let source = if artifact.kind == ArtifactKind::Hook {
            let project_root = artifact
                .source_path
                .parent() // .claude/
                .and_then(Path::parent); // project root
            project_root.map_or_else(
                || artifact.source_path.join(&normalized),
                |root| root.join(&normalized),
            )
        } else {
            artifact.source_path.join(&normalized)
        };

        if fs.exists(&source) {
            // Strip "scripts/" prefix for destination to avoid scripts/scripts/
            let relative = normalized.strip_prefix(scripts_root).unwrap_or(&normalized);
            let dest = scripts_dir.join(relative);
            if let Some(parent) = dest.parent() {
                fs.create_dir_all(parent)?;
            }
            let content = fs.read_to_string(&source)?;
            fs.write_file(&dest, content.as_bytes())?;
        }
    }
    Ok(())
}

/// Generate `aipm.toml` for a package-scoped plugin with multiple artifacts.
fn generate_package_manifest(
    plugin_name: &str,
    artifacts: &[Artifact],
    component_paths: &[String],
    has_multiple_types: bool,
    has_hooks_yaml: bool,
) -> String {
    let type_str = if has_multiple_types {
        "composite"
    } else {
        artifacts.first().map_or("composite", |a| a.kind.to_type_string())
    };
    let description = artifacts
        .first()
        .and_then(|a| a.metadata.description.as_deref())
        .unwrap_or("Migrated from .claude/ configuration");

    let mut components = PluginComponents::default();

    // Group component paths by type
    let skill_paths: Vec<String> =
        component_paths.iter().filter(|p| p.starts_with("skills/")).cloned().collect();
    let agent_paths: Vec<String> =
        component_paths.iter().filter(|p| p.starts_with("agents/")).cloned().collect();
    let mcp_paths: Vec<String> =
        component_paths.iter().filter(|p| *p == ".mcp.json").cloned().collect();
    let hook_paths: Vec<String> =
        component_paths.iter().filter(|p| p.starts_with("hooks/")).cloned().collect();
    let style_paths: Vec<String> = component_paths
        .iter()
        .filter(|p| {
            !p.starts_with("skills/")
                && !p.starts_with("agents/")
                && *p != ".mcp.json"
                && !p.starts_with("hooks/")
        })
        .cloned()
        .collect();

    if !skill_paths.is_empty() {
        components.skills = Some(skill_paths);
    }
    if !agent_paths.is_empty() {
        components.agents = Some(agent_paths);
    }
    if !mcp_paths.is_empty() {
        components.mcp_servers = Some(mcp_paths);
    }
    if !hook_paths.is_empty() {
        components.hooks = Some(hook_paths.clone());
    }
    if !style_paths.is_empty() {
        components.output_styles = Some(style_paths);
    }

    let all_scripts: Vec<String> = artifacts
        .iter()
        .flat_map(|a| {
            let scripts_root = Path::new("scripts");
            a.referenced_scripts.iter().map(move |p| {
                let relative = p.strip_prefix(scripts_root).unwrap_or(p);
                format!("scripts/{}", relative.to_string_lossy())
            })
        })
        .collect();
    if !all_scripts.is_empty() {
        components.scripts = Some(all_scripts);
    }
    // Only append hooks from skill/command frontmatter when no Hook artifact already emitted hooks
    if has_hooks_yaml && hook_paths.is_empty() {
        components.hooks = Some(vec!["hooks/hooks.json".to_string()]);
    }

    let manifest = PluginToml {
        package: PluginPackage {
            name: plugin_name.to_string(),
            version: "0.1.0".to_string(),
            kind: type_str.to_string(),
            description: description.to_string(),
        },
        components,
    };

    toml::to_string_pretty(&manifest).unwrap_or_default()
}

/// Check if a file path refers to a `SKILL.md` file.
fn file_is_skill_md(path: &Path) -> bool {
    path.file_name().and_then(|f| f.to_str()).is_some_and(|f| f == "SKILL.md")
}

/// Rewrite `${CLAUDE_SKILL_DIR}/scripts/` paths in SKILL.md content.
fn rewrite_skill_dir_paths(content: &str) -> String {
    content.replace("${CLAUDE_SKILL_DIR}/scripts/", "${CLAUDE_SKILL_DIR}/../../scripts/")
}

/// Rewrite relative `command` paths in hook handlers to absolute paths.
///
/// Parses the hooks JSON, finds `"type": "command"` handlers, and resolves
/// relative `command` paths against the project root (derived from `source_path`).
fn rewrite_hook_command_paths(content: &str, source_path: &Path) -> String {
    let mut json: serde_json::Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(_) => return content.to_string(),
    };

    // source_path is .claude/settings.json → project root is grandparent
    let project_root = source_path
        .parent() // .claude/
        .and_then(Path::parent); // project root

    if let Some(root) = project_root {
        rewrite_commands_recursive(&mut json, root);
    }

    serde_json::to_string_pretty(&json).unwrap_or_else(|_| content.to_string())
}

/// Recursively walk JSON and rewrite relative command paths to absolute.
fn rewrite_commands_recursive(value: &mut serde_json::Value, project_root: &Path) {
    match value {
        serde_json::Value::Object(map) => {
            let is_command_type =
                map.get("type").and_then(|v| v.as_str()).is_some_and(|t| t == "command");
            if is_command_type {
                if let Some(cmd_val) = map.get_mut("command") {
                    if let Some(cmd_str) = cmd_val.as_str() {
                        let rewritten = rewrite_single_command(cmd_str, project_root);
                        *cmd_val = serde_json::Value::String(rewritten);
                    }
                }
            }
            for v in map.values_mut() {
                rewrite_commands_recursive(v, project_root);
            }
        },
        serde_json::Value::Array(arr) => {
            for v in arr {
                rewrite_commands_recursive(v, project_root);
            }
        },
        _ => {},
    }
}

/// Rewrite a single command string's script path from relative to absolute.
fn rewrite_single_command(cmd: &str, project_root: &Path) -> String {
    let parts: Vec<&str> = cmd.splitn(2, char::is_whitespace).collect();
    let script = parts.first().copied().unwrap_or(cmd);

    // Only rewrite relative paths (starts with ./ or contains / but not absolute)
    let path = Path::new(script);
    if path.is_absolute() || (!script.starts_with("./") && !script.contains('/')) {
        return cmd.to_string();
    }

    let absolute = project_root.join(script);
    let abs_str = absolute.to_string_lossy();

    if parts.len() > 1 {
        format!("{abs_str} {}", parts.get(1).copied().unwrap_or(""))
    } else {
        abs_str.into_owned()
    }
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
        } else {
            // Indented line: either start a new key (if none yet) or continue the current value.
            if current_key.is_none() {
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
            } else if current_value.is_none() {
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

/// Serializable structure for `aipm.toml` generation.
#[derive(Serialize)]
struct PluginToml {
    package: PluginPackage,
    components: PluginComponents,
}

/// The `[package]` table of `aipm.toml`.
#[derive(Serialize)]
struct PluginPackage {
    name: String,
    version: String,
    #[serde(rename = "type")]
    kind: String,
    description: String,
}

/// The `[components]` table of `aipm.toml`.
#[derive(Default, Serialize)]
struct PluginComponents {
    #[serde(skip_serializing_if = "Option::is_none")]
    skills: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agents: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mcp_servers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hooks: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_styles: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scripts: Option<Vec<String>>,
}

/// Generate `aipm.toml` for a migrated plugin.
fn generate_plugin_manifest(artifact: &Artifact, plugin_name: &str) -> String {
    let type_str = artifact.kind.to_type_string();

    let description =
        artifact.metadata.description.as_deref().unwrap_or("Migrated from .claude/ configuration");

    let mut components = PluginComponents::default();

    match artifact.kind {
        ArtifactKind::Skill | ArtifactKind::Command => {
            components.skills = Some(vec![format!("skills/{}/SKILL.md", artifact.name)]);
        },
        ArtifactKind::Agent => {
            components.agents = Some(vec![format!("agents/{}.md", artifact.name)]);
        },
        ArtifactKind::McpServer => {
            components.mcp_servers = Some(vec![".mcp.json".to_string()]);
        },
        ArtifactKind::Hook => {
            components.hooks = Some(vec!["hooks/hooks.json".to_string()]);
        },
        ArtifactKind::OutputStyle => {
            components.output_styles = Some(vec![format!("{}.md", artifact.name)]);
        },
    }

    // Scripts component (if any) — preserves relative path structure
    if !artifact.referenced_scripts.is_empty() {
        let scripts_root = Path::new("scripts");
        let scripts: Vec<String> = artifact
            .referenced_scripts
            .iter()
            .map(|p| {
                let relative = p.strip_prefix(scripts_root).unwrap_or(p);
                format!("scripts/{}", relative.to_string_lossy())
            })
            .collect();
        components.scripts = Some(scripts);
    }

    // Hooks component (if extracted from skill/command frontmatter)
    if artifact.metadata.hooks.is_some() && artifact.kind != ArtifactKind::Hook {
        components.hooks = Some(vec!["hooks/hooks.json".to_string()]);
    }

    let manifest = PluginToml {
        package: PluginPackage {
            name: plugin_name.to_string(),
            version: "0.1.0".to_string(),
            kind: type_str.to_string(),
            description: description.to_string(),
        },
        components,
    };

    toml::to_string_pretty(&manifest).unwrap_or_default()
}

/// Generate `.claude-plugin/plugin.json` for a migrated plugin.
///
/// For single-artifact plugins, includes the component field for that kind.
/// Accepts a slice of kinds to include all relevant component fields for composites.
fn generate_plugin_json(name: &str, metadata: &ArtifactMetadata, kind: &ArtifactKind) -> String {
    generate_plugin_json_multi(name, metadata, std::slice::from_ref(kind))
}

/// Generate `plugin.json` with component fields for all provided artifact kinds.
fn generate_plugin_json_multi(
    name: &str,
    metadata: &ArtifactMetadata,
    kinds: &[ArtifactKind],
) -> String {
    let description =
        metadata.description.as_deref().unwrap_or("Migrated from .claude/ configuration");

    let mut map = serde_json::Map::new();
    map.insert("name".to_string(), serde_json::Value::String(name.to_string()));
    map.insert("version".to_string(), serde_json::Value::String("0.1.0".to_string()));
    map.insert("description".to_string(), serde_json::Value::String(description.to_string()));

    let distinct: HashSet<&ArtifactKind> = kinds.iter().collect();
    if distinct.contains(&ArtifactKind::Skill) || distinct.contains(&ArtifactKind::Command) {
        map.insert("skills".to_string(), serde_json::Value::String("./skills/".to_string()));
    }
    if distinct.contains(&ArtifactKind::Agent) {
        map.insert("agents".to_string(), serde_json::Value::String("./agents/".to_string()));
    }
    if distinct.contains(&ArtifactKind::McpServer) {
        map.insert("mcpServers".to_string(), serde_json::Value::String("./.mcp.json".to_string()));
    }
    if distinct.contains(&ArtifactKind::Hook) {
        map.insert(
            "hooks".to_string(),
            serde_json::Value::String("./hooks/hooks.json".to_string()),
        );
    }
    if distinct.contains(&ArtifactKind::OutputStyle) {
        map.insert("outputStyles".to_string(), serde_json::Value::String("./".to_string()));
    }

    let obj = serde_json::Value::Object(map);
    let mut output = serde_json::to_string_pretty(&obj).unwrap_or_default();
    output.push('\n');
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Mutex;

    struct MockFs {
        exists: HashSet<PathBuf>,
        dirs: HashMap<PathBuf, Vec<crate::fs::DirEntry>>,
        files: HashMap<PathBuf, String>,
        written: Mutex<HashMap<PathBuf, Vec<u8>>>,
    }

    impl MockFs {
        fn new() -> Self {
            Self {
                exists: HashSet::new(),
                dirs: HashMap::new(),
                files: HashMap::new(),
                written: Mutex::new(HashMap::new()),
            }
        }

        fn get_written(&self, path: &Path) -> Option<String> {
            self.written
                .lock()
                .expect("MockFs::get_written: mutex poisoned")
                .get(path)
                .and_then(|b| String::from_utf8(b.clone()).ok())
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
            self.written
                .lock()
                .expect("MockFs::write_file: mutex poisoned")
                .insert(path.to_path_buf(), content.to_vec());
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
                ..ArtifactMetadata::default()
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
                model_invocation_disabled: true,
                ..ArtifactMetadata::default()
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
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);
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
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

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
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

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
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

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
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

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
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

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
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

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
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

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
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

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
                ..ArtifactMetadata::default()
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
        let json = generate_plugin_json("test", &metadata, &ArtifactKind::Skill);
        assert!(json.contains("Test desc"));
    }

    #[test]
    fn generate_plugin_json_no_description() {
        let json = generate_plugin_json("test", &ArtifactMetadata::default(), &ArtifactKind::Skill);
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
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

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
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);
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
    fn emit_rejects_unsafe_name_with_traversal() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/../etc/SKILL.md"), "bad".to_string());

        let existing = HashSet::new();
        let mut counter = 0;
        let mut artifact = make_skill_artifact();
        artifact.name = "../etc".to_string();
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);
        assert!(result.is_ok());
        if let Some((_, actions)) = result.ok() {
            assert!(actions.iter().any(|a| matches!(a, Action::Skipped { .. })));
        }
    }

    #[test]
    fn emit_rejects_unsafe_name_with_separator() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/a/b/SKILL.md"), "bad".to_string());

        let existing = HashSet::new();
        let mut counter = 0;
        let mut artifact = make_skill_artifact();
        artifact.name = "a/b".to_string();
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);
        assert!(result.is_ok());
        if let Some((_, actions)) = result.ok() {
            assert!(actions.iter().any(|a| matches!(a, Action::Skipped { .. })));
        }
    }

    #[test]
    fn inject_disable_deduplicates_existing_key() {
        let content = "---\nname: test\ndisable-model-invocation: false\n---\nbody";
        let result = inject_disable_model_invocation(content);
        assert!(result.contains("disable-model-invocation: true"));
        // Should only appear once
        assert_eq!(result.matches("disable-model-invocation").count(), 1);
    }

    #[test]
    fn is_safe_path_segment_rejects_empty() {
        assert!(!is_safe_path_segment(""));
    }

    #[test]
    fn is_safe_path_segment_rejects_dot() {
        assert!(!is_safe_path_segment("."));
    }

    #[test]
    fn is_safe_path_segment_rejects_dotdot() {
        assert!(!is_safe_path_segment(".."));
    }

    #[test]
    fn is_safe_path_segment_rejects_backslash() {
        assert!(!is_safe_path_segment("a\\b"));
    }

    #[test]
    fn is_safe_path_segment_accepts_valid() {
        assert!(is_safe_path_segment("deploy"));
        assert!(is_safe_path_segment("my-plugin-123"));
    }

    #[test]
    fn emit_skill_skips_referenced_scripts_from_file_copy() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());
        fs.files.insert(
            PathBuf::from("/src/skills/deploy/scripts/deploy.sh"),
            "#!/bin/bash".to_string(),
        );
        fs.exists.insert(PathBuf::from("/src/skills/deploy/scripts/deploy.sh"));

        let existing = HashSet::new();
        let mut counter = 0;
        let mut artifact = make_skill_artifact();
        artifact.files = vec![PathBuf::from("SKILL.md"), PathBuf::from("scripts/deploy.sh")];
        artifact.referenced_scripts = vec![PathBuf::from("scripts/deploy.sh")];
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

        // SKILL.md should be copied to skill dir
        assert!(fs.get_written(Path::new("/ai/deploy/skills/deploy/SKILL.md")).is_some());
        // scripts/deploy.sh should NOT be in skill dir (it's a referenced script)
        assert!(fs.get_written(Path::new("/ai/deploy/skills/deploy/scripts/deploy.sh")).is_none());
        // scripts/deploy.sh SHOULD be in the root scripts dir
        assert!(fs.get_written(Path::new("/ai/deploy/scripts/deploy.sh")).is_some());
    }

    #[test]
    fn emit_skill_keeps_unreferenced_scripts_in_skill_dir() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());
        fs.files.insert(
            PathBuf::from("/src/skills/deploy/scripts/helper.sh"),
            "#!/bin/bash".to_string(),
        );

        let existing = HashSet::new();
        let mut counter = 0;
        let mut artifact = make_skill_artifact();
        artifact.files = vec![PathBuf::from("SKILL.md"), PathBuf::from("scripts/helper.sh")];
        // helper.sh is NOT in referenced_scripts
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

        // Unreferenced scripts stay in the skill dir
        assert!(fs.get_written(Path::new("/ai/deploy/skills/deploy/scripts/helper.sh")).is_some());
    }

    #[test]
    fn emit_preserves_nested_script_paths() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());
        fs.files.insert(
            PathBuf::from("/src/skills/deploy/scripts/tools/deploy.sh"),
            "#!/bin/bash".to_string(),
        );
        fs.exists.insert(PathBuf::from("/src/skills/deploy/scripts/tools/deploy.sh"));

        let existing = HashSet::new();
        let mut counter = 0;
        let mut artifact = make_skill_artifact();
        artifact.referenced_scripts = vec![PathBuf::from("scripts/tools/deploy.sh")];
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

        // Nested path should be preserved under scripts/
        assert!(fs.get_written(Path::new("/ai/deploy/scripts/tools/deploy.sh")).is_some());
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
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

        // SKILL.md should have rewritten paths
        let skill = fs.get_written(Path::new("/ai/deploy/skills/deploy/SKILL.md"));
        assert!(skill.as_ref().is_some_and(|c| c.contains("${CLAUDE_SKILL_DIR}/../../scripts/")));
        // README.md should be copied as-is
        let readme = fs.get_written(Path::new("/ai/deploy/skills/deploy/README.md"));
        assert!(readme.is_some_and(|c| c == "readme"));
    }

    #[test]
    fn emit_plugin_with_name_creates_structure() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());

        let artifact = make_skill_artifact();
        let result = emit_plugin_with_name(&artifact, "my-plugin", Path::new("/ai"), true, &fs);
        assert!(result.is_ok());

        // Check that files were written at the specified plugin name
        assert!(fs.get_written(Path::new("/ai/my-plugin/aipm.toml")).is_some());
        assert!(fs.get_written(Path::new("/ai/my-plugin/.claude-plugin/plugin.json")).is_some());
        assert!(fs.get_written(Path::new("/ai/my-plugin/skills/deploy/SKILL.md")).is_some());
    }

    #[test]
    fn emit_plugin_with_name_command_artifact() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/commands/review.md"), "Review code".to_string());

        let artifact = make_command_artifact();
        let result = emit_plugin_with_name(&artifact, "review-plugin", Path::new("/ai"), true, &fs);
        assert!(result.is_ok());

        let skill = fs.get_written(Path::new("/ai/review-plugin/skills/review/SKILL.md"));
        assert!(skill.as_ref().is_some_and(|c| c.contains("disable-model-invocation: true")));
    }

    #[test]
    fn emit_plugin_with_name_with_scripts() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy".to_string());
        fs.files
            .insert(PathBuf::from("/src/skills/deploy/scripts/run.sh"), "#!/bin/bash".to_string());
        fs.exists.insert(PathBuf::from("/src/skills/deploy/scripts/run.sh"));

        let mut artifact = make_skill_artifact();
        artifact.referenced_scripts = vec![PathBuf::from("scripts/run.sh")];
        let result = emit_plugin_with_name(&artifact, "deploy", Path::new("/ai"), true, &fs);
        assert!(result.is_ok());

        assert!(fs.get_written(Path::new("/ai/deploy/scripts/run.sh")).is_some());
    }

    #[test]
    fn emit_plugin_with_name_with_hooks() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy".to_string());

        let mut artifact = make_skill_artifact();
        artifact.metadata.hooks = Some("PreToolUse: check".to_string());
        let result = emit_plugin_with_name(&artifact, "deploy", Path::new("/ai"), true, &fs);
        assert!(result.is_ok());

        assert!(fs.get_written(Path::new("/ai/deploy/hooks/hooks.json")).is_some());
    }

    #[test]
    fn emit_package_plugin_single_skill() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());

        let artifact = make_skill_artifact();
        let result = emit_package_plugin("auth", &[artifact], Path::new("/ai"), true, &fs);
        assert!(result.is_ok());

        // Check plugin structure
        assert!(fs.get_written(Path::new("/ai/auth/aipm.toml")).is_some());
        assert!(fs.get_written(Path::new("/ai/auth/.claude-plugin/plugin.json")).is_some());
        assert!(fs.get_written(Path::new("/ai/auth/skills/deploy/SKILL.md")).is_some());

        // Check aipm.toml content
        let toml = fs.get_written(Path::new("/ai/auth/aipm.toml"));
        assert!(toml.as_ref().is_some_and(|c| c.contains("name = \"auth\"")));
        assert!(toml.as_ref().is_some_and(|c| c.contains("type = \"skill\"")));
    }

    #[test]
    fn emit_package_plugin_multiple_artifacts_composite() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());
        fs.files.insert(PathBuf::from("/src/commands/review.md"), "Review code".to_string());

        let skill = make_skill_artifact();
        let cmd = make_command_artifact();
        let result = emit_package_plugin("auth", &[skill, cmd], Path::new("/ai"), true, &fs);
        assert!(result.is_ok());

        // Both artifacts should be present
        assert!(fs.get_written(Path::new("/ai/auth/skills/deploy/SKILL.md")).is_some());
        assert!(fs.get_written(Path::new("/ai/auth/skills/review/SKILL.md")).is_some());

        // Should be composite type since it has both skills and commands
        let toml = fs.get_written(Path::new("/ai/auth/aipm.toml"));
        assert!(toml.as_ref().is_some_and(|c| c.contains("type = \"composite\"")));
    }

    #[test]
    fn emit_package_plugin_merges_hooks() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());

        let mut artifact = make_skill_artifact();
        artifact.metadata.hooks = Some("PreToolUse: check_deploy".to_string());

        let result = emit_package_plugin("auth", &[artifact], Path::new("/ai"), true, &fs);
        assert!(result.is_ok());

        let hooks = fs.get_written(Path::new("/ai/auth/hooks/hooks.json"));
        assert!(hooks.is_some());
        assert!(hooks.as_ref().is_some_and(|c| c.contains("PreToolUse")));

        let toml = fs.get_written(Path::new("/ai/auth/aipm.toml"));
        assert!(toml.as_ref().is_some_and(|c| c.contains("hooks")));
    }

    #[test]
    fn emit_package_plugin_with_scripts() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy".to_string());
        fs.files
            .insert(PathBuf::from("/src/skills/deploy/scripts/run.sh"), "#!/bin/bash".to_string());
        fs.exists.insert(PathBuf::from("/src/skills/deploy/scripts/run.sh"));

        let mut artifact = make_skill_artifact();
        artifact.referenced_scripts = vec![PathBuf::from("scripts/run.sh")];

        let result = emit_package_plugin("auth", &[artifact], Path::new("/ai"), true, &fs);
        assert!(result.is_ok());

        assert!(fs.get_written(Path::new("/ai/auth/scripts/run.sh")).is_some());
        let toml = fs.get_written(Path::new("/ai/auth/aipm.toml"));
        assert!(toml.as_ref().is_some_and(|c| c.contains("scripts")));
    }

    #[test]
    fn emit_plugin_with_name_missing_script_skipped() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy".to_string());
        // Script referenced but does NOT exist on disk

        let mut artifact = make_skill_artifact();
        artifact.referenced_scripts = vec![PathBuf::from("scripts/missing.sh")];
        let result = emit_plugin_with_name(&artifact, "deploy", Path::new("/ai"), true, &fs);
        assert!(result.is_ok());
        assert!(fs.get_written(Path::new("/ai/deploy/scripts/missing.sh")).is_none());
    }

    #[test]
    fn emit_package_plugin_command_only() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/commands/review.md"), "Review code".to_string());

        let cmd = make_command_artifact();
        let result = emit_package_plugin("auth", &[cmd], Path::new("/ai"), true, &fs);
        assert!(result.is_ok());

        // Should be skill type (command converts to skill)
        let toml = fs.get_written(Path::new("/ai/auth/aipm.toml"));
        assert!(toml.as_ref().is_some_and(|c| c.contains("type = \"skill\"")));
        assert!(toml.as_ref().is_some_and(|c| !c.contains("composite")));
    }

    #[test]
    fn emit_package_plugin_missing_script() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy".to_string());

        let mut artifact = make_skill_artifact();
        artifact.referenced_scripts = vec![PathBuf::from("scripts/missing.sh")];

        let result = emit_package_plugin("auth", &[artifact], Path::new("/ai"), true, &fs);
        assert!(result.is_ok());
        // Missing script should not be written
        assert!(fs.get_written(Path::new("/ai/auth/scripts/missing.sh")).is_none());
    }

    #[test]
    fn emit_package_plugin_empty_artifacts() {
        let fs = MockFs::new();
        let result = emit_package_plugin("empty", &[], Path::new("/ai"), true, &fs);
        assert!(result.is_ok());
        let actions = result.ok().unwrap_or_default();
        assert!(actions.is_empty());
    }

    #[test]
    fn emit_plugin_no_manifest_skips_aipm_toml() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());
        let artifact = make_skill_artifact();
        let existing = HashSet::new();
        let mut counter = 0;
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, false, &fs);
        assert!(result.is_ok());
        // aipm.toml should NOT be written
        assert!(fs.get_written(Path::new("/ai/deploy/aipm.toml")).is_none());
        // plugin.json should still be written
        assert!(fs.get_written(Path::new("/ai/deploy/.claude-plugin/plugin.json")).is_some());
    }

    #[test]
    fn emit_plugin_with_name_no_manifest_skips_aipm_toml() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());
        let artifact = make_skill_artifact();
        let result = emit_plugin_with_name(&artifact, "deploy", Path::new("/ai"), false, &fs);
        assert!(result.is_ok());
        assert!(fs.get_written(Path::new("/ai/deploy/aipm.toml")).is_none());
        assert!(fs.get_written(Path::new("/ai/deploy/.claude-plugin/plugin.json")).is_some());
    }

    #[test]
    fn emit_package_plugin_no_manifest_skips_aipm_toml() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());
        let artifact = make_skill_artifact();
        let result = emit_package_plugin("auth", &[artifact], Path::new("/ai"), false, &fs);
        assert!(result.is_ok());
        assert!(fs.get_written(Path::new("/ai/auth/aipm.toml")).is_none());
        assert!(fs.get_written(Path::new("/ai/auth/.claude-plugin/plugin.json")).is_some());
    }

    // =====================================================================
    // Tests for new artifact types: Agent, McpServer, Hook, OutputStyle
    // =====================================================================

    fn make_agent_artifact() -> Artifact {
        Artifact {
            kind: ArtifactKind::Agent,
            name: "reviewer".to_string(),
            source_path: PathBuf::from("/src/agents/reviewer.md"),
            files: vec![PathBuf::from("reviewer.md")],
            referenced_scripts: Vec::new(),
            metadata: ArtifactMetadata {
                name: Some("reviewer".to_string()),
                description: Some("Reviews code".to_string()),
                ..ArtifactMetadata::default()
            },
        }
    }

    fn make_mcp_artifact() -> Artifact {
        Artifact {
            kind: ArtifactKind::McpServer,
            name: "project-mcp-servers".to_string(),
            source_path: PathBuf::from("/project/.mcp.json"),
            files: vec![PathBuf::from(".mcp.json")],
            referenced_scripts: Vec::new(),
            metadata: ArtifactMetadata {
                name: Some("project-mcp-servers".to_string()),
                description: Some("2 MCP server(s) from .mcp.json".to_string()),
                raw_content: Some(r#"{"mcpServers":{"s1":{},"s2":{}}}"#.to_string()),
                ..ArtifactMetadata::default()
            },
        }
    }

    fn make_hook_artifact() -> Artifact {
        Artifact {
            kind: ArtifactKind::Hook,
            name: "project-hooks".to_string(),
            source_path: PathBuf::from("/project/.claude/settings.json"),
            files: Vec::new(),
            referenced_scripts: Vec::new(),
            metadata: ArtifactMetadata {
                name: Some("project-hooks".to_string()),
                description: Some("Hooks from settings.json".to_string()),
                raw_content: Some(
                    r#"{"hooks":{"PreToolUse":[{"type":"command","command":"echo check"}]}}"#
                        .to_string(),
                ),
                ..ArtifactMetadata::default()
            },
        }
    }

    fn make_output_style_artifact() -> Artifact {
        Artifact {
            kind: ArtifactKind::OutputStyle,
            name: "concise".to_string(),
            source_path: PathBuf::from("/src/output-styles/concise.md"),
            files: vec![PathBuf::from("concise.md")],
            referenced_scripts: Vec::new(),
            metadata: ArtifactMetadata {
                name: Some("concise".to_string()),
                description: Some("Short outputs".to_string()),
                ..ArtifactMetadata::default()
            },
        }
    }

    #[test]
    fn emit_agent_creates_agents_dir() {
        let mut fs = MockFs::new();
        fs.files.insert(
            PathBuf::from("/src/agents/reviewer.md"),
            "You are a code reviewer.".to_string(),
        );

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_agent_artifact();
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);
        assert!(result.is_ok());

        assert!(fs.get_written(Path::new("/ai/reviewer/agents/reviewer.md")).is_some());
        let toml = fs.get_written(Path::new("/ai/reviewer/aipm.toml"));
        assert!(toml.as_ref().is_some_and(|c| c.contains("type = \"agent\"")));
        assert!(toml.as_ref().is_some_and(|c| c.contains("agents = [\"agents/reviewer.md\"]")));
    }

    #[test]
    fn emit_agent_plugin_json_has_agents_field() {
        let mut fs = MockFs::new();
        fs.files.insert(
            PathBuf::from("/src/agents/reviewer.md"),
            "You are a code reviewer.".to_string(),
        );

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_agent_artifact();
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

        let json = fs.get_written(Path::new("/ai/reviewer/.claude-plugin/plugin.json"));
        assert!(json.as_ref().is_some_and(|c| c.contains("\"agents\"")));
    }

    #[test]
    fn emit_mcp_writes_mcp_json() {
        let fs = MockFs::new();

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_mcp_artifact();
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);
        assert!(result.is_ok());

        let mcp_content = fs.get_written(Path::new("/ai/project-mcp-servers/.mcp.json"));
        assert!(mcp_content.as_ref().is_some_and(|c| c.contains("mcpServers")));

        let toml = fs.get_written(Path::new("/ai/project-mcp-servers/aipm.toml"));
        assert!(toml.as_ref().is_some_and(|c| c.contains("type = \"mcp\"")));
        assert!(toml.as_ref().is_some_and(|c| c.contains("mcp_servers = [\".mcp.json\"]")));
    }

    #[test]
    fn emit_mcp_plugin_json_has_mcp_servers_field() {
        let fs = MockFs::new();

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_mcp_artifact();
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

        let json = fs.get_written(Path::new("/ai/project-mcp-servers/.claude-plugin/plugin.json"));
        assert!(json.as_ref().is_some_and(|c| c.contains("\"mcpServers\"")));
    }

    #[test]
    fn emit_hooks_writes_hooks_json() {
        let fs = MockFs::new();

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_hook_artifact();
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);
        assert!(result.is_ok());

        let hooks_content = fs.get_written(Path::new("/ai/project-hooks/hooks/hooks.json"));
        assert!(hooks_content.as_ref().is_some_and(|c| c.contains("hooks")));

        let toml = fs.get_written(Path::new("/ai/project-hooks/aipm.toml"));
        assert!(toml.as_ref().is_some_and(|c| c.contains("type = \"hook\"")));
        assert!(toml.as_ref().is_some_and(|c| c.contains("hooks = [\"hooks/hooks.json\"]")));
    }

    #[test]
    fn emit_hooks_plugin_json_has_hooks_field() {
        let fs = MockFs::new();

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_hook_artifact();
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

        let json = fs.get_written(Path::new("/ai/project-hooks/.claude-plugin/plugin.json"));
        assert!(json.as_ref().is_some_and(|c| c.contains("\"hooks\"")));
    }

    #[test]
    fn emit_hook_artifact_with_hooks_yaml_metadata_skips_redundant_extraction() {
        // A Hook artifact that also carries `metadata.hooks` (YAML string) should NOT
        // re-write hooks.json from the YAML — `emit_hooks_config` already wrote it from
        // `raw_content`. This exercises the `false` branch of
        // `if artifact.kind != ArtifactKind::Hook` in `emit_plugin`.
        let fs = MockFs::new();
        let existing = HashSet::new();
        let mut counter = 0;
        let mut artifact = make_hook_artifact();
        artifact.metadata.hooks = Some("PreToolUse: check_deploy".to_string());
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);
        assert!(result.is_ok());
        // hooks.json must still exist (written via emit_hooks_config from raw_content).
        let hooks_content = fs.get_written(Path::new("/ai/project-hooks/hooks/hooks.json"));
        assert!(hooks_content.is_some());
    }

    #[test]
    fn emit_output_style_copies_to_plugin_root() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/output-styles/concise.md"), "Be concise.".to_string());

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_output_style_artifact();
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);
        assert!(result.is_ok());

        let style_content = fs.get_written(Path::new("/ai/concise/concise.md"));
        assert!(style_content.is_some_and(|c| c == "Be concise."));

        let toml = fs.get_written(Path::new("/ai/concise/aipm.toml"));
        assert!(toml.as_ref().is_some_and(|c| c.contains("type = \"composite\"")));
        assert!(toml.as_ref().is_some_and(|c| c.contains("output_styles = [\"concise.md\"]")));
    }

    #[test]
    fn emit_output_style_plugin_json_has_output_styles_field() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/output-styles/concise.md"), "Be concise.".to_string());

        let existing = HashSet::new();
        let mut counter = 0;
        let artifact = make_output_style_artifact();
        let _result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);

        let json = fs.get_written(Path::new("/ai/concise/.claude-plugin/plugin.json"));
        assert!(json.as_ref().is_some_and(|c| c.contains("\"outputStyles\"")));
    }

    #[test]
    fn emit_plugin_with_name_agent() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/agents/reviewer.md"), "Agent content.".to_string());

        let artifact = make_agent_artifact();
        let result = emit_plugin_with_name(&artifact, "reviewer", Path::new("/ai"), true, &fs);
        assert!(result.is_ok());
        assert!(fs.get_written(Path::new("/ai/reviewer/agents/reviewer.md")).is_some());
    }

    #[test]
    fn emit_plugin_with_name_mcp() {
        let fs = MockFs::new();
        let artifact = make_mcp_artifact();
        let result =
            emit_plugin_with_name(&artifact, "project-mcp-servers", Path::new("/ai"), true, &fs);
        assert!(result.is_ok());
        assert!(fs.get_written(Path::new("/ai/project-mcp-servers/.mcp.json")).is_some());
    }

    #[test]
    fn emit_plugin_with_name_hook() {
        let fs = MockFs::new();
        let artifact = make_hook_artifact();
        let result = emit_plugin_with_name(&artifact, "project-hooks", Path::new("/ai"), true, &fs);
        assert!(result.is_ok());
        assert!(fs.get_written(Path::new("/ai/project-hooks/hooks/hooks.json")).is_some());
    }

    #[test]
    fn emit_plugin_with_name_output_style() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/output-styles/concise.md"), "Be concise.".to_string());

        let artifact = make_output_style_artifact();
        let result = emit_plugin_with_name(&artifact, "concise", Path::new("/ai"), true, &fs);
        assert!(result.is_ok());
        assert!(fs.get_written(Path::new("/ai/concise/concise.md")).is_some());
    }

    #[test]
    fn emit_package_plugin_mixed_agent_and_skill() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy content".to_string());
        fs.files.insert(PathBuf::from("/src/agents/reviewer.md"), "Agent content.".to_string());

        let skill = make_skill_artifact();
        let agent = make_agent_artifact();
        let result = emit_package_plugin("auth", &[skill, agent], Path::new("/ai"), true, &fs);
        assert!(result.is_ok());

        assert!(fs.get_written(Path::new("/ai/auth/skills/deploy/SKILL.md")).is_some());
        assert!(fs.get_written(Path::new("/ai/auth/agents/reviewer.md")).is_some());

        let toml = fs.get_written(Path::new("/ai/auth/aipm.toml"));
        assert!(toml.as_ref().is_some_and(|c| c.contains("type = \"composite\"")));
        assert!(toml.as_ref().is_some_and(|c| c.contains("skills =")));
        assert!(toml.as_ref().is_some_and(|c| c.contains("agents =")));
    }

    #[test]
    fn emit_package_plugin_hook_and_mcp() {
        let fs = MockFs::new();

        let hook = make_hook_artifact();
        let mcp = make_mcp_artifact();
        let result = emit_package_plugin("infra", &[hook, mcp], Path::new("/ai"), true, &fs);
        assert!(result.is_ok());

        assert!(fs.get_written(Path::new("/ai/infra/hooks/hooks.json")).is_some());
        assert!(fs.get_written(Path::new("/ai/infra/.mcp.json")).is_some());

        let toml = fs.get_written(Path::new("/ai/infra/aipm.toml"));
        assert!(toml.as_ref().is_some_and(|c| c.contains("type = \"composite\"")));
        assert!(toml.as_ref().is_some_and(|c| c.contains("hooks =")));
        assert!(toml.as_ref().is_some_and(|c| c.contains("mcp_servers =")));
    }

    #[test]
    fn emit_hook_with_script_references() {
        let mut fs = MockFs::new();
        fs.files.insert(
            PathBuf::from("/project/.claude/scripts/validate.sh"),
            "#!/bin/bash\nexit 0".to_string(),
        );
        fs.exists.insert(PathBuf::from("/project/.claude/scripts/validate.sh"));

        let mut artifact = make_hook_artifact();
        artifact.referenced_scripts = vec![PathBuf::from("./scripts/validate.sh")];

        let existing = HashSet::new();
        let mut counter = 0;
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);
        assert!(result.is_ok());

        // Hook scripts use source_path.parent() for resolution
        assert!(fs.get_written(Path::new("/ai/project-hooks/hooks/hooks.json")).is_some());
    }

    #[test]
    fn emit_mcp_no_raw_content_falls_back_to_source() {
        let mut fs = MockFs::new();
        fs.files
            .insert(PathBuf::from("/project/.mcp.json"), r#"{"mcpServers":{"s1":{}}}"#.to_string());
        let mut artifact = make_mcp_artifact();
        artifact.metadata.raw_content = None;

        let existing = HashSet::new();
        let mut counter = 0;
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);
        assert!(result.is_ok());
        assert!(fs
            .get_written(Path::new("/ai/project-mcp-servers/.mcp.json"))
            .is_some_and(|c| c.contains("mcpServers")));
    }

    #[test]
    fn emit_mcp_no_raw_content_no_source_errors() {
        let fs = MockFs::new();
        let mut artifact = make_mcp_artifact();
        artifact.metadata.raw_content = None;

        let existing = HashSet::new();
        let mut counter = 0;
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);
        assert!(result.is_err());
    }

    #[test]
    fn emit_hooks_no_raw_content_falls_back_to_source() {
        let mut fs = MockFs::new();
        fs.files.insert(
            PathBuf::from("/project/.claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[]}}"#.to_string(),
        );
        let mut artifact = make_hook_artifact();
        artifact.metadata.raw_content = None;

        let existing = HashSet::new();
        let mut counter = 0;
        let result = emit_plugin(&artifact, Path::new("/ai"), &existing, &mut counter, true, &fs);
        assert!(result.is_ok());
        assert!(fs.get_written(Path::new("/ai/project-hooks/hooks/hooks.json")).is_some());
    }

    #[test]
    fn generate_plugin_json_agent_kind() {
        let json = generate_plugin_json("test", &ArtifactMetadata::default(), &ArtifactKind::Agent);
        assert!(json.contains("\"agents\": \"./agents/\""));
    }

    #[test]
    fn generate_plugin_json_mcp_kind() {
        let json =
            generate_plugin_json("test", &ArtifactMetadata::default(), &ArtifactKind::McpServer);
        assert!(json.contains("\"mcpServers\": \"./.mcp.json\""));
    }

    #[test]
    fn generate_plugin_json_hook_kind() {
        let json = generate_plugin_json("test", &ArtifactMetadata::default(), &ArtifactKind::Hook);
        assert!(json.contains("\"hooks\": \"./hooks/hooks.json\""));
    }

    #[test]
    fn generate_plugin_json_output_style_kind() {
        let json =
            generate_plugin_json("test", &ArtifactMetadata::default(), &ArtifactKind::OutputStyle);
        assert!(json.contains("\"outputStyles\": \"./\""));
    }

    #[test]
    fn generate_manifest_agent_kind() {
        let artifact = make_agent_artifact();
        let manifest = generate_plugin_manifest(&artifact, "reviewer");
        assert!(manifest.contains("type = \"agent\""));
        assert!(manifest.contains("agents = [\"agents/reviewer.md\"]"));
    }

    #[test]
    fn generate_manifest_mcp_kind() {
        let artifact = make_mcp_artifact();
        let manifest = generate_plugin_manifest(&artifact, "project-mcp-servers");
        assert!(manifest.contains("type = \"mcp\""));
        assert!(manifest.contains("mcp_servers = [\".mcp.json\"]"));
    }

    #[test]
    fn generate_manifest_hook_kind() {
        let artifact = make_hook_artifact();
        let manifest = generate_plugin_manifest(&artifact, "project-hooks");
        assert!(manifest.contains("type = \"hook\""));
        assert!(manifest.contains("hooks = [\"hooks/hooks.json\"]"));
    }

    #[test]
    fn generate_manifest_output_style_kind() {
        let artifact = make_output_style_artifact();
        let manifest = generate_plugin_manifest(&artifact, "concise");
        assert!(manifest.contains("type = \"composite\""));
        assert!(manifest.contains("output_styles = [\"concise.md\"]"));
    }

    #[test]
    fn emit_package_plugin_output_style_only() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/output-styles/concise.md"), "Be concise.".to_string());

        let artifact = make_output_style_artifact();
        let result = emit_package_plugin("styles", &[artifact], Path::new("/ai"), true, &fs);
        assert!(result.is_ok());

        let toml = fs.get_written(Path::new("/ai/styles/aipm.toml"));
        assert!(toml.as_ref().is_some_and(|c| c.contains("type = \"composite\"")));
        assert!(toml.as_ref().is_some_and(|c| c.contains("output_styles =")));
    }

    #[test]
    fn emit_package_plugin_mcp_only() {
        let fs = MockFs::new();

        let mcp = make_mcp_artifact();
        let result = emit_package_plugin("mcp", &[mcp], Path::new("/ai"), true, &fs);
        assert!(result.is_ok());

        let toml = fs.get_written(Path::new("/ai/mcp/aipm.toml"));
        assert!(toml.as_ref().is_some_and(|c| c.contains("type = \"mcp\"")));
    }

    #[test]
    fn emit_package_plugin_unsafe_plugin_name() {
        let fs = MockFs::new();
        let result = emit_package_plugin("../evil", &[], Path::new("/ai"), true, &fs);
        assert!(result.is_ok());
        let actions = result.ok().unwrap_or_default();
        assert!(actions.iter().any(|a| matches!(a, Action::Skipped { .. })));
    }

    #[test]
    fn emit_package_plugin_unsafe_artifact_name() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy".to_string());
        let mut artifact = make_skill_artifact();
        artifact.name = "../bad".to_string();
        let result = emit_package_plugin("auth", &[artifact], Path::new("/ai"), true, &fs);
        assert!(result.is_ok());
        let actions = result.ok().unwrap_or_default();
        assert!(actions.iter().any(|a| matches!(a, Action::Skipped { .. })));
    }

    #[test]
    fn emit_plugin_with_name_unsafe_plugin_name() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy".to_string());
        let artifact = make_skill_artifact();
        let result = emit_plugin_with_name(&artifact, "../evil", Path::new("/ai"), true, &fs);
        assert!(result.is_ok());
        let actions = result.ok().unwrap_or_default();
        assert!(actions.iter().any(|a| matches!(a, Action::Skipped { .. })));
    }

    #[test]
    fn emit_plugin_with_name_unsafe_artifact_name() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/src/skills/deploy/SKILL.md"), "Deploy".to_string());
        let mut artifact = make_skill_artifact();
        artifact.name = "a/b".to_string();
        let result = emit_plugin_with_name(&artifact, "plugin", Path::new("/ai"), true, &fs);
        assert!(result.is_ok());
        let actions = result.ok().unwrap_or_default();
        assert!(actions.iter().any(|a| matches!(a, Action::Skipped { .. })));
    }

    // =====================================================================
    // Tests for hook command path rewriting
    // =====================================================================

    #[test]
    fn rewrite_hook_command_paths_rewrites_relative() {
        let content = r#"{"hooks":{"PreToolUse":[{"type":"command","command":"./scripts/check.sh --strict"}]}}"#;
        let result =
            rewrite_hook_command_paths(content, Path::new("/project/.claude/settings.json"));
        // The relative ./scripts/check.sh should no longer appear as-is
        assert!(!result.contains("\"./scripts/check.sh"));
        assert!(result.contains("check.sh"));
        assert!(result.contains("--strict"));
    }

    #[test]
    fn rewrite_hook_command_paths_leaves_absolute_alone() {
        let content =
            r#"{"hooks":{"PreToolUse":[{"type":"command","command":"/usr/bin/check --strict"}]}}"#;
        let result =
            rewrite_hook_command_paths(content, Path::new("/project/.claude/settings.json"));
        assert!(result.contains("/usr/bin/check"));
    }

    #[test]
    fn rewrite_hook_command_paths_leaves_bare_commands() {
        let content =
            r#"{"hooks":{"PreToolUse":[{"type":"command","command":"echo hello world"}]}}"#;
        let result =
            rewrite_hook_command_paths(content, Path::new("/project/.claude/settings.json"));
        assert!(result.contains("echo hello world"));
    }

    #[test]
    fn rewrite_hook_command_paths_invalid_json() {
        let content = "not json at all";
        let result =
            rewrite_hook_command_paths(content, Path::new("/project/.claude/settings.json"));
        assert_eq!(result, "not json at all");
    }

    #[test]
    fn rewrite_hook_command_paths_no_args() {
        let content =
            r#"{"hooks":{"PreToolUse":[{"type":"command","command":"./scripts/run.sh"}]}}"#;
        let result =
            rewrite_hook_command_paths(content, Path::new("/project/.claude/settings.json"));
        assert!(!result.contains("\"./scripts/run.sh\""));
        assert!(result.contains("run.sh"));
    }

    #[test]
    fn rewrite_hook_command_paths_nested_arrays() {
        let content = r#"{"hooks":{"PreToolUse":[{"type":"command","command":"./a.sh"},{"type":"command","command":"./b.sh"}]}}"#;
        let result =
            rewrite_hook_command_paths(content, Path::new("/project/.claude/settings.json"));
        // Both should be rewritten
        assert!(!result.contains("\"./a.sh\""));
        assert!(!result.contains("\"./b.sh\""));
    }

    #[test]
    fn rewrite_single_command_with_slash_path() {
        let result = rewrite_single_command("scripts/check.sh --flag", Path::new("/project"));
        assert!(!result.starts_with("scripts/check.sh"));
        assert!(result.contains("check.sh"));
        assert!(result.contains("--flag"));
    }

    #[test]
    fn rewrite_single_command_bare_command_unchanged() {
        let result = rewrite_single_command("echo hello", Path::new("/project"));
        assert_eq!(result, "echo hello");
    }

    #[test]
    fn generate_plugin_json_multi_composite() {
        let kinds = vec![ArtifactKind::Skill, ArtifactKind::Agent, ArtifactKind::OutputStyle];
        let json = generate_plugin_json_multi("test", &ArtifactMetadata::default(), &kinds);
        assert!(json.contains("\"skills\""));
        assert!(json.contains("\"agents\""));
        assert!(json.contains("\"outputStyles\""));
        assert!(!json.contains("\"mcpServers\""));
        assert!(!json.contains("\"hooks\""));
    }

    #[test]
    fn generate_plugin_json_multi_all_kinds() {
        let kinds = vec![
            ArtifactKind::Skill,
            ArtifactKind::Command,
            ArtifactKind::Agent,
            ArtifactKind::McpServer,
            ArtifactKind::Hook,
            ArtifactKind::OutputStyle,
        ];
        let json = generate_plugin_json_multi("test", &ArtifactMetadata::default(), &kinds);
        assert!(json.contains("\"skills\""));
        assert!(json.contains("\"agents\""));
        assert!(json.contains("\"mcpServers\""));
        assert!(json.contains("\"hooks\""));
        assert!(json.contains("\"outputStyles\""));
    }

    #[test]
    fn generate_plugin_json_valid_json_roundtrip() {
        let metadata = ArtifactMetadata {
            description: Some("Deploy app".to_string()),
            ..ArtifactMetadata::default()
        };
        let json = generate_plugin_json("test", &metadata, &ArtifactKind::Skill);
        let parsed: serde_json::Value = serde_json::from_str(&json).ok().unwrap_or_default();
        assert_eq!(
            parsed.get("description").and_then(serde_json::Value::as_str),
            Some("Deploy app")
        );
        assert_eq!(parsed.get("name").and_then(serde_json::Value::as_str), Some("test"));
    }

    #[test]
    fn generate_plugin_json_description_with_special_chars() {
        let metadata = ArtifactMetadata {
            description: Some("She said \"hello\" and \\backslash".to_string()),
            ..ArtifactMetadata::default()
        };
        let json = generate_plugin_json("test", &metadata, &ArtifactKind::Skill);
        let parsed: serde_json::Value = serde_json::from_str(&json).ok().unwrap_or_default();
        assert_eq!(
            parsed.get("description").and_then(serde_json::Value::as_str),
            Some("She said \"hello\" and \\backslash")
        );
    }

    #[test]
    fn generate_manifest_valid_toml_roundtrip() {
        let artifact = Artifact {
            kind: ArtifactKind::Skill,
            name: "deploy".to_string(),
            source_path: PathBuf::from("/src"),
            files: vec![PathBuf::from("SKILL.md")],
            referenced_scripts: Vec::new(),
            metadata: ArtifactMetadata {
                description: Some("Deploy app".to_string()),
                ..ArtifactMetadata::default()
            },
        };
        let manifest = generate_plugin_manifest(&artifact, "deploy");
        let parsed: toml::Value =
            toml::from_str(&manifest).ok().unwrap_or(toml::Value::Table(Default::default()));
        let desc =
            parsed.get("package").and_then(|p| p.get("description")).and_then(toml::Value::as_str);
        assert_eq!(desc, Some("Deploy app"));
    }

    #[test]
    fn generate_manifest_description_with_special_chars() {
        let artifact = Artifact {
            kind: ArtifactKind::Agent,
            name: "reviewer".to_string(),
            source_path: PathBuf::from("/src"),
            files: vec![],
            referenced_scripts: Vec::new(),
            metadata: ArtifactMetadata {
                description: Some("She said \"hello\" and \\backslash".to_string()),
                ..ArtifactMetadata::default()
            },
        };
        let manifest = generate_plugin_manifest(&artifact, "reviewer");
        let parsed: toml::Value =
            toml::from_str(&manifest).ok().unwrap_or(toml::Value::Table(Default::default()));
        let desc =
            parsed.get("package").and_then(|p| p.get("description")).and_then(toml::Value::as_str);
        assert_eq!(desc, Some("She said \"hello\" and \\backslash"));
    }
}
