//! Copilot skill detector: scans `.github/skills/` and `.github/copilot/` for directories
//! containing `SKILL.md`.
//!
//! Thin wrapper delegating to shared `skill_common` logic.

use std::path::Path;

use crate::fs::Fs;

use super::detector::Detector;
use super::skill_common;
use super::{Artifact, ArtifactKind, Error};

/// Scans `.github/skills/` and `.github/copilot/` (Copilot CLI) for directories containing
/// `SKILL.md`.
pub struct CopilotSkillDetector;

impl Detector for CopilotSkillDetector {
    fn name(&self) -> &'static str {
        "copilot-skill"
    }

    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error> {
        let mut artifacts = Vec::new();

        // Scan both `.github/skills/` (legacy) and `.github/copilot/` (default copilot-cli path)
        for subdir in &["skills", "copilot"] {
            let skills_dir = source_dir.join(subdir);
            if !fs.exists(&skills_dir) {
                continue;
            }

            let entries = fs.read_dir(&skills_dir)?;

            for entry in entries {
                if !entry.is_dir {
                    continue;
                }

                let entry_dir = skills_dir.join(&entry.name);
                let skill_md = entry_dir.join("SKILL.md");

                if !fs.exists(&skill_md) {
                    continue;
                }

                let content = fs.read_to_string(&skill_md)?;
                let metadata = skill_common::parse_frontmatter(&content, &skill_md)?;
                let files = skill_common::collect_files_recursive(&entry_dir, &entry_dir, fs)?;

                // Search for both Copilot and Claude skill dir variable references
                let mut referenced_scripts =
                    skill_common::extract_script_references(&content, "${SKILL_DIR}/");
                referenced_scripts.extend(skill_common::extract_script_references(
                    &content,
                    "${CLAUDE_SKILL_DIR}/",
                ));

                let name = metadata.name.clone().unwrap_or_else(|| entry.name.clone());

                artifacts.push(Artifact {
                    kind: ArtifactKind::Skill,
                    name,
                    source_path: entry_dir,
                    files,
                    referenced_scripts,
                    metadata,
                });
            }
        }

        Ok(artifacts)
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

    fn de(name: &str, is_dir: bool) -> crate::fs::DirEntry {
        crate::fs::DirEntry { name: name.to_string(), is_dir }
    }

    #[test]
    fn detect_skill_from_github_skills() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/skills"));
        fs.exists.insert(PathBuf::from("/project/.github/skills/lint/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/project/.github/skills"), vec![de("lint", true)]);
        fs.dirs.insert(PathBuf::from("/project/.github/skills/lint"), vec![de("SKILL.md", false)]);
        fs.files.insert(
            PathBuf::from("/project/.github/skills/lint/SKILL.md"),
            "---\nname: lint\ndescription: Lint code\n---\nLint instructions".to_string(),
        );

        let detector = CopilotSkillDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("lint"));
        assert_eq!(artifacts.first().map(|a| &a.kind), Some(&ArtifactKind::Skill));
    }

    #[test]
    fn detect_empty_skills_dir() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/skills"));
        fs.dirs.insert(PathBuf::from("/project/.github/skills"), Vec::new());

        let detector = CopilotSkillDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn detect_no_skills_dir() {
        let fs = MockFs::new();
        let detector = CopilotSkillDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn detect_skill_frontmatter_name_overrides_dir() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/skills"));
        fs.exists.insert(PathBuf::from("/project/.github/skills/my-dir/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/project/.github/skills"), vec![de("my-dir", true)]);
        fs.dirs
            .insert(PathBuf::from("/project/.github/skills/my-dir"), vec![de("SKILL.md", false)]);
        fs.files.insert(
            PathBuf::from("/project/.github/skills/my-dir/SKILL.md"),
            "---\nname: custom-name\n---\nbody".to_string(),
        );

        let detector = CopilotSkillDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("custom-name"));
    }

    #[test]
    fn detect_skill_without_frontmatter_uses_dir_name() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/skills"));
        fs.exists.insert(PathBuf::from("/project/.github/skills/my-skill/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/project/.github/skills"), vec![de("my-skill", true)]);
        fs.dirs
            .insert(PathBuf::from("/project/.github/skills/my-skill"), vec![de("SKILL.md", false)]);
        fs.files.insert(
            PathBuf::from("/project/.github/skills/my-skill/SKILL.md"),
            "---\ndescription: no name\n---\nbody".to_string(),
        );

        let detector = CopilotSkillDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("my-skill"));
    }

    #[test]
    fn detect_skill_dir_refs_extracted() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/skills"));
        fs.exists.insert(PathBuf::from("/project/.github/skills/deploy/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/project/.github/skills"), vec![de("deploy", true)]);
        fs.dirs
            .insert(PathBuf::from("/project/.github/skills/deploy"), vec![de("SKILL.md", false)]);
        fs.files.insert(
            PathBuf::from("/project/.github/skills/deploy/SKILL.md"),
            "---\nname: deploy\n---\nRun `${SKILL_DIR}/scripts/run.sh`".to_string(),
        );

        let detector = CopilotSkillDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.referenced_scripts.len()), Some(1));
    }

    #[test]
    fn detect_claude_skill_dir_refs_also_extracted() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/skills"));
        fs.exists.insert(PathBuf::from("/project/.github/skills/deploy/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/project/.github/skills"), vec![de("deploy", true)]);
        fs.dirs
            .insert(PathBuf::from("/project/.github/skills/deploy"), vec![de("SKILL.md", false)]);
        fs.files.insert(
            PathBuf::from("/project/.github/skills/deploy/SKILL.md"),
            "---\nname: deploy\n---\nRun `${CLAUDE_SKILL_DIR}/scripts/run.sh`".to_string(),
        );

        let detector = CopilotSkillDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.referenced_scripts.len()), Some(1));
    }

    #[test]
    fn detect_skill_skips_non_dir_entries() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/skills"));
        fs.dirs.insert(PathBuf::from("/project/.github/skills"), vec![de("readme.txt", false)]);

        let detector = CopilotSkillDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn detect_skill_skips_dir_without_skill_md() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/skills"));
        fs.dirs.insert(PathBuf::from("/project/.github/skills"), vec![de("empty-dir", true)]);
        fs.dirs.insert(PathBuf::from("/project/.github/skills/empty-dir"), vec![]);

        let detector = CopilotSkillDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn detect_skill_from_github_copilot_dir() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/copilot"));
        fs.exists.insert(PathBuf::from("/project/.github/copilot/lint/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/project/.github/copilot"), vec![de("lint", true)]);
        fs.dirs.insert(PathBuf::from("/project/.github/copilot/lint"), vec![de("SKILL.md", false)]);
        fs.files.insert(
            PathBuf::from("/project/.github/copilot/lint/SKILL.md"),
            "---\nname: lint\ndescription: Lint code\n---\nLint instructions".to_string(),
        );

        let detector = CopilotSkillDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("lint"));
        assert_eq!(artifacts.first().map(|a| &a.kind), Some(&ArtifactKind::Skill));
    }

    #[test]
    fn detect_skills_from_both_github_skills_and_copilot_dirs() {
        let mut fs = MockFs::new();
        // skill in .github/skills/
        fs.exists.insert(PathBuf::from("/project/.github/skills"));
        fs.exists.insert(PathBuf::from("/project/.github/skills/deploy/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/project/.github/skills"), vec![de("deploy", true)]);
        fs.dirs
            .insert(PathBuf::from("/project/.github/skills/deploy"), vec![de("SKILL.md", false)]);
        fs.files.insert(
            PathBuf::from("/project/.github/skills/deploy/SKILL.md"),
            "---\nname: deploy\ndescription: Deploy skill\n---\nDeploy instructions".to_string(),
        );
        // skill in .github/copilot/
        fs.exists.insert(PathBuf::from("/project/.github/copilot"));
        fs.exists.insert(PathBuf::from("/project/.github/copilot/lint/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/project/.github/copilot"), vec![de("lint", true)]);
        fs.dirs.insert(PathBuf::from("/project/.github/copilot/lint"), vec![de("SKILL.md", false)]);
        fs.files.insert(
            PathBuf::from("/project/.github/copilot/lint/SKILL.md"),
            "---\nname: lint\ndescription: Lint code\n---\nLint instructions".to_string(),
        );

        let detector = CopilotSkillDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.len(), 2);
        let names: Vec<&str> = artifacts.iter().map(|a| a.name.as_str()).collect();
        assert!(names.contains(&"deploy"));
        assert!(names.contains(&"lint"));
    }

    #[test]
    fn detect_skill_from_copilot_dir_without_name_uses_dir_name() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.github/copilot"));
        fs.exists.insert(PathBuf::from("/project/.github/copilot/my-skill/SKILL.md"));
        fs.dirs.insert(PathBuf::from("/project/.github/copilot"), vec![de("my-skill", true)]);
        fs.dirs.insert(
            PathBuf::from("/project/.github/copilot/my-skill"),
            vec![de("SKILL.md", false)],
        );
        fs.files.insert(
            PathBuf::from("/project/.github/copilot/my-skill/SKILL.md"),
            "---\ndescription: no name\n---\nbody".to_string(),
        );

        let detector = CopilotSkillDetector;
        let result = detector.detect(Path::new("/project/.github"), &fs);
        assert!(result.is_ok());
        let artifacts = result.ok().unwrap_or_default();
        assert_eq!(artifacts.first().map(|a| a.name.as_str()), Some("my-skill"));
    }
}
