//! Rule: `skill/missing-name` — SKILL.md missing `name` field in frontmatter.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::scan;

/// Checks that every `SKILL.md` in marketplace plugins has a `name` frontmatter field.
pub struct MissingName;

impl Rule for MissingName {
    fn id(&self) -> &'static str {
        "skill/missing-name"
    }

    fn name(&self) -> &'static str {
        "missing skill name"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/skill/missing-name.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("add a \"name\" field to the YAML frontmatter")
    }

    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();

        for skill in scan::scan_skills(source_dir, fs) {
            match skill.frontmatter {
                Some(ref fm) if fm.fields.get("name").is_some_and(|v| !v.trim().is_empty()) => {},
                Some(ref fm) => {
                    diagnostics.push(Diagnostic {
                        rule_id: self.id().to_string(),
                        severity: self.default_severity(),
                        message: "SKILL.md missing required field: name".to_string(),
                        file_path: skill.path,
                        line: Some(fm.start_line),
                        col: Some(1),
                        end_line: Some(fm.start_line),
                        end_col: Some(4),
                        source_type: ".ai".to_string(),
                        help_text: None,
                        help_url: None,
                    });
                },
                None => {
                    diagnostics.push(Diagnostic {
                        rule_id: self.id().to_string(),
                        severity: self.default_severity(),
                        message: "SKILL.md has no frontmatter".to_string(),
                        file_path: skill.path,
                        line: Some(1),
                        col: Some(1),
                        end_line: Some(1),
                        end_col: Some(4),
                        source_type: ".ai".to_string(),
                        help_text: None,
                        help_url: None,
                    });
                },
            }
        }

        Ok(diagnostics)
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let source_type = scan::source_type_from_path(file_path).to_string();
        let Some(skill) = scan::read_skill(file_path, fs) else {
            return Ok(vec![]);
        };
        let diag = match skill.frontmatter {
            Some(ref fm) if fm.fields.get("name").is_some_and(|v| !v.trim().is_empty()) => {
                return Ok(vec![]);
            },
            Some(ref fm) => Diagnostic {
                rule_id: self.id().to_string(),
                severity: self.default_severity(),
                message: "SKILL.md missing required field: name".to_string(),
                file_path: skill.path,
                line: Some(fm.start_line),
                col: Some(1),
                end_line: Some(fm.start_line),
                end_col: Some(4),
                source_type,
                help_text: None,
                help_url: None,
            },
            None => Diagnostic {
                rule_id: self.id().to_string(),
                severity: self.default_severity(),
                message: "SKILL.md has no frontmatter".to_string(),
                file_path: skill.path,
                line: Some(1),
                col: Some(1),
                end_line: Some(1),
                end_col: Some(4),
                source_type,
                help_text: None,
                help_url: None,
            },
        };
        Ok(vec![diag])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
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
        fn read_dir(&self, path: &Path) -> std::io::Result<Vec<crate::fs::DirEntry>> {
            self.dirs.get(path).cloned().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, format!("{}", path.display()))
            })
        }
    }

    fn setup_skill(fs: &mut MockFs, plugin: &str, skill: &str, content: &str) {
        let ai = PathBuf::from(".ai");
        let skills_dir = ai.join(plugin).join("skills");
        let skill_md = skills_dir.join(skill).join("SKILL.md");

        fs.exists.insert(skills_dir.clone());
        fs.exists.insert(skill_md.clone());

        // Plugin entry in .ai/ (avoid duplicates)
        let ai_entries = fs.dirs.entry(ai.clone()).or_default();
        if !ai_entries.iter().any(|e| e.name == plugin) {
            ai_entries.push(crate::fs::DirEntry { name: plugin.to_string(), is_dir: true });
        }
        // Skill entry in skills/ dir (avoid duplicates)
        let skill_entries = fs.dirs.entry(skills_dir).or_default();
        if !skill_entries.iter().any(|e| e.name == skill) {
            skill_entries.push(crate::fs::DirEntry { name: skill.to_string(), is_dir: true });
        }
        // Skill file
        fs.files.insert(skill_md, content.to_string());
    }

    #[test]
    fn no_finding_when_name_present() {
        let mut fs = MockFs::new();
        setup_skill(&mut fs, "my-plugin", "default", "---\nname: my-skill\n---\nbody");
        let result = MissingName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn finding_when_name_absent() {
        let mut fs = MockFs::new();
        setup_skill(&mut fs, "my-plugin", "default", "---\ndescription: test\n---\nbody");
        let result = MissingName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags.first().map(|d| d.rule_id.as_str()), Some("skill/missing-name"));
    }

    #[test]
    fn finding_when_no_frontmatter() {
        let mut fs = MockFs::new();
        setup_skill(&mut fs, "my-plugin", "default", "just plain text");
        let result = MissingName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn multiple_skills_checked() {
        let mut fs = MockFs::new();
        setup_skill(&mut fs, "my-plugin", "skill-a", "---\nname: a\n---\n");
        setup_skill(&mut fs, "my-plugin", "skill-b", "---\ndescription: b\n---\n");
        let result = MissingName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        // skill-a has name, skill-b does not
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn empty_ai_dir_no_findings() {
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from(".ai"), vec![]);
        let result = MissingName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    // --- check_file() tests ---

    #[test]
    fn check_file_no_file_returns_empty() {
        let fs = MockFs::new();
        let result = MissingName.check_file(Path::new(".ai/p/skills/s/SKILL.md"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_name_present_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: s\ndescription: test\n---\nbody".to_string());

        let result = MissingName.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_name_absent_diagnostic() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\ndescription: no name\n---\nbody".to_string());

        let result = MissingName.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/missing-name");
    }

    #[test]
    fn check_file_no_frontmatter_warns() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "just text without frontmatter".to_string());

        let result = MissingName.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/missing-name");
        assert_eq!(diags[0].line, Some(1));
    }

    #[test]
    fn missing_name_points_to_frontmatter_opener() {
        let mut fs = MockFs::new();
        setup_skill(&mut fs, "my-plugin", "default", "---\ndescription: test\n---\nbody");
        let diags = MissingName.check(Path::new(".ai"), &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        // Points to the --- opener on line 1
        assert_eq!(diags[0].line, Some(1));
        assert_eq!(diags[0].col, Some(1));
        assert_eq!(diags[0].end_line, Some(1));
        assert_eq!(diags[0].end_col, Some(4));
    }

    #[test]
    fn no_frontmatter_points_to_line_one() {
        let mut fs = MockFs::new();
        setup_skill(&mut fs, "my-plugin", "default", "just plain text");
        let diags = MissingName.check(Path::new(".ai"), &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, Some(1));
        assert_eq!(diags[0].col, Some(1));
        assert_eq!(diags[0].end_line, Some(1));
        assert_eq!(diags[0].end_col, Some(4));
    }

    #[test]
    fn setup_skill_deduplicates_skill_dir_entries() {
        // Cover the False branch of `if !skill_entries.iter().any(|e| e.name == skill)`
        // in `setup_skill`: calling with the same plugin+skill twice must not add a duplicate.
        let mut fs = MockFs::new();
        setup_skill(&mut fs, "my-plugin", "my-skill", "---\nname: my-skill\n---\nbody");
        // Second call with same plugin/skill — the skill dir entry already exists.
        setup_skill(&mut fs, "my-plugin", "my-skill", "---\nname: my-skill\n---\nupdated body");

        let skills_dir = PathBuf::from(".ai/my-plugin/skills");
        let entry_count = fs.dirs.get(&skills_dir).map(Vec::len).unwrap_or(0);
        assert_eq!(entry_count, 1, "skill dir entry should not be duplicated");

        let result = MissingName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }
}
