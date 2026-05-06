//! Copilot CLI tool adaptor for `aipm init`.
//!
//! Creates `.github/copilot-instructions.md` so GitHub Copilot CLI can
//! discover the `.ai/` local marketplace via convention. MVP: writes a
//! templated file when none exists; preserves user content when one is
//! already present (no merge logic in this iteration — see spec NG3).

use std::path::Path;

use libaipm_engine_spec::{paths, Engine};

use crate::fs::Fs;
use crate::workspace_init::{Error, ToolAdaptor};

/// Configures GitHub Copilot CLI to discover the `.ai/` local marketplace.
pub struct Adaptor;

impl ToolAdaptor for Adaptor {
    fn name(&self) -> &'static str {
        "Copilot CLI"
    }

    fn engine(&self) -> Engine {
        Engine::Copilot
    }

    fn apply(
        &self,
        dir: &Path,
        no_starter: bool,
        marketplace_name: &str,
        fs: &dyn Fs,
    ) -> Result<bool, Error> {
        let github_dir = dir.join(paths::GITHUB_DOT);
        let instructions_path = github_dir.join("copilot-instructions.md");

        // Preserve user-managed content: if the file already exists, do
        // nothing. A future iteration can implement a marker-block merge
        // similar to the Claude adaptor's settings.json merge.
        if fs.exists(&instructions_path) {
            return Ok(false);
        }

        fs.create_dir_all(&github_dir)?;
        let body = generate_copilot_instructions_template(no_starter, marketplace_name);
        fs.write_file(&instructions_path, body.as_bytes())?;
        Ok(true)
    }
}

/// Build the templated body for `.github/copilot-instructions.md`.
///
/// Always includes a marketplace pointer line; conditionally appends a
/// starter-plugin section gated on `no_starter`. Wrapped in BEGIN/END
/// markers so a future merge-into-existing-file path can locate and
/// rewrite the managed region without touching surrounding user content.
fn generate_copilot_instructions_template(no_starter: bool, marketplace_name: &str) -> String {
    let starter_block = if no_starter {
        String::new()
    } else {
        format!(
            "\n## Default plugin\n\n\
             This project bundles the `starter-aipm-plugin@{marketplace_name}` plugin.\n"
        )
    };
    format!(
        "# Copilot Instructions\n\n\
         This project uses [aipm](https://github.com/TheLarkInn/aipm) to manage AI plugins.\n\
         The local marketplace lives at `.ai/` and is registered as `{marketplace_name}`.\n\
         {starter_block}\n\
         <!-- aipm marketplace pointer; do not edit between markers -->\n\
         <!-- AIPM_MARKETPLACE: {marketplace_name} -->\n",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Real;

    fn make_temp_dir(name: &str) -> std::path::PathBuf {
        let tmp = std::env::temp_dir().join(format!("aipm-test-copilot-{name}"));
        if tmp.exists() {
            let _ = std::fs::remove_dir_all(&tmp);
        }
        let _ = std::fs::create_dir_all(&tmp);
        tmp
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }

    /// Covers the `if tmp.exists()` True branch in `make_temp_dir`: when the
    /// directory already exists before `make_temp_dir` is called, the helper
    /// removes the old tree so tests start with a clean slate.
    #[test]
    fn make_temp_dir_cleans_up_pre_existing_directory() {
        let name = "cleanup-existing";
        let tmp = std::env::temp_dir().join(format!("aipm-test-copilot-{name}"));
        // Pre-create the directory and a sentinel file so we can verify cleanup.
        let _ = std::fs::create_dir_all(&tmp);
        let _ = std::fs::write(tmp.join("sentinel.txt"), b"old content");
        assert!(tmp.exists(), "pre-condition: directory must exist before make_temp_dir");

        // make_temp_dir must remove the old tree (True branch of `if tmp.exists()`)
        // and then recreate a fresh directory.
        let result = make_temp_dir(name);
        assert!(result.exists(), "make_temp_dir must create the directory");
        assert!(
            !result.join("sentinel.txt").exists(),
            "sentinel file must be removed by make_temp_dir cleanup"
        );

        cleanup(&result);
    }

    #[test]
    fn adaptor_reports_copilot_engine_and_name() {
        let adaptor = Adaptor;
        assert_eq!(adaptor.engine(), Engine::Copilot);
        assert_eq!(adaptor.name(), "Copilot CLI");
    }

    #[test]
    fn copilot_instructions_created_when_absent() {
        let tmp = make_temp_dir("fresh");
        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp, false, "local-repo-plugins", &Real);
        assert!(result.is_ok_and(|v| v), "fresh apply must return Ok(true)");

        let content = std::fs::read_to_string(tmp.join(".github/copilot-instructions.md"));
        assert!(content.is_ok());
        let body = content.unwrap_or_default();
        assert!(body.contains("# Copilot Instructions"));
        assert!(body.contains("local-repo-plugins"));
        assert!(body.contains("starter-aipm-plugin@local-repo-plugins"));
        assert!(body.contains("AIPM_MARKETPLACE: local-repo-plugins"));

        cleanup(&tmp);
    }

    #[test]
    fn copilot_instructions_skipped_when_present() {
        let tmp = make_temp_dir("preserve");
        let _ = std::fs::create_dir_all(tmp.join(".github"));
        let user_content = "# My Custom Instructions\n\nNothing to see here.\n";
        let _ = std::fs::write(tmp.join(".github/copilot-instructions.md"), user_content);

        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp, false, "local-repo-plugins", &Real);
        assert!(result.is_ok_and(|v| !v), "existing file must yield Ok(false)");

        // User content untouched
        let content = std::fs::read_to_string(tmp.join(".github/copilot-instructions.md"))
            .unwrap_or_default();
        assert_eq!(content, user_content, "user content must be preserved");

        cleanup(&tmp);
    }

    #[test]
    fn no_starter_omits_starter_block() {
        let tmp = make_temp_dir("no-starter");
        let adaptor = Adaptor;
        let result = adaptor.apply(&tmp, true, "local-repo-plugins", &Real);
        assert!(result.is_ok_and(|v| v));

        let body = std::fs::read_to_string(tmp.join(".github/copilot-instructions.md"))
            .unwrap_or_default();
        assert!(body.contains("# Copilot Instructions"));
        assert!(body.contains("local-repo-plugins"));
        assert!(!body.contains("Default plugin"), "starter section must be omitted");
        assert!(!body.contains("starter-aipm-plugin"), "starter plugin must not be referenced");

        cleanup(&tmp);
    }

    #[test]
    fn template_with_starter_includes_starter_block() {
        let body = generate_copilot_instructions_template(false, "my-marketplace");
        assert!(body.contains("## Default plugin"));
        assert!(body.contains("starter-aipm-plugin@my-marketplace"));
        assert!(body.contains("AIPM_MARKETPLACE: my-marketplace"));
    }

    #[test]
    fn template_no_starter_omits_starter_block() {
        let body = generate_copilot_instructions_template(true, "my-marketplace");
        assert!(!body.contains("## Default plugin"));
        assert!(!body.contains("starter-aipm-plugin"));
        assert!(body.contains("AIPM_MARKETPLACE: my-marketplace"));
    }

    #[test]
    fn template_snapshot_with_starter() {
        let body = generate_copilot_instructions_template(false, "local-repo-plugins");
        insta::assert_snapshot!(body);
    }

    #[test]
    fn template_snapshot_no_starter() {
        let body = generate_copilot_instructions_template(true, "local-repo-plugins");
        insta::assert_snapshot!(body);
    }

    /// Configurable Fs stub for testing error paths in `apply()` without
    /// touching the filesystem.
    struct TestFs {
        exists_result: bool,
        create_dir: fn() -> std::io::Result<()>,
        write: fn() -> std::io::Result<()>,
    }

    impl crate::fs::Fs for TestFs {
        fn exists(&self, _: &Path) -> bool {
            self.exists_result
        }

        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            (self.create_dir)()
        }

        fn write_file(&self, _: &Path, _: &[u8]) -> std::io::Result<()> {
            (self.write)()
        }

        fn read_to_string(&self, _: &Path) -> std::io::Result<String> {
            Ok(String::new())
        }

        fn read_dir(&self, _: &Path) -> std::io::Result<Vec<crate::fs::DirEntry>> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn create_dir_failure_propagates_as_io_variant() {
        let fs = TestFs {
            exists_result: false,
            create_dir: || {
                Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "mkdir denied"))
            },
            write: || Ok(()),
        };
        let adaptor = Adaptor;
        let result = adaptor.apply(Path::new("/tmp/aipm-test-fake"), false, "test", &fs);
        assert!(
            result.is_err_and(|e| matches!(e, Error::Io(_))),
            "create_dir_all failure should produce Error::Io"
        );
    }

    #[test]
    fn write_failure_propagates_as_io_variant() {
        let fs = TestFs {
            exists_result: false,
            create_dir: || Ok(()),
            write: || {
                Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "write denied"))
            },
        };
        let adaptor = Adaptor;
        let result = adaptor.apply(Path::new("/tmp/aipm-test-fake"), false, "test", &fs);
        assert!(
            result.is_err_and(|e| matches!(e, Error::Io(_))),
            "write_file failure should produce Error::Io"
        );
    }
}
