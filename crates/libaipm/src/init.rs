//! Package initialization and scaffolding for `aipm-pack init`.
//!
//! Creates a new plugin directory with an `aipm.toml` manifest and
//! conventional directory layout based on the plugin type.

use std::path::Path;

use crate::fs::Fs;
use crate::manifest::error::Error as ManifestError;
use crate::manifest::types::PluginType;

/// Options for initializing a new plugin package.
pub struct Options<'a> {
    /// Target directory to initialize in.
    pub dir: &'a Path,
    /// Package name (defaults to directory name).
    pub name: Option<&'a str>,
    /// Plugin type (defaults to composite).
    pub plugin_type: Option<PluginType>,
}

/// Errors specific to the init command.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The directory already has an aipm.toml.
    #[error("already initialized: aipm.toml already exists in {}", .0.display())]
    AlreadyInitialized(std::path::PathBuf),

    /// Invalid package name.
    #[error("invalid package name: {name} — {reason}")]
    InvalidName {
        /// The invalid name.
        name: String,
        /// Why it's invalid.
        reason: String,
    },

    /// Could not determine directory name.
    #[error("cannot determine package name from directory path")]
    NoDirectoryName,

    /// Manifest validation error.
    #[error(transparent)]
    Manifest(#[from] ManifestError),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Initialize a new plugin package in the given directory.
///
/// # Errors
///
/// Returns `Error` if the directory already contains an `aipm.toml`,
/// the package name is invalid, or I/O operations fail.
pub fn init(opts: &Options<'_>, fs: &dyn Fs) -> Result<(), Error> {
    let dir = opts.dir;

    // Check for existing manifest
    let manifest_path = dir.join("aipm.toml");
    if fs.exists(&manifest_path) {
        return Err(Error::AlreadyInitialized(dir.to_path_buf()));
    }

    // Determine package name
    let name = match opts.name {
        Some(n) => n.to_string(),
        None => dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(String::from)
            .ok_or(Error::NoDirectoryName)?,
    };

    // Validate name
    if !is_valid_package_name(&name) {
        return Err(Error::InvalidName {
            name,
            reason: "must be lowercase alphanumeric with hyphens, optionally scoped with @org/name"
                .to_string(),
        });
    }

    let plugin_type = opts.plugin_type.unwrap_or(PluginType::Composite);

    // Create directory structure
    fs.create_dir_all(dir)?;
    create_directory_layout(dir, plugin_type, fs)?;

    // Generate aipm.toml
    let toml_content = generate_manifest(&name, plugin_type);
    fs.write_file(&manifest_path, toml_content.as_bytes())?;

    Ok(())
}

/// Check if a package name is valid (same rules as manifest validation).
fn is_valid_package_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    if let Some(rest) = name.strip_prefix('@') {
        let Some(slash_pos) = rest.find('/') else {
            return false;
        };
        let scope = &rest[..slash_pos];
        let pkg = &rest[slash_pos + 1..];
        if scope.is_empty() || pkg.is_empty() {
            return false;
        }
        return is_valid_segment(scope) && is_valid_segment(pkg);
    }

    is_valid_segment(name)
}

fn is_valid_segment(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    if !bytes.first().is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit()) {
        return false;
    }
    bytes.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// Create the conventional directory layout for a plugin type.
fn create_directory_layout(
    dir: &Path,
    plugin_type: PluginType,
    fs: &dyn Fs,
) -> Result<(), std::io::Error> {
    match plugin_type {
        PluginType::Skill => {
            fs.create_dir_all(&dir.join("skills"))?;
            create_gitkeep(&dir.join("skills"), fs)?;
            create_skill_template(dir, fs)?;
        },
        PluginType::Agent => {
            fs.create_dir_all(&dir.join("agents"))?;
            create_gitkeep(&dir.join("agents"), fs)?;
        },
        PluginType::Mcp => {
            fs.create_dir_all(&dir.join("mcp"))?;
            create_gitkeep(&dir.join("mcp"), fs)?;
        },
        PluginType::Hook => {
            fs.create_dir_all(&dir.join("hooks"))?;
            create_gitkeep(&dir.join("hooks"), fs)?;
        },
        PluginType::Lsp => {
            // LSP plugins just need the .lsp.json config (generated separately)
        },
        PluginType::Composite => {
            fs.create_dir_all(&dir.join("skills"))?;
            fs.create_dir_all(&dir.join("agents"))?;
            fs.create_dir_all(&dir.join("hooks"))?;
            create_gitkeep(&dir.join("skills"), fs)?;
            create_gitkeep(&dir.join("agents"), fs)?;
            create_gitkeep(&dir.join("hooks"), fs)?;
        },
    }
    Ok(())
}

fn create_gitkeep(dir: &Path, fs: &dyn Fs) -> Result<(), std::io::Error> {
    fs.write_file(&dir.join(".gitkeep"), b"")
}

fn create_skill_template(dir: &Path, fs: &dyn Fs) -> Result<(), std::io::Error> {
    let skill_dir = dir.join("skills").join("default");
    fs.create_dir_all(&skill_dir)?;
    fs.write_file(
        &skill_dir.join("SKILL.md"),
        b"---\n\
        description: A starter skill template\n\
        ---\n\n\
        # Default Skill\n\n\
        Describe what this skill does and when Claude should invoke it.\n",
    )
}

/// Generate the `aipm.toml` manifest content.
fn generate_manifest(name: &str, plugin_type: PluginType) -> String {
    let type_str = match plugin_type {
        PluginType::Skill => "skill",
        PluginType::Agent => "agent",
        PluginType::Mcp => "mcp",
        PluginType::Hook => "hook",
        PluginType::Lsp => "lsp",
        PluginType::Composite => "composite",
    };

    format!(
        "[package]\n\
         name = \"{name}\"\n\
         version = \"0.1.0\"\n\
         type = \"{type_str}\"\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Real;

    #[test]
    fn valid_names() {
        assert!(is_valid_package_name("my-plugin"));
        assert!(is_valid_package_name("plugin123"));
        assert!(is_valid_package_name("@org/my-plugin"));
        // Digit-starting names (exercises is_ascii_digit branch in first-char check)
        assert!(is_valid_package_name("1abc"));
        assert!(is_valid_package_name("123"));
        // Hyphens in middle (exercises b == b'-' branch in all() iterator)
        assert!(is_valid_package_name("a-b-c"));
        // Scoped with digit-starting segments
        assert!(is_valid_package_name("@1org/2pkg"));
        // All-digit segments
        assert!(is_valid_package_name("@123/456"));
    }

    #[test]
    fn invalid_names() {
        assert!(!is_valid_package_name(""));
        assert!(!is_valid_package_name("INVALID_Name!"));
        assert!(!is_valid_package_name("has spaces"));
        assert!(!is_valid_package_name("-starts-dash"));
        // Scoped name edge cases (branch coverage)
        assert!(!is_valid_package_name("@noslash"));
        assert!(!is_valid_package_name("@/pkg"));
        assert!(!is_valid_package_name("@org/"));
        assert!(!is_valid_package_name("@ORG/my-plugin"));
        assert!(!is_valid_package_name("@org/INVALID"));
    }

    #[test]
    fn is_valid_segment_rejects_empty_string() {
        // is_valid_segment is never called with "" via is_valid_package_name (which guards
        // the empty-name case and the empty-scope/pkg cases before delegating).
        // Call it directly to cover the early-return branch.
        assert!(!is_valid_segment(""));
    }

    #[test]
    fn init_creates_manifest_and_dirs() {
        let tmp = std::env::temp_dir().join("aipm-test-init-basic");
        if tmp.exists() {
            let _ = std::fs::remove_dir_all(&tmp);
        }
        std::fs::create_dir_all(&tmp).ok();

        let opts = Options { dir: &tmp, name: Some("test-plugin"), plugin_type: None };
        let result = init(&opts, &Real);
        assert!(result.is_ok());

        // Manifest exists
        assert!(tmp.join("aipm.toml").exists());

        // Directories exist (composite default)
        assert!(tmp.join("skills").exists());
        assert!(tmp.join("agents").exists());
        assert!(tmp.join("hooks").exists());

        // Gitkeep exists
        assert!(tmp.join("skills/.gitkeep").exists());

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn init_uses_directory_name_as_default() {
        let tmp = std::env::temp_dir().join("aipm-test-init-dirname");
        if tmp.exists() {
            let _ = std::fs::remove_dir_all(&tmp);
        }
        std::fs::create_dir_all(&tmp).ok();

        let opts = Options { dir: &tmp, name: None, plugin_type: None };
        let result = init(&opts, &Real);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(tmp.join("aipm.toml"));
        assert!(content.is_ok());
        assert!(content.is_ok_and(|c| c.contains("aipm-test-init-dirname")));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn init_fails_if_already_initialized() {
        let tmp = std::env::temp_dir().join("aipm-test-init-exists");
        if tmp.exists() {
            let _ = std::fs::remove_dir_all(&tmp);
        }
        std::fs::create_dir_all(&tmp).ok();
        std::fs::File::create(tmp.join("aipm.toml")).ok();

        let opts = Options { dir: &tmp, name: Some("test"), plugin_type: None };
        let result = init(&opts, &Real);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("already initialized")));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn init_fails_for_invalid_name() {
        let tmp = std::env::temp_dir().join("aipm-test-init-badname");
        if tmp.exists() {
            let _ = std::fs::remove_dir_all(&tmp);
        }
        std::fs::create_dir_all(&tmp).ok();

        let opts = Options { dir: &tmp, name: Some("INVALID_Name!"), plugin_type: None };
        let result = init(&opts, &Real);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("invalid package name")));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn init_skill_type_creates_template() {
        let tmp = std::env::temp_dir().join("aipm-test-init-skill");
        if tmp.exists() {
            let _ = std::fs::remove_dir_all(&tmp);
        }
        std::fs::create_dir_all(&tmp).ok();

        let opts =
            Options { dir: &tmp, name: Some("my-skill"), plugin_type: Some(PluginType::Skill) };
        let result = init(&opts, &Real);
        assert!(result.is_ok());

        // Skill template created
        assert!(tmp.join("skills/default/SKILL.md").exists());

        // Manifest has skill type
        let content = std::fs::read_to_string(tmp.join("aipm.toml"));
        assert!(content.is_ok_and(|c| c.contains("type = \"skill\"")));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn init_each_type_sets_correct_manifest() {
        for (type_str, pt) in [
            ("skill", PluginType::Skill),
            ("agent", PluginType::Agent),
            ("mcp", PluginType::Mcp),
            ("hook", PluginType::Hook),
            ("lsp", PluginType::Lsp),
            ("composite", PluginType::Composite),
        ] {
            let tmp = std::env::temp_dir().join(format!("aipm-test-init-type-{type_str}"));
            if tmp.exists() {
                let _ = std::fs::remove_dir_all(&tmp);
            }
            std::fs::create_dir_all(&tmp).ok();

            let opts = Options { dir: &tmp, name: Some("test-pkg"), plugin_type: Some(pt) };
            let result = init(&opts, &Real);
            assert!(result.is_ok(), "init should succeed for type {type_str}");

            let content = std::fs::read_to_string(tmp.join("aipm.toml"));
            assert!(
                content.is_ok_and(|c| c.contains(&format!("type = \"{type_str}\""))),
                "manifest should contain type = \"{type_str}\""
            );

            let _ = std::fs::remove_dir_all(&tmp);
        }
    }

    #[test]
    fn generated_manifest_is_parseable() {
        let tmp = std::env::temp_dir().join("aipm-test-init-parseable");
        if tmp.exists() {
            let _ = std::fs::remove_dir_all(&tmp);
        }
        std::fs::create_dir_all(&tmp).ok();

        let opts = Options {
            dir: &tmp,
            name: Some("valid-plugin"),
            plugin_type: Some(PluginType::Composite),
        };
        let result = init(&opts, &Real);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(tmp.join("aipm.toml"));
        assert!(content.is_ok());
        let parsed = crate::manifest::parse_and_validate(content.as_deref().unwrap_or(""), None);
        assert!(parsed.is_ok(), "generated manifest should be valid");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // =====================================================================
    // Mock Fs tests — I/O error path coverage
    // =====================================================================

    struct FailFs {
        fail_on: &'static str,
    }

    impl crate::fs::Fs for FailFs {
        fn exists(&self, _: &Path) -> bool {
            false
        }

        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            if self.fail_on == "create_dir" {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "mock: read-only",
                ));
            }
            Ok(())
        }

        fn write_file(&self, _: &Path, _: &[u8]) -> std::io::Result<()> {
            if self.fail_on == "write_file" {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "mock: disk full"));
            }
            Ok(())
        }

        fn read_to_string(&self, _: &Path) -> std::io::Result<String> {
            Ok(String::new())
        }

        fn read_dir(&self, _: &Path) -> std::io::Result<Vec<crate::fs::DirEntry>> {
            Ok(Vec::new())
        }
    }

    /// Mock that succeeds N times then fails on the (N+1)th call.
    struct CountingFs {
        create_dir_fail_after: std::sync::atomic::AtomicU32,
        write_file_fail_after: std::sync::atomic::AtomicU32,
    }

    impl crate::fs::Fs for CountingFs {
        fn exists(&self, _: &Path) -> bool {
            false
        }

        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            let n = self.create_dir_fail_after.load(std::sync::atomic::Ordering::Relaxed);
            if n == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "mock: create_dir failed",
                ));
            }
            self.create_dir_fail_after.store(n - 1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }

        fn write_file(&self, _: &Path, _: &[u8]) -> std::io::Result<()> {
            let n = self.write_file_fail_after.load(std::sync::atomic::Ordering::Relaxed);
            if n == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "mock: write_file failed",
                ));
            }
            self.write_file_fail_after.store(n - 1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }

        fn read_to_string(&self, _: &Path) -> std::io::Result<String> {
            Ok(String::new())
        }

        fn read_dir(&self, _: &Path) -> std::io::Result<Vec<crate::fs::DirEntry>> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn init_fails_on_create_dir_error() {
        let fs = FailFs { fail_on: "create_dir" };
        let tmp = std::path::PathBuf::from("/tmp/fake-init-dir");
        let opts = Options { dir: &tmp, name: Some("test"), plugin_type: None };
        let result = init(&opts, &fs);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("mock")));
    }

    #[test]
    fn init_fails_on_write_file_error() {
        let fs = FailFs { fail_on: "write_file" };
        let tmp = std::path::PathBuf::from("/tmp/fake-init-write");
        // Lsp type skips directory layout (no create_dir in layout), so write_file is first to fail
        let opts = Options { dir: &tmp, name: Some("test"), plugin_type: Some(PluginType::Lsp) };
        let result = init(&opts, &fs);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("mock")));
    }

    #[test]
    fn init_skill_layout_fails_on_second_create_dir() {
        // First create_dir_all (init:88) succeeds, second (create_directory_layout:135) fails
        let fs = CountingFs {
            create_dir_fail_after: std::sync::atomic::AtomicU32::new(1),
            write_file_fail_after: std::sync::atomic::AtomicU32::new(u32::MAX),
        };
        let tmp = std::path::PathBuf::from("/tmp/fake-init-skill-dir");
        let opts = Options { dir: &tmp, name: Some("test"), plugin_type: Some(PluginType::Skill) };
        let result = init(&opts, &fs);
        assert!(result.is_err());
    }

    #[test]
    fn init_composite_write_fails_on_gitkeep() {
        // create_dir_all succeeds (all of them), but write_file fails (gitkeep)
        let fs = CountingFs {
            create_dir_fail_after: std::sync::atomic::AtomicU32::new(u32::MAX),
            write_file_fail_after: std::sync::atomic::AtomicU32::new(0),
        };
        let tmp = std::path::PathBuf::from("/tmp/fake-init-composite-write");
        let opts =
            Options { dir: &tmp, name: Some("test"), plugin_type: Some(PluginType::Composite) };
        let result = init(&opts, &fs);
        assert!(result.is_err());
    }

    #[test]
    fn init_skill_write_fails_on_template() {
        // create_dir_all succeeds, first write (gitkeep) succeeds, second write (SKILL.md) fails
        let fs = CountingFs {
            create_dir_fail_after: std::sync::atomic::AtomicU32::new(u32::MAX),
            write_file_fail_after: std::sync::atomic::AtomicU32::new(1),
        };
        let tmp = std::path::PathBuf::from("/tmp/fake-init-skill-write");
        let opts = Options { dir: &tmp, name: Some("test"), plugin_type: Some(PluginType::Skill) };
        let result = init(&opts, &fs);
        assert!(result.is_err());
    }

    #[test]
    fn init_agent_layout_write_fails() {
        let fs = CountingFs {
            create_dir_fail_after: std::sync::atomic::AtomicU32::new(u32::MAX),
            write_file_fail_after: std::sync::atomic::AtomicU32::new(0),
        };
        let tmp = std::path::PathBuf::from("/tmp/fake-init-agent-write");
        let opts = Options { dir: &tmp, name: Some("test"), plugin_type: Some(PluginType::Agent) };
        let result = init(&opts, &fs);
        assert!(result.is_err());
    }

    #[test]
    fn init_mcp_layout_write_fails() {
        let fs = CountingFs {
            create_dir_fail_after: std::sync::atomic::AtomicU32::new(u32::MAX),
            write_file_fail_after: std::sync::atomic::AtomicU32::new(0),
        };
        let tmp = std::path::PathBuf::from("/tmp/fake-init-mcp-write");
        let opts = Options { dir: &tmp, name: Some("test"), plugin_type: Some(PluginType::Mcp) };
        let result = init(&opts, &fs);
        assert!(result.is_err());
    }

    #[test]
    fn init_hook_layout_write_fails() {
        let fs = CountingFs {
            create_dir_fail_after: std::sync::atomic::AtomicU32::new(u32::MAX),
            write_file_fail_after: std::sync::atomic::AtomicU32::new(0),
        };
        let tmp = std::path::PathBuf::from("/tmp/fake-init-hook-write");
        let opts = Options { dir: &tmp, name: Some("test"), plugin_type: Some(PluginType::Hook) };
        let result = init(&opts, &fs);
        assert!(result.is_err());
    }

    #[test]
    fn init_agent_layout_dir_fails() {
        let fs = CountingFs {
            create_dir_fail_after: std::sync::atomic::AtomicU32::new(1),
            write_file_fail_after: std::sync::atomic::AtomicU32::new(u32::MAX),
        };
        let tmp = std::path::PathBuf::from("/tmp/fake-init-agent-dir");
        let opts = Options { dir: &tmp, name: Some("test"), plugin_type: Some(PluginType::Agent) };
        let result = init(&opts, &fs);
        assert!(result.is_err());
    }

    #[test]
    fn init_no_directory_name_from_root_path() {
        let root = std::path::PathBuf::from("/");
        let opts = Options { dir: &root, name: None, plugin_type: None };
        let result = init(&opts, &Real);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("cannot determine package name")));
    }

    #[test]
    fn init_fails_for_empty_name() {
        let fs = FailFs { fail_on: "never" };
        let tmp = std::path::PathBuf::from("/tmp/fake-init-empty-name");
        let opts = Options { dir: &tmp, name: Some(""), plugin_type: None };
        let result = init(&opts, &fs);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("invalid package name")));
    }

    #[test]
    fn init_fails_for_scoped_name_without_slash() {
        let fs = FailFs { fail_on: "never" };
        let tmp = std::path::PathBuf::from("/tmp/fake-init-scoped-noslash");
        let opts = Options { dir: &tmp, name: Some("@noslash"), plugin_type: None };
        let result = init(&opts, &fs);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("invalid package name")));
    }

    #[test]
    fn init_fails_for_scoped_name_with_empty_scope() {
        let fs = FailFs { fail_on: "never" };
        let tmp = std::path::PathBuf::from("/tmp/fake-init-scoped-noscope");
        let opts = Options { dir: &tmp, name: Some("@/pkg"), plugin_type: None };
        let result = init(&opts, &fs);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("invalid package name")));
    }

    #[test]
    fn init_fails_for_scoped_name_with_invalid_segment() {
        let fs = FailFs { fail_on: "never" };
        let tmp = std::path::PathBuf::from("/tmp/fake-init-scoped-invalid-seg");
        let opts = Options { dir: &tmp, name: Some("@INVALID/pkg"), plugin_type: None };
        let result = init(&opts, &fs);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("invalid package name")));
    }
}
