//! Shared scanning utilities for lint rules.
//!
//! Provides helpers to iterate over plugin directories, skills, agents, and hooks
//! without duplicating filesystem traversal logic across individual rules.

use std::path::{Path, PathBuf};

use crate::frontmatter::Frontmatter;
use crate::fs::Fs;

/// A skill found during scanning.
pub struct FoundSkill {
    /// Path to the SKILL.md file.
    pub path: PathBuf,
    /// Parsed frontmatter (if any).
    pub frontmatter: Option<Frontmatter>,
    /// Raw content of the file.
    pub content: String,
}

/// An agent found during scanning.
pub struct FoundAgent {
    /// Path to the agent .md file.
    pub path: PathBuf,
    /// Parsed frontmatter (if any).
    pub frontmatter: Option<Frontmatter>,
}

/// Scan all `SKILL.md` files across plugins in a marketplace directory.
///
/// Iterates `.ai/<plugin>/skills/<name>/SKILL.md` for each plugin directory.
pub fn scan_skills(marketplace_dir: &Path, fs: &dyn Fs) -> Vec<FoundSkill> {
    let mut found = Vec::new();
    let Ok(plugins) = fs.read_dir(marketplace_dir) else {
        return found;
    };

    for plugin in &plugins {
        if !plugin.is_dir {
            continue;
        }
        let skills_dir = marketplace_dir.join(&plugin.name).join("skills");
        if !fs.exists(&skills_dir) {
            continue;
        }
        let Ok(skill_entries) = fs.read_dir(&skills_dir) else {
            continue;
        };
        for skill in &skill_entries {
            if !skill.is_dir {
                continue;
            }
            let skill_md = skills_dir.join(&skill.name).join("SKILL.md");
            if !fs.exists(&skill_md) {
                continue;
            }
            let Ok(content) = fs.read_to_string(&skill_md) else {
                continue;
            };
            let frontmatter = crate::frontmatter::parse(&content).ok().flatten();
            found.push(FoundSkill { path: skill_md, frontmatter, content });
        }
    }

    found
}

/// Scan all agent `.md` files across plugins in a marketplace directory.
///
/// Iterates `.ai/<plugin>/agents/<name>.md` for each plugin directory.
pub fn scan_agents(marketplace_dir: &Path, fs: &dyn Fs) -> Vec<FoundAgent> {
    let mut found = Vec::new();
    let Ok(plugins) = fs.read_dir(marketplace_dir) else {
        return found;
    };

    for plugin in &plugins {
        if !plugin.is_dir {
            continue;
        }
        let agents_dir = marketplace_dir.join(&plugin.name).join("agents");
        if !fs.exists(&agents_dir) {
            continue;
        }
        let Ok(agent_entries) = fs.read_dir(&agents_dir) else {
            continue;
        };
        for agent in &agent_entries {
            let is_md = std::path::Path::new(&agent.name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
            if agent.is_dir || !is_md {
                continue;
            }
            let agent_md = agents_dir.join(&agent.name);
            let Ok(content) = fs.read_to_string(&agent_md) else {
                continue;
            };
            let frontmatter = crate::frontmatter::parse(&content).ok().flatten();
            found.push(FoundAgent { path: agent_md, frontmatter });
        }
    }

    found
}

/// Scan all `hooks/hooks.json` files across plugins in a marketplace directory.
///
/// Returns `(path, content)` pairs for each found hooks file.
pub fn scan_hook_files(marketplace_dir: &Path, fs: &dyn Fs) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    let Ok(plugins) = fs.read_dir(marketplace_dir) else {
        return found;
    };

    for plugin in &plugins {
        if !plugin.is_dir {
            continue;
        }
        let hooks_json = marketplace_dir.join(&plugin.name).join("hooks").join("hooks.json");
        if !fs.exists(&hooks_json) {
            continue;
        }
        let Ok(content) = fs.read_to_string(&hooks_json) else {
            continue;
        };
        found.push((hooks_json, content));
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::test_helpers::MockFs;

    // --- scan_skills tests ---

    #[test]
    fn scan_skills_empty_marketplace() {
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from(".ai"), vec![]);
        let skills = scan_skills(Path::new(".ai"), &fs);
        assert!(skills.is_empty());
    }

    #[test]
    fn scan_skills_nonexistent_marketplace() {
        let fs = MockFs::new();
        let skills = scan_skills(Path::new(".ai"), &fs);
        assert!(skills.is_empty());
    }

    #[test]
    fn scan_skills_plugin_is_file_not_dir() {
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from(".ai"),
            vec![crate::fs::DirEntry { name: "file.txt".to_string(), is_dir: false }],
        );
        let skills = scan_skills(Path::new(".ai"), &fs);
        assert!(skills.is_empty());
    }

    #[test]
    fn scan_skills_no_skills_dir() {
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from(".ai"),
            vec![crate::fs::DirEntry { name: "plugin".to_string(), is_dir: true }],
        );
        // skills dir does not exist
        let skills = scan_skills(Path::new(".ai"), &fs);
        assert!(skills.is_empty());
    }

    #[test]
    fn scan_skills_skill_entry_is_file() {
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from(".ai"),
            vec![crate::fs::DirEntry { name: "p".to_string(), is_dir: true }],
        );
        let skills_dir = PathBuf::from(".ai/p/skills");
        fs.exists.insert(skills_dir.clone());
        fs.dirs.insert(
            skills_dir,
            vec![crate::fs::DirEntry { name: "README.md".to_string(), is_dir: false }],
        );
        let skills = scan_skills(Path::new(".ai"), &fs);
        assert!(skills.is_empty());
    }

    #[test]
    fn scan_skills_no_skill_md() {
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from(".ai"),
            vec![crate::fs::DirEntry { name: "p".to_string(), is_dir: true }],
        );
        let skills_dir = PathBuf::from(".ai/p/skills");
        fs.exists.insert(skills_dir.clone());
        fs.dirs.insert(
            skills_dir,
            vec![crate::fs::DirEntry { name: "default".to_string(), is_dir: true }],
        );
        // SKILL.md does not exist
        let skills = scan_skills(Path::new(".ai"), &fs);
        assert!(skills.is_empty());
    }

    #[test]
    fn scan_skills_finds_skill() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\nbody");
        let skills = scan_skills(Path::new(".ai"), &fs);
        assert_eq!(skills.len(), 1);
        assert!(skills[0].frontmatter.is_some());
    }

    // --- scan_agents tests ---

    #[test]
    fn scan_agents_empty_marketplace() {
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from(".ai"), vec![]);
        let agents = scan_agents(Path::new(".ai"), &fs);
        assert!(agents.is_empty());
    }

    #[test]
    fn scan_agents_nonexistent_marketplace() {
        let fs = MockFs::new();
        let agents = scan_agents(Path::new(".ai"), &fs);
        assert!(agents.is_empty());
    }

    #[test]
    fn scan_agents_no_agents_dir() {
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from(".ai"),
            vec![crate::fs::DirEntry { name: "p".to_string(), is_dir: true }],
        );
        let agents = scan_agents(Path::new(".ai"), &fs);
        assert!(agents.is_empty());
    }

    #[test]
    fn scan_agents_agent_is_dir() {
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from(".ai"),
            vec![crate::fs::DirEntry { name: "p".to_string(), is_dir: true }],
        );
        let agents_dir = PathBuf::from(".ai/p/agents");
        fs.exists.insert(agents_dir.clone());
        fs.dirs.insert(
            agents_dir,
            vec![crate::fs::DirEntry { name: "subdir".to_string(), is_dir: true }],
        );
        let agents = scan_agents(Path::new(".ai"), &fs);
        assert!(agents.is_empty());
    }

    #[test]
    fn scan_agents_non_md_file() {
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from(".ai"),
            vec![crate::fs::DirEntry { name: "p".to_string(), is_dir: true }],
        );
        let agents_dir = PathBuf::from(".ai/p/agents");
        fs.exists.insert(agents_dir.clone());
        fs.dirs.insert(
            agents_dir,
            vec![crate::fs::DirEntry { name: "config.json".to_string(), is_dir: false }],
        );
        let agents = scan_agents(Path::new(".ai"), &fs);
        assert!(agents.is_empty());
    }

    #[test]
    fn scan_agents_finds_agent() {
        let mut fs = MockFs::new();
        fs.add_agent("p", "reviewer", "---\nname: reviewer\n---\nprompt");
        let agents = scan_agents(Path::new(".ai"), &fs);
        assert_eq!(agents.len(), 1);
        assert!(agents[0].frontmatter.is_some());
    }

    // --- scan_hook_files tests ---

    #[test]
    fn scan_hooks_empty_marketplace() {
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from(".ai"), vec![]);
        let hooks = scan_hook_files(Path::new(".ai"), &fs);
        assert!(hooks.is_empty());
    }

    #[test]
    fn scan_hooks_nonexistent_marketplace() {
        let fs = MockFs::new();
        let hooks = scan_hook_files(Path::new(".ai"), &fs);
        assert!(hooks.is_empty());
    }

    #[test]
    fn scan_hooks_no_hooks_file() {
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from(".ai"),
            vec![crate::fs::DirEntry { name: "p".to_string(), is_dir: true }],
        );
        let hooks = scan_hook_files(Path::new(".ai"), &fs);
        assert!(hooks.is_empty());
    }

    #[test]
    fn scan_hooks_finds_hooks() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "PreToolUse": [] }"#);
        let hooks = scan_hook_files(Path::new(".ai"), &fs);
        assert_eq!(hooks.len(), 1);
    }

    #[test]
    fn scan_hooks_plugin_is_file() {
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from(".ai"),
            vec![crate::fs::DirEntry { name: "file.txt".to_string(), is_dir: false }],
        );
        let hooks = scan_hook_files(Path::new(".ai"), &fs);
        assert!(hooks.is_empty());
    }
}
