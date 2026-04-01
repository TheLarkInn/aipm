//! Shared test utilities for lint rule tests.

#![cfg(test)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::fs::{DirEntry, Fs};

/// A mock filesystem for testing lint rules.
pub struct MockFs {
    pub exists: HashSet<PathBuf>,
    pub dirs: HashMap<PathBuf, Vec<DirEntry>>,
    pub files: HashMap<PathBuf, String>,
    written: Mutex<HashMap<PathBuf, Vec<u8>>>,
}

impl MockFs {
    pub fn new() -> Self {
        Self {
            exists: HashSet::new(),
            dirs: HashMap::new(),
            files: HashMap::new(),
            written: Mutex::new(HashMap::new()),
        }
    }

    /// Add a skill SKILL.md at `.ai/<plugin>/skills/<skill>/SKILL.md`.
    pub fn add_skill(&mut self, plugin: &str, skill: &str, content: &str) {
        let ai = PathBuf::from(".ai");
        let skills_dir = ai.join(plugin).join("skills");
        let skill_md = skills_dir.join(skill).join("SKILL.md");

        self.exists.insert(skills_dir.clone());
        self.exists.insert(skill_md.clone());

        let ai_entries = self.dirs.entry(ai).or_default();
        if !ai_entries.iter().any(|e| e.name == plugin) {
            ai_entries.push(DirEntry { name: plugin.to_string(), is_dir: true });
        }
        let skill_entries = self.dirs.entry(skills_dir).or_default();
        if !skill_entries.iter().any(|e| e.name == skill) {
            skill_entries.push(DirEntry { name: skill.to_string(), is_dir: true });
        }
        self.files.insert(skill_md, content.to_string());
    }

    /// Add an agent `.md` at `.ai/<plugin>/agents/<name>.md`.
    pub fn add_agent(&mut self, plugin: &str, name: &str, content: &str) {
        let ai = PathBuf::from(".ai");
        let agents_dir = ai.join(plugin).join("agents");
        let agent_md = agents_dir.join(format!("{name}.md"));

        self.exists.insert(agents_dir.clone());
        self.exists.insert(agent_md.clone());

        let ai_entries = self.dirs.entry(ai).or_default();
        if !ai_entries.iter().any(|e| e.name == plugin) {
            ai_entries.push(DirEntry { name: plugin.to_string(), is_dir: true });
        }
        let agent_entries = self.dirs.entry(agents_dir).or_default();
        if !agent_entries.iter().any(|e| e.name == format!("{name}.md")) {
            agent_entries.push(DirEntry { name: format!("{name}.md"), is_dir: false });
        }
        self.files.insert(agent_md, content.to_string());
    }

    /// Add a hooks.json at `.ai/<plugin>/hooks/hooks.json`.
    pub fn add_hooks(&mut self, plugin: &str, content: &str) {
        let ai = PathBuf::from(".ai");
        let hooks_dir = ai.join(plugin).join("hooks");
        let hooks_json = hooks_dir.join("hooks.json");

        self.exists.insert(hooks_json.clone());

        let ai_entries = self.dirs.entry(ai).or_default();
        if !ai_entries.iter().any(|e| e.name == plugin) {
            ai_entries.push(DirEntry { name: plugin.to_string(), is_dir: true });
        }
        self.files.insert(hooks_json, content.to_string());
    }

    /// Mark a path as existing (for exists() checks).
    pub fn add_existing(&mut self, path: &str) {
        self.exists.insert(PathBuf::from(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_fs_write_file() {
        let fs = MockFs::new();
        assert!(fs.write_file(Path::new("/tmp/test.txt"), b"hello").is_ok());
    }

    #[test]
    fn mock_fs_create_dir_all() {
        let fs = MockFs::new();
        assert!(fs.create_dir_all(Path::new("/tmp/dir")).is_ok());
    }

    #[test]
    fn mock_fs_read_to_string_not_found() {
        let fs = MockFs::new();
        assert!(fs.read_to_string(Path::new("/missing")).is_err());
    }

    #[test]
    fn mock_fs_read_dir_not_found() {
        let fs = MockFs::new();
        assert!(fs.read_dir(Path::new("/missing")).is_err());
    }

    #[test]
    fn mock_fs_exists_false() {
        let fs = MockFs::new();
        assert!(!fs.exists(Path::new("/nonexistent")));
    }

    #[test]
    fn mock_fs_add_existing() {
        let mut fs = MockFs::new();
        fs.add_existing("/tmp/file");
        assert!(fs.exists(Path::new("/tmp/file")));
    }

    #[test]
    fn mock_fs_add_skill_creates_entries() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "content");
        assert!(fs.exists(Path::new(".ai/p/skills/s/SKILL.md")));
        assert!(fs.read_to_string(Path::new(".ai/p/skills/s/SKILL.md")).is_ok());
    }

    #[test]
    fn mock_fs_add_agent_creates_entries() {
        let mut fs = MockFs::new();
        fs.add_agent("p", "reviewer", "content");
        assert!(fs.exists(Path::new(".ai/p/agents/reviewer.md")));
    }

    #[test]
    fn mock_fs_add_hooks_creates_entries() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", "{}");
        assert!(fs.exists(Path::new(".ai/p/hooks/hooks.json")));
    }

    #[test]
    fn mock_fs_no_duplicate_entries() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s1", "a");
        fs.add_skill("p", "s2", "b");
        // Plugin "p" should only appear once in .ai dir listing
        let entries = fs.read_dir(Path::new(".ai")).unwrap_or_default();
        let plugin_count = entries.iter().filter(|e| e.name == "p").count();
        assert_eq!(plugin_count, 1);
    }
}

impl Fs for MockFs {
    fn exists(&self, path: &Path) -> bool {
        self.exists.contains(path)
    }
    fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
        Ok(())
    }
    fn write_file(&self, path: &Path, content: &[u8]) -> std::io::Result<()> {
        self.written
            .lock()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?
            .insert(path.to_path_buf(), content.to_vec());
        Ok(())
    }
    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        self.files.get(path).cloned().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, format!("{}", path.display()))
        })
    }
    fn read_dir(&self, path: &Path) -> std::io::Result<Vec<DirEntry>> {
        self.dirs.get(path).cloned().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, format!("{}", path.display()))
        })
    }
}
