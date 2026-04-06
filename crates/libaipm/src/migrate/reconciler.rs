//! Reconciler: identifies files in a source directory not claimed by any detector.
//!
//! After all detectors have run, the reconciler enumerates every file in the
//! source directory and diffs against the set of files claimed by detector
//! artifacts. Unclaimed files become [`OtherFile`] entries, optionally
//! associated with an artifact that references them.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::fs::Fs;

use super::skill_common::collect_files_recursive;
use super::{Artifact, ArtifactKind, Error, OtherFile};

/// Identify files in `source_dir` not claimed by any artifact.
///
/// Steps:
/// 1. Enumerate all files recursively under `source_dir`.
/// 2. Build a set of files claimed by the given artifacts.
/// 3. Diff to find unclaimed files.
/// 4. Associate unclaimed files with artifacts that reference them.
pub fn reconcile(
    source_dir: &Path,
    artifacts: &[Artifact],
    fs: &dyn Fs,
) -> Result<Vec<OtherFile>, Error> {
    let all_files = collect_all_files(source_dir, fs)?;
    let claimed = build_claimed_set(source_dir, artifacts);
    let unclaimed: Vec<PathBuf> = all_files.into_iter().filter(|f| !claimed.contains(f)).collect();

    Ok(associate_with_artifacts(source_dir, &unclaimed, artifacts))
}

/// Recursively collect all files under `source_dir` as absolute paths.
fn collect_all_files(source_dir: &Path, fs: &dyn Fs) -> Result<Vec<PathBuf>, Error> {
    let relative_paths = collect_files_recursive(source_dir, source_dir, fs)?;
    Ok(relative_paths.into_iter().map(|rel| source_dir.join(rel)).collect())
}

/// Build a set of absolute paths claimed by the given artifacts.
fn build_claimed_set(source_dir: &Path, artifacts: &[Artifact]) -> HashSet<PathBuf> {
    let mut claimed = HashSet::new();

    for artifact in artifacts {
        // The source_path itself is claimed
        claimed.insert(artifact.source_path.clone());

        // For directory-based artifacts (skills, extensions), claim all files relative to source_path
        if matches!(artifact.kind, ArtifactKind::Skill | ArtifactKind::Extension) {
            for file in &artifact.files {
                claimed.insert(artifact.source_path.join(file));
            }
        }

        // Referenced scripts are relative to the source dir
        for script in &artifact.referenced_scripts {
            claimed.insert(source_dir.join(script));
        }
    }

    claimed
}

/// Check each unclaimed file against artifact references and content.
fn associate_with_artifacts(
    source_dir: &Path,
    unclaimed: &[PathBuf],
    artifacts: &[Artifact],
) -> Vec<OtherFile> {
    let mut other_files = Vec::new();

    for path in unclaimed {
        let relative_path = path.strip_prefix(source_dir).unwrap_or(path).to_path_buf();
        // Always false when walking source_dir; reserved for future cross-boundary detection.
        let is_external = !path.starts_with(source_dir);

        // Check if any artifact references this file
        let associated = find_associated_artifact(&relative_path, artifacts);

        other_files.push(OtherFile {
            path: path.clone(),
            relative_path,
            associated_artifact: associated,
            is_external,
        });
    }

    other_files
}

/// Strip a leading `./` prefix from a path string.
fn strip_dot_slash(p: &Path) -> &Path {
    p.strip_prefix("./").unwrap_or(p)
}

/// Find the first artifact that references the given relative path.
fn find_associated_artifact(relative_path: &Path, artifacts: &[Artifact]) -> Option<String> {
    let path_str = relative_path.to_string_lossy();
    let normalized = strip_dot_slash(relative_path);

    for artifact in artifacts {
        // Check referenced_scripts
        for script in &artifact.referenced_scripts {
            if strip_dot_slash(script) == normalized {
                return Some(artifact.name.clone());
            }
        }

        // Check raw_content for the path string
        if let Some(ref content) = artifact.metadata.raw_content {
            if content.contains(path_str.as_ref()) {
                return Some(artifact.name.clone());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrate::ArtifactMetadata;
    use std::collections::HashMap;
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

    fn make_artifact(name: &str, kind: ArtifactKind, source_path: &str) -> Artifact {
        Artifact {
            kind,
            name: name.to_string(),
            source_path: PathBuf::from(source_path),
            files: Vec::new(),
            referenced_scripts: Vec::new(),
            metadata: ArtifactMetadata::default(),
        }
    }

    #[test]
    fn reconcile_empty_dir() {
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from("/src"), Vec::new());

        let result = reconcile(Path::new("/src"), &[], &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn reconcile_all_claimed() {
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from("/src"), vec![de("agents", true)]);
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("reviewer.md", false)]);

        let artifacts =
            vec![make_artifact("reviewer", ArtifactKind::Agent, "/src/agents/reviewer.md")];

        let result = reconcile(Path::new("/src"), &artifacts, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn reconcile_unclaimed_files() {
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from("/src"),
            vec![de("agents", true), de("README.md", false), de("utils.sh", false)],
        );
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("reviewer.md", false)]);

        let artifacts =
            vec![make_artifact("reviewer", ArtifactKind::Agent, "/src/agents/reviewer.md")];

        let result = reconcile(Path::new("/src"), &artifacts, &fs);
        assert!(result.is_ok());
        let others = result.ok().unwrap_or_default();
        assert_eq!(others.len(), 2);
        let names: Vec<_> =
            others.iter().map(|o| o.relative_path.to_string_lossy().to_string()).collect();
        assert!(names.contains(&"README.md".to_string()));
        assert!(names.contains(&"utils.sh".to_string()));
    }

    #[test]
    fn reconcile_associated_dependency() {
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from("/src"), vec![de("agents", true), de("scripts", true)]);
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("reviewer.md", false)]);
        fs.dirs.insert(PathBuf::from("/src/scripts"), vec![de("deploy.sh", false)]);

        let mut artifact =
            make_artifact("reviewer", ArtifactKind::Agent, "/src/agents/reviewer.md");
        artifact.referenced_scripts.push(PathBuf::from("scripts/deploy.sh"));

        let result = reconcile(Path::new("/src"), &[artifact], &fs);
        assert!(result.is_ok());
        let others = result.ok().unwrap_or_default();
        // scripts/deploy.sh is claimed via referenced_scripts, so no other files
        assert!(others.is_empty());
    }

    #[test]
    fn reconcile_unassociated() {
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from("/src"), vec![de("random.txt", false)]);

        let result = reconcile(Path::new("/src"), &[], &fs);
        assert!(result.is_ok());
        let others = result.ok().unwrap_or_default();
        assert_eq!(others.len(), 1);
        assert!(others.first().map_or(true, |o| o.associated_artifact.is_none()));
    }

    #[test]
    fn reconcile_associated_via_raw_content() {
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from("/src"), vec![de("agents", true), de("helpers.sh", false)]);
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("builder.md", false)]);

        let mut artifact = make_artifact("builder", ArtifactKind::Agent, "/src/agents/builder.md");
        artifact.metadata.raw_content = Some("Run helpers.sh for setup".to_string());

        let result = reconcile(Path::new("/src"), &[artifact], &fs);
        assert!(result.is_ok());
        let others = result.ok().unwrap_or_default();
        assert_eq!(others.len(), 1);
        assert_eq!(others.first().and_then(|o| o.associated_artifact.as_deref()), Some("builder"));
    }

    #[test]
    fn reconcile_settings_json_claimed_by_mcp() {
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from("/src"), vec![de("settings.json", false)]);

        let artifact = make_artifact("my-mcp", ArtifactKind::McpServer, "/src/settings.json");

        let result = reconcile(Path::new("/src"), &[artifact], &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn reconcile_settings_json_claimed_by_hook() {
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from("/src"), vec![de("settings.json", false)]);

        let artifact = make_artifact("my-hook", ArtifactKind::Hook, "/src/settings.json");

        let result = reconcile(Path::new("/src"), &[artifact], &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn reconcile_skill_files_claimed() {
        // Skill artifacts claim their source_path and files relative to source_path
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from("/src"), vec![de("skills", true)]);
        fs.dirs.insert(PathBuf::from("/src/skills"), vec![de("deploy", true)]);
        fs.dirs.insert(PathBuf::from("/src/skills/deploy"), vec![de("SKILL.md", false)]);

        let mut artifact = make_artifact("deploy", ArtifactKind::Skill, "/src/skills/deploy");
        artifact.files = vec![PathBuf::from("SKILL.md")];

        let result = reconcile(Path::new("/src"), &[artifact], &fs);
        assert!(result.is_ok());
        // SKILL.md under skills/deploy is claimed by the skill
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn find_associated_via_referenced_scripts() {
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from("/src"), vec![de("agents", true), de("utils.sh", false)]);
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("builder.md", false)]);

        let mut artifact = make_artifact("builder", ArtifactKind::Agent, "/src/agents/builder.md");
        // Reference utils.sh directly (different from raw_content match)
        artifact.referenced_scripts.push(PathBuf::from("utils.sh"));

        let result = reconcile(Path::new("/src"), &[artifact], &fs);
        assert!(result.is_ok());
        let others = result.ok().unwrap_or_default();
        // utils.sh is claimed as a referenced script
        assert!(others.is_empty());
    }

    #[test]
    fn find_associated_no_raw_content() {
        // Artifact with no raw_content — None branch of if let
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from("/src"), vec![de("orphan.txt", false)]);

        let artifact = make_artifact("test", ArtifactKind::Agent, "/src/agents/test.md");

        let result = reconcile(Path::new("/src"), &[artifact], &fs);
        assert!(result.is_ok());
        let others = result.ok().unwrap_or_default();
        assert_eq!(others.len(), 1);
        assert!(others[0].associated_artifact.is_none());
    }

    #[test]
    fn find_associated_raw_content_no_match() {
        // Artifact with raw_content that doesn't contain the file name
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from("/src"), vec![de("orphan.txt", false)]);

        let mut artifact = make_artifact("test", ArtifactKind::Agent, "/src/agents/test.md");
        artifact.metadata.raw_content = Some("no mention of files here".to_string());

        let result = reconcile(Path::new("/src"), &[artifact], &fs);
        assert!(result.is_ok());
        let others = result.ok().unwrap_or_default();
        assert_eq!(others.len(), 1);
        assert!(others[0].associated_artifact.is_none());
    }

    #[test]
    fn find_associated_normalizes_dot_slash_prefix() {
        // Referenced script uses ./scripts/foo.sh but enumerated path is scripts/foo.sh
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from("/src"), vec![de("agents", true), de("scripts", true)]);
        fs.dirs.insert(PathBuf::from("/src/agents"), vec![de("builder.md", false)]);
        fs.dirs.insert(PathBuf::from("/src/scripts"), vec![de("foo.sh", false)]);

        let mut artifact = make_artifact("builder", ArtifactKind::Agent, "/src/agents/builder.md");
        // Reference with ./ prefix
        artifact.referenced_scripts.push(PathBuf::from("./scripts/foo.sh"));

        let result = reconcile(Path::new("/src"), &[artifact], &fs);
        assert!(result.is_ok());
        let others = result.ok().unwrap_or_default();
        // scripts/foo.sh should be claimed via normalized matching
        assert!(
            !others.iter().any(|o| o.relative_path == PathBuf::from("scripts/foo.sh")),
            "scripts/foo.sh should be claimed by ./scripts/foo.sh reference"
        );
    }

    #[test]
    fn reconcile_extension_files_claimed() {
        let mut fs = MockFs::new();
        fs.dirs.insert(PathBuf::from("/src"), vec![de("extensions", true)]);
        fs.dirs.insert(PathBuf::from("/src/extensions"), vec![de("my-ext", true)]);
        fs.dirs.insert(PathBuf::from("/src/extensions/my-ext"), vec![de("index.js", false)]);

        let mut artifact =
            make_artifact("my-ext", ArtifactKind::Extension, "/src/extensions/my-ext");
        artifact.files = vec![PathBuf::from("index.js")];

        let result = reconcile(Path::new("/src"), &[artifact], &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn reconcile_propagates_read_dir_error() {
        // MockFs with no directories configured — read_dir returns NotFound,
        // covering the ? error paths in collect_all_files / collect_files_recursive.
        let fs = MockFs::new();
        let result = reconcile(Path::new("/src"), &[], &fs);
        assert!(
            matches!(result, Err(Error::Io(ref e)) if e.kind() == std::io::ErrorKind::NotFound),
            "reconcile should propagate read_dir NotFound failure"
        );
    }

    #[test]
    fn find_associated_artifact_referenced_script_true_branch() {
        // Directly exercises the True branch of `strip_dot_slash(script) == normalized`
        // in find_associated_artifact, which is unreachable through reconcile() because
        // referenced_scripts are claimed before reaching the unclaimed list.
        let mut artifact =
            make_artifact("deployer", ArtifactKind::Agent, "/src/agents/deployer.md");
        artifact.referenced_scripts.push(PathBuf::from("scripts/deploy.sh"));

        let result = find_associated_artifact(Path::new("scripts/deploy.sh"), &[artifact]);
        assert_eq!(result, Some("deployer".to_string()));
    }

    #[test]
    fn find_associated_artifact_referenced_script_false_then_true_branch() {
        // First artifact has a non-matching script → exercises the False branch of the
        // strip_dot_slash comparison. Second artifact matches → exercises the True branch.
        let mut artifact1 = make_artifact("builder", ArtifactKind::Agent, "/src/agents/builder.md");
        artifact1.referenced_scripts.push(PathBuf::from("scripts/other.sh"));

        let mut artifact2 =
            make_artifact("deployer", ArtifactKind::Agent, "/src/agents/deployer.md");
        artifact2.referenced_scripts.push(PathBuf::from("scripts/deploy.sh"));

        let result =
            find_associated_artifact(Path::new("scripts/deploy.sh"), &[artifact1, artifact2]);
        assert_eq!(result, Some("deployer".to_string()));
    }
}
