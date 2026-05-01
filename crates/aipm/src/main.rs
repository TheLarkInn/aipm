//! `aipm` — consumer CLI for AI plugin management.
//!
//! Commands: init, install, update, uninstall, link, unlink, list, lint, migrate, lsp.

mod error;
mod lsp;
mod wizard;
mod wizard_tty;

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use clap_verbosity_flag::{Verbosity, WarnLevel};
use libaipm::lint::reporter::Reporter;

#[derive(Parser)]
#[command(name = "aipm", version = libaipm::version(), about = "AI Plugin Manager — consumer CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Increase verbosity (-v info, -vv debug, -vvv trace).
    #[command(flatten)]
    verbose: Verbosity<WarnLevel>,

    /// Log output format for tracing diagnostics on stderr; top-level fatal errors are always plain text.
    #[arg(long, default_value = "text", value_parser = ["text", "json"])]
    log_format: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a workspace for AI plugin management.
    Init {
        /// Skip interactive prompts, use all defaults.
        #[arg(short = 'y', long)]
        yes: bool,

        /// Generate a workspace manifest (aipm.toml with [workspace] section).
        #[arg(long)]
        workspace: bool,

        /// Generate a .ai/ local marketplace with tool settings.
        #[arg(long)]
        marketplace: bool,

        /// Skip the starter plugin (create bare .ai/ directory only).
        #[arg(long)]
        no_starter: bool,

        /// Generate aipm.toml plugin manifests (opt-in; dependency management not yet available).
        #[arg(long)]
        manifest: bool,

        /// Custom marketplace name (default: "local-repo-plugins").
        #[arg(long)]
        name: Option<String>,

        /// Directory to initialize (defaults to current directory).
        #[arg(default_value = ".")]
        dir: PathBuf,
    },

    /// Install packages from the registry or other sources.
    Install {
        /// Package to install (e.g., "code-review@^1.0", "github:org/repo:path@ref").
        package: Option<String>,

        /// CI mode: fail if lockfile doesn't match manifest.
        #[arg(long)]
        locked: bool,

        /// Use a specific registry.
        #[arg(long)]
        registry: Option<String>,

        /// Install globally (available to all projects).
        #[arg(long)]
        global: bool,

        /// Restrict global install to a specific engine (e.g., "claude", "copilot").
        #[arg(long)]
        engine: Option<String>,

        /// Download cache policy override.
        #[arg(long, value_parser = ["auto", "cache-only", "skip", "force-refresh", "no-refresh"])]
        plugin_cache: Option<String>,

        /// Project directory.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },

    /// Update packages to their latest compatible versions.
    Update {
        /// Package to update (omit to update all).
        package: Option<String>,

        /// Project directory.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },

    /// Link a local package directory for development.
    Link {
        /// Path to local package directory.
        path: PathBuf,

        /// Project directory.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },

    /// Uninstall or unlink a package.
    Uninstall {
        /// Package spec or name to uninstall.
        package: String,

        /// Uninstall from global registry.
        #[arg(long)]
        global: bool,

        /// Remove from a specific engine only (global installs).
        #[arg(long)]
        engine: Option<String>,

        /// Project directory (ignored when --global is set).
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },

    /// Unlink a previously linked package, restoring the registry version.
    Unlink {
        /// Package name to unlink.
        package: String,

        /// Project directory.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },

    /// List installed packages or active link overrides.
    List {
        /// Show only active dev link overrides.
        #[arg(long)]
        linked: bool,

        /// Show globally installed plugins.
        #[arg(long)]
        global: bool,

        /// Project directory.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },

    /// Lint AI plugin configurations for quality issues.
    Lint {
        /// Project directory.
        #[arg(default_value = ".")]
        dir: PathBuf,

        /// Filter to a specific source type (.claude, .github, .ai).
        #[arg(long)]
        source: Option<String>,

        /// Output reporter: human, json, ci-github, ci-azure.
        #[arg(long, default_value = "human", value_parser = ["human", "json", "ci-github", "ci-azure"])]
        reporter: String,

        /// Color mode for the human reporter.
        #[arg(long, default_value = "auto", value_parser = ["never", "auto", "always"])]
        color: String,

        /// Deprecated alias for --reporter (hidden).
        #[arg(long, hide = true, value_parser = ["human", "json", "ci-github", "ci-azure", "text"])]
        format: Option<String>,

        /// Maximum directory traversal depth.
        #[arg(long)]
        max_depth: Option<usize>,
    },

    /// Migrate AI tool configurations into marketplace plugins.
    Migrate {
        /// Preview migration without writing files (generates report).
        #[arg(long)]
        dry_run: bool,

        /// Remove migrated source files after successful migration.
        /// When omitted, an interactive prompt asks whether to clean up (TTY only).
        #[arg(long)]
        destructive: bool,

        /// Source folder to scan (e.g., ".claude").
        /// When omitted, recursively discovers all .claude/ directories.
        #[arg(long)]
        source: Option<String>,

        /// Maximum directory depth for recursive discovery.
        /// Ignored when --source is provided.
        #[arg(long)]
        max_depth: Option<usize>,

        /// Generate aipm.toml plugin manifests (opt-in; dependency management not yet available).
        #[arg(long)]
        manifest: bool,

        /// Project directory.
        #[arg(default_value = ".")]
        dir: PathBuf,
    },

    /// Scaffold new plugins in a marketplace directory.
    Make {
        #[command(subcommand)]
        subcommand: MakeSubcommand,
    },

    /// Author commands for plugin packages.
    Pack {
        #[command(subcommand)]
        subcommand: PackSubcommand,
    },

    /// Start the Language Server Protocol server (for VS Code / IDE integration).
    Lsp,
}

#[derive(Subcommand)]
enum PackSubcommand {
    /// Initialize a new AI plugin package.
    Init {
        /// Skip interactive prompts, use all defaults.
        #[arg(short = 'y', long)]
        yes: bool,

        /// Package name (defaults to directory name).
        #[arg(long)]
        name: Option<String>,

        /// Plugin type: skill, agent, mcp, hook, lsp, composite.
        #[arg(long, rename_all = "kebab-case", value_name = "TYPE")]
        r#type: Option<String>,

        /// Directory to initialize (defaults to current directory).
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
}

#[derive(Subcommand)]
enum MakeSubcommand {
    /// Create a new plugin in the marketplace directory.
    Plugin {
        /// Plugin name (required or prompted).
        #[arg(long)]
        name: Option<String>,

        /// Target engine: claude, copilot, both (default: claude).
        #[arg(long)]
        engine: Option<String>,

        /// AI feature types to include (repeatable).
        #[arg(long = "feature")]
        features: Vec<String>,

        /// Skip interactive prompts, use defaults.
        #[arg(short = 'y', long)]
        yes: bool,

        /// Project directory.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },
}

// =========================================================================
// Stub registry — placeholder until GitRegistry (git2/reqwest) is implemented
// =========================================================================

struct StubRegistry;

impl libaipm::registry::Registry for StubRegistry {
    fn get_metadata(
        &self,
        name: &str,
    ) -> Result<libaipm::registry::PackageMetadata, libaipm::registry::error::Error> {
        Err(libaipm::registry::error::Error::Io {
            reason: format!("no registry configured — cannot look up '{name}'"),
        })
    }

    fn download(
        &self,
        name: &str,
        version: &libaipm::version::Version,
    ) -> Result<Vec<u8>, libaipm::registry::error::Error> {
        Err(libaipm::registry::error::Error::Io {
            reason: format!("no registry configured — cannot download '{name}@{version}'"),
        })
    }
}

// =========================================================================
// Helpers
// =========================================================================

/// Resolve a directory argument: if ".", use `current_dir()`.
fn resolve_dir(dir: PathBuf) -> Result<PathBuf, error::CliError> {
    if dir.as_os_str() == "." {
        Ok(std::env::current_dir()?)
    } else {
        Ok(dir)
    }
}

/// Read `[workspace].plugins_dir` from the manifest at `dir/aipm.toml`, falling
/// back to `.ai` when unset or when the manifest cannot be loaded.
fn resolve_plugins_dir(dir: &Path) -> PathBuf {
    let manifest_path = dir.join("aipm.toml");
    match libaipm::manifest::load(&libaipm::fs::Real, &manifest_path) {
        Ok(manifest) => {
            if let Some(ws) = manifest.workspace {
                if let Some(pd) = ws.plugins_dir {
                    return dir.join(pd);
                }
            }
            tracing::debug!(path = %manifest_path.display(), "manifest has no plugins_dir, using .ai");
        },
        Err(e) => {
            tracing::debug!(path = %manifest_path.display(), error = %e, "could not load manifest, using .ai");
        },
    }
    dir.join(".ai")
}

/// Get the global content-addressable store path (`~/.aipm/store/`).
fn home_store_path() -> Result<PathBuf, error::CliError> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "could not determine home directory")?;
    Ok(PathBuf::from(home).join(".aipm/store"))
}

/// Produce an approximate ISO-8601 timestamp using `SystemTime` (no extra deps).
///
/// Format: `YYYY-MM-DDTHH:MM:SSZ` (UTC, second precision).
fn timestamp_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    // Convert Unix seconds to a basic UTC datetime string
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    // Approximate calendar date (good enough for display)
    let year = 1970 + days / 365;
    let day_of_year = days % 365;
    let (month, day) = day_of_year_to_month_day(day_of_year);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn day_of_year_to_month_day(day: u64) -> (u64, u64) {
    let months = [31u64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut remaining = day;
    for (i, &days_in_month) in months.iter().enumerate() {
        if remaining < days_in_month {
            return (i as u64 + 1, remaining + 1);
        }
        remaining -= days_in_month;
    }
    (12, remaining + 1)
}

// =========================================================================
// Command handlers
// =========================================================================

/// Grouped wizard flags for the init command.
struct InitWizardFlags {
    yes: bool,
    workspace: bool,
    marketplace: bool,
    no_starter: bool,
}

fn cmd_init(
    flags: &InitWizardFlags,
    manifest: bool,
    name: Option<&str>,
    dir: PathBuf,
) -> Result<(), error::CliError> {
    let dir = resolve_dir(dir)?;
    let interactive = !flags.yes && std::io::stdin().is_terminal();

    let (do_workspace, do_marketplace, do_no_starter, marketplace_name) = wizard_tty::resolve(
        interactive,
        (flags.workspace, flags.marketplace, flags.no_starter),
        name,
    )?;

    let adaptors = libaipm::workspace_init::adaptors::defaults();
    let opts = libaipm::workspace_init::Options {
        dir: &dir,
        workspace: do_workspace,
        marketplace: do_marketplace,
        no_starter: do_no_starter,
        manifest,
        marketplace_name: &marketplace_name,
    };

    let result = libaipm::workspace_init::init(&opts, &adaptors, &libaipm::fs::Real)?;

    let mut stdout = std::io::stdout();
    for action in &result.actions {
        let msg = match action {
            libaipm::workspace_init::InitAction::WorkspaceCreated => {
                format!("Initialized workspace in {}", dir.display())
            },
            libaipm::workspace_init::InitAction::MarketplaceCreated => {
                if do_no_starter {
                    format!("Created .ai/ marketplace '{marketplace_name}' (no starter plugin)")
                } else {
                    format!("Created .ai/ marketplace '{marketplace_name}' with starter plugin")
                }
            },
            libaipm::workspace_init::InitAction::ToolConfigured(name) => {
                format!("Configured {name} settings")
            },
        };
        let _ = writeln!(stdout, "{msg}");
    }
    Ok(())
}

fn cmd_install(package: Option<String>, locked: bool, dir: PathBuf) -> Result<(), error::CliError> {
    let dir = resolve_dir(dir)?;

    // Discover workspace root if we're inside one
    let workspace_root = libaipm::workspace::find_workspace_root(&libaipm::fs::Real, &dir);
    if let Some(ref ws_root) = workspace_root {
        if ws_root != &dir {
            tracing::info!(workspace_root = %ws_root.display(), "found workspace root");
        }
    }

    let plugins_dir = resolve_plugins_dir(&dir);
    let config = libaipm::installer::pipeline::InstallConfig {
        manifest_path: dir.join("aipm.toml"),
        lockfile_path: dir.join("aipm.lock"),
        store_path: home_store_path()?,
        links_dir: dir.join(".aipm/links"),
        gitignore_path: plugins_dir.join(".gitignore"),
        link_state_path: dir.join(".aipm/links.toml"),
        plugins_dir,
        workspace_root,
        locked,
        add_package: package,
        generated_by: format!("aipm {}", libaipm::version()),
    };

    let registry = StubRegistry;
    let result = libaipm::installer::pipeline::install(&libaipm::fs::Real, &config, &registry)?;

    let mut stdout = std::io::stdout();
    let _ = writeln!(
        stdout,
        "Installed {} package(s), {} up-to-date, {} removed",
        result.installed, result.up_to_date, result.removed
    );
    Ok(())
}

fn cmd_update(package: Option<String>, dir: PathBuf) -> Result<(), error::CliError> {
    let dir = resolve_dir(dir)?;

    let plugins_dir = resolve_plugins_dir(&dir);
    let config = libaipm::installer::pipeline::UpdateConfig {
        manifest_path: dir.join("aipm.toml"),
        lockfile_path: dir.join("aipm.lock"),
        store_path: home_store_path()?,
        links_dir: dir.join(".aipm/links"),
        gitignore_path: plugins_dir.join(".gitignore"),
        link_state_path: dir.join(".aipm/links.toml"),
        plugins_dir,
        package,
        generated_by: format!("aipm {}", libaipm::version()),
    };

    let registry = StubRegistry;
    let result = libaipm::installer::pipeline::update(&libaipm::fs::Real, &config, &registry)?;

    let mut stdout = std::io::stdout();
    let _ = writeln!(
        stdout,
        "Updated {} package(s), {} up-to-date, {} removed",
        result.installed, result.up_to_date, result.removed
    );
    Ok(())
}

fn cmd_link(path: PathBuf, dir: PathBuf) -> Result<(), error::CliError> {
    let dir = resolve_dir(dir)?;
    let path = if path.is_relative() { dir.join(&path) } else { path };

    // Validate the target path has an aipm.toml
    let target_manifest = path.join("aipm.toml");
    if !target_manifest.exists() {
        return Err(format!(
            "no aipm.toml found at '{}' — not a valid package directory",
            path.display()
        )
        .into());
    }

    // Read package name from the manifest
    let manifest = libaipm::manifest::load(&libaipm::fs::Real, &target_manifest)?;
    let pkg_name = manifest
        .package
        .as_ref()
        .map(|p| p.name.clone())
        .ok_or("manifest at linked path has no [package] section")?;

    let plugins_dir = resolve_plugins_dir(&dir);
    let link_target = plugins_dir.join(&pkg_name);

    // Create the directory link
    std::fs::create_dir_all(&plugins_dir)?;
    libaipm::linker::directory_link::create(&path, &link_target)?;

    // Update link state
    let link_state_path = dir.join(".aipm/links.toml");
    let entry = libaipm::linker::link_state::LinkEntry {
        name: pkg_name.clone(),
        path: path.clone(),
        linked_at: timestamp_now(),
    };
    libaipm::linker::link_state::add(&libaipm::fs::Real, &link_state_path, entry)?;

    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "Linked '{pkg_name}' → {}", path.display());
    Ok(())
}

fn cmd_unlink(package: &str, dir: PathBuf) -> Result<(), error::CliError> {
    let dir = resolve_dir(dir)?;
    let plugins_dir = resolve_plugins_dir(&dir);
    let links_dir = dir.join(".aipm/links");

    libaipm::linker::pipeline::unlink_package(package, &links_dir, &plugins_dir)?;

    let link_state_path = dir.join(".aipm/links.toml");
    libaipm::linker::link_state::remove(&libaipm::fs::Real, &link_state_path, package)?;

    let gitignore_path = plugins_dir.join(".gitignore");
    if gitignore_path.exists() {
        libaipm::linker::gitignore::remove_entry(&libaipm::fs::Real, &gitignore_path, package)?;
    }

    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "Unlinked '{package}'");
    Ok(())
}

fn cmd_list(linked: bool, dir: PathBuf) -> Result<(), error::CliError> {
    let dir = resolve_dir(dir)?;
    let mut stdout = std::io::stdout();

    if linked {
        let link_state_path = dir.join(".aipm/links.toml");
        let entries = libaipm::linker::link_state::list(&libaipm::fs::Real, &link_state_path)?;

        if entries.is_empty() {
            let _ = writeln!(stdout, "No active dev link overrides.");
        } else {
            let _ = writeln!(stdout, "Active dev link overrides:");
            for entry in &entries {
                let _ = writeln!(
                    stdout,
                    "  {} → {} (linked at {})",
                    entry.name,
                    entry.path.display(),
                    entry.linked_at
                );
            }
        }
    } else {
        let lockfile_path = dir.join("aipm.lock");
        if lockfile_path.exists() {
            let lf = libaipm::lockfile::read(&libaipm::fs::Real, &lockfile_path)?;

            if lf.packages.is_empty() {
                let _ = writeln!(stdout, "No packages installed.");
            } else {
                let _ = writeln!(stdout, "Installed packages:");
                for pkg in &lf.packages {
                    let _ = writeln!(stdout, "  {}@{}", pkg.name, pkg.version);
                }
            }
        } else {
            let _ = writeln!(stdout, "No lockfile found. Run 'aipm install' first.");
        }
    }
    Ok(())
}

fn cmd_install_global(
    package: Option<String>,
    engine: Option<String>,
    plugin_cache: Option<String>,
    _dir: PathBuf,
) -> Result<(), error::CliError> {
    let spec = package.ok_or("--global install requires a package spec")?;
    let engines: Vec<String> = engine.into_iter().collect();
    let cache_policy: Option<libaipm::cache::Policy> =
        plugin_cache.map(|s| s.parse()).transpose()?;

    // Load or create the global installed registry
    let registry_path = home_aipm_path()?.join("installed.json");
    let mut registry = load_installed_registry(&registry_path)?;

    let added = registry.install(spec.clone(), &engines, cache_policy, None)?;

    // Save under lock
    let json = serde_json::to_string_pretty(&registry)?;
    let mut locked = libaipm::locked_file::LockedFile::open(&registry_path)?;
    locked.write_content(&json)?;

    let mut stdout = std::io::stdout();
    if added {
        let _ = writeln!(stdout, "Installed '{spec}' globally");
    } else {
        let _ = writeln!(stdout, "Updated '{spec}' in global registry");
    }
    Ok(())
}

fn cmd_uninstall_global(
    package: &str,
    engine: Option<&str>,
    _dir: PathBuf,
) -> Result<(), error::CliError> {
    let registry_path = home_aipm_path()?.join("installed.json");
    let mut registry = load_installed_registry(&registry_path)?;

    let engine_filter: Vec<String> = engine.iter().map(ToString::to_string).collect();
    let spec = registry.resolve_spec(package, &engine_filter)?;

    let changed = if let Some(eng) = engine {
        registry.uninstall_engine(&spec, &[eng.to_string()])
    } else {
        registry.uninstall(&spec)
    };

    if !changed {
        return Err(format!("Plugin '{package}' not found in global registry").into());
    }

    let json = serde_json::to_string_pretty(&registry)?;
    let mut locked = libaipm::locked_file::LockedFile::open(&registry_path)?;
    locked.write_content(&json)?;

    let mut stdout = std::io::stdout();
    if let Some(eng) = engine {
        let _ = writeln!(stdout, "Removed '{spec}' from {eng} engine globally");
    } else {
        let _ = writeln!(stdout, "Uninstalled '{spec}' globally");
    }
    Ok(())
}

fn cmd_list_global() -> Result<(), error::CliError> {
    let registry_path = home_aipm_path()?.join("installed.json");
    let registry = load_installed_registry(&registry_path)?;

    let mut stdout = std::io::stdout();
    if registry.plugins.is_empty() {
        let _ = writeln!(stdout, "No globally installed plugins.");
    } else {
        let _ = writeln!(stdout, "Globally installed plugins:");
        for plugin in &registry.plugins {
            let engine_info = if plugin.engines.is_empty() {
                "all engines".to_string()
            } else {
                plugin.engines.join(", ")
            };
            let _ = writeln!(stdout, "  {} ({engine_info})", plugin.spec);
        }
    }
    Ok(())
}

/// Get the `~/.aipm/` directory path.
fn home_aipm_path() -> Result<PathBuf, error::CliError> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "could not determine home directory")?;
    Ok(PathBuf::from(home).join(".aipm"))
}

/// Load the global installed registry from disk (or empty default).
fn load_installed_registry(path: &Path) -> std::io::Result<libaipm::installed::Registry> {
    libaipm::fs::read_or_default(&libaipm::fs::Real, path)
}

fn cmd_lint(
    dir: PathBuf,
    source: Option<String>,
    reporter: &str,
    color: &str,
    format: Option<&str>,
    max_depth: Option<usize>,
) -> Result<(), error::CliError> {
    let dir = resolve_dir(dir)?;

    // Validate --source against supported set
    if let Some(ref src) = source {
        const SUPPORTED_SOURCES: &[&str] = &[".claude", ".github", ".ai"];
        if !SUPPORTED_SOURCES.contains(&src.as_str()) {
            return Err(format!(
                "unsupported source '{src}'; valid sources: .claude, .github, .ai"
            )
            .into());
        }
    }

    // --format is a deprecated alias for --reporter; it takes precedence when provided
    let effective_reporter = format.unwrap_or(reporter);

    // Map legacy format names to new reporter names
    let effective_reporter = match effective_reporter {
        "text" => "human",
        other => other,
    };

    // Reject any unrecognised reporter value (belt-and-suspenders after mapping)
    if !["human", "json", "ci-github", "ci-azure"].contains(&effective_reporter) {
        return Err(format!(
            "unknown reporter '{effective_reporter}'. Valid values: human, json, ci-github, ci-azure"
        )
        .into());
    }

    // Resolve color choice
    let color_choice = match color {
        "never" => libaipm::lint::reporter::ColorChoice::Never,
        "always" => libaipm::lint::reporter::ColorChoice::Always,
        _ => libaipm::lint::reporter::ColorChoice::Auto,
    };

    // Load lint config from aipm.toml [workspace.lints] if it exists
    let config = load_lint_config(&dir);

    let opts = libaipm::lint::Options { dir: dir.clone(), source, config, max_depth };

    let outcome = libaipm::lint::lint(&opts, &libaipm::fs::Real)?;

    let mut stdout = std::io::stdout();
    match effective_reporter {
        "json" => {
            libaipm::lint::reporter::Json.report(&outcome, &mut stdout)?;
        },
        "ci-github" => {
            libaipm::lint::reporter::CiGitHub.report(&outcome, &mut stdout)?;
        },
        "ci-azure" => {
            libaipm::lint::reporter::CiAzure.report(&outcome, &mut stdout)?;
        },
        _ => {
            let human = libaipm::lint::reporter::Human {
                fs: &libaipm::fs::Real,
                color: color_choice,
                base_dir: &dir,
            };
            human.report(&outcome, &mut stdout)?;
        },
    }

    if outcome.error_count > 0 {
        // Use a specific error type so main() can distinguish lint failures
        // from unexpected errors, avoiding noisy "error:" prefix for JSON consumers.
        return Err(format!("lint found {} error(s)", outcome.error_count).into());
    }

    Ok(())
}

pub(crate) fn load_lint_config(dir: &Path) -> libaipm::lint::config::Config {
    let manifest_path = dir.join("aipm.toml");
    let content = match std::fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(path = %manifest_path.display(), "no aipm.toml found, using default lint config");
            return libaipm::lint::config::Config::default();
        },
        Err(e) => {
            tracing::warn!(
                path = %manifest_path.display(),
                error = %e,
                "failed to read aipm.toml, using default lint config"
            );
            return libaipm::lint::config::Config::default();
        },
    };
    let manifest = match toml::from_str::<toml::Value>(&content) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(
                path = %manifest_path.display(),
                error = %e,
                "failed to parse aipm.toml, using default lint config"
            );
            return libaipm::lint::config::Config::default();
        },
    };

    let mut config = libaipm::lint::config::Config::default();

    let Some(workspace) = manifest.get("workspace") else {
        return config;
    };
    let Some(lints) = workspace.get("lints") else {
        return config;
    };
    let Some(lints_table) = lints.as_table() else {
        return config;
    };

    // Parse global ignore paths
    if let Some(ignore) = lints_table.get("ignore") {
        if let Some(paths) = ignore.get("paths").and_then(toml::Value::as_array) {
            for p in paths {
                if let Some(s) = p.as_str() {
                    config.ignore_paths.push(s.to_string());
                }
            }
        }
    }

    // Parse per-rule overrides
    for (key, value) in lints_table {
        if key == "ignore" {
            continue;
        }
        if let Some(s) = value.as_str() {
            if s == "allow" {
                config
                    .rule_overrides
                    .insert(key.clone(), libaipm::lint::config::RuleOverride::Allow);
            } else if let Some(severity) = libaipm::lint::Severity::from_str_config(s) {
                config
                    .rule_overrides
                    .insert(key.clone(), libaipm::lint::config::RuleOverride::Level(severity));
            }
        } else if let Some(table) = value.as_table() {
            let level = table
                .get("level")
                .and_then(toml::Value::as_str)
                .and_then(libaipm::lint::Severity::from_str_config);
            let ignore: Vec<String> = table
                .get("ignore")
                .and_then(toml::Value::as_array)
                .map(|arr| arr.iter().filter_map(toml::Value::as_str).map(String::from).collect())
                .unwrap_or_default();

            let options: std::collections::BTreeMap<String, toml::Value> = table
                .iter()
                .filter(|(k, _)| *k != "level" && *k != "ignore")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            // Record Detailed when there is anything meaningful: a level override,
            // ignore paths, or custom option keys.  This allows users to configure
            // per-rule options (e.g. `lines`, `characters`) without being forced to
            // also specify a `level`.
            if level.is_some() || !ignore.is_empty() || !options.is_empty() {
                config.rule_overrides.insert(
                    key.clone(),
                    libaipm::lint::config::RuleOverride::Detailed { level, ignore, options },
                );
            }
        }
    }

    config
}

fn cmd_migrate(
    dry_run: bool,
    destructive: bool,
    source: Option<&str>,
    max_depth: Option<usize>,
    manifest: bool,
    dir: PathBuf,
) -> Result<(), error::CliError> {
    let dir = resolve_dir(dir)?;
    let opts =
        libaipm::migrate::Options { dir: &dir, source, dry_run, destructive, max_depth, manifest };

    let result = libaipm::migrate::migrate(&opts, &libaipm::fs::Real)?;

    let mut stdout = std::io::stdout();
    for action in &result.actions {
        match action {
            libaipm::migrate::Action::PluginCreated { name, source, plugin_type, .. } => {
                let _ =
                    writeln!(stdout, "Migrated {plugin_type} '{name}' from {}", source.display());
            },
            libaipm::migrate::Action::MarketplaceRegistered { name } => {
                let _ = writeln!(stdout, "Registered '{name}' in marketplace.json");
            },
            libaipm::migrate::Action::Renamed { original_name, new_name, reason } => {
                let _ = writeln!(
                    stdout,
                    "Warning: renamed '{original_name}' → '{new_name}' ({reason})"
                );
            },
            libaipm::migrate::Action::Skipped { name, reason } => {
                let _ = writeln!(stdout, "Skipped '{name}': {reason}");
            },
            libaipm::migrate::Action::DryRunReport { path } => {
                let _ = writeln!(stdout, "Dry run report written to {}", path.display());
            },
            libaipm::migrate::Action::OtherFileMigrated {
                path,
                destination,
                associated_artifact,
            } => {
                let note = associated_artifact
                    .as_ref()
                    .map_or_else(|| "(unassociated)".to_string(), |a| format!("(dep of {a})"));
                let _ = writeln!(
                    stdout,
                    "  Copied other file {} → {} {note}",
                    path.display(),
                    destination.display()
                );
            },
            libaipm::migrate::Action::ExternalReferenceDetected { path, referenced_by } => {
                let mut stderr = std::io::stderr();
                let _ = writeln!(
                    stderr,
                    "Warning: external file {} referenced by '{referenced_by}' — not moved",
                    path.display()
                );
            },
            libaipm::migrate::Action::SourceFileRemoved { .. }
            | libaipm::migrate::Action::SourceDirRemoved { .. }
            | libaipm::migrate::Action::EmptyDirPruned { .. } => {
                // Cleanup actions are printed separately below
            },
        }
    }

    // Post-migration cleanup phase
    if dry_run || !result.has_migrated_artifacts() {
        return Ok(());
    }

    let should_clean = if destructive {
        true
    } else {
        let interactive = std::io::stdin().is_terminal();
        if interactive {
            wizard_tty::resolve_migrate_cleanup(interactive, &result)?
        } else {
            false
        }
    };

    if should_clean {
        let cleanup_actions =
            libaipm::migrate::cleanup::remove_migrated_sources(&result, &libaipm::fs::Real)?;
        for action in &cleanup_actions {
            match action {
                libaipm::migrate::Action::SourceFileRemoved { path } => {
                    let _ = writeln!(stdout, "Removed source file: {}", path.display());
                },
                libaipm::migrate::Action::SourceDirRemoved { path } => {
                    let _ = writeln!(stdout, "Removed source directory: {}", path.display());
                },
                libaipm::migrate::Action::EmptyDirPruned { path } => {
                    let _ = writeln!(stdout, "Pruned empty directory: {}", path.display());
                },
                _ => {},
            }
        }
    }

    Ok(())
}

fn cmd_make_plugin(
    name: Option<&str>,
    engine: Option<&str>,
    features: &[String],
    yes: bool,
    dir: PathBuf,
) -> Result<(), error::CliError> {
    let dir = resolve_dir(dir)?;
    let interactive = !yes && std::io::stdin().is_terminal();

    let (resolved_name, resolved_engine, resolved_features) =
        wizard_tty::resolve_make_plugin(interactive, name, engine, features)?;

    // Validate name
    libaipm::manifest::validate::check_name(
        &resolved_name,
        libaipm::manifest::validate::ValidationMode::Strict,
    )
    .map_err(libaipm::make::Error::InvalidName)?;

    // Validate engine
    match resolved_engine.as_str() {
        "claude" | "copilot" | "both" => {},
        _ => return Err(libaipm::make::Error::InvalidEngine(resolved_engine).into()),
    }

    // Parse and validate features
    let parsed_features: Vec<libaipm::make::Feature> = resolved_features
        .iter()
        .map(|f| {
            libaipm::make::Feature::from_cli_name(f)
                .ok_or_else(|| libaipm::make::Error::InvalidFeature(f.clone()))
        })
        .collect::<Result<_, _>>()?;

    if let Err(unsupported) =
        libaipm::make::engine_features::validate_features(&resolved_engine, &parsed_features)
    {
        if let Some(first) = unsupported.first() {
            return Err(libaipm::make::Error::UnsupportedFeature {
                feature: first.cli_name().to_string(),
                engine: resolved_engine,
            }
            .into());
        }
    }

    // Discover marketplace directory
    let plugins_dir = libaipm::make::discovery::find_marketplace(&dir, &libaipm::fs::Real)?;

    let opts = libaipm::make::PluginOpts {
        marketplace_dir: &plugins_dir,
        name: &resolved_name,
        engine: &resolved_engine,
        features: &parsed_features,
    };

    let result = libaipm::make::plugin(&opts, &libaipm::fs::Real)?;

    let mut stdout = std::io::stdout();
    for action in &result.actions {
        match action {
            libaipm::make::Action::DirectoryCreated { path } => {
                let _ = writeln!(stdout, "Created {}", path.display());
            },
            libaipm::make::Action::DirectoryAlreadyExists { path }
            | libaipm::make::Action::FileAlreadyExists { path } => {
                let _ = writeln!(stdout, "Already exists: {}", path.display());
            },
            libaipm::make::Action::FileWritten { path, description } => {
                let _ = writeln!(stdout, "Wrote {description}: {}", path.display());
            },
            libaipm::make::Action::PluginRegistered { name, .. } => {
                let _ = writeln!(stdout, "Registered '{name}' in marketplace");
            },
            libaipm::make::Action::PluginAlreadyRegistered { name } => {
                let _ = writeln!(stdout, "Plugin '{name}' already registered");
            },
            libaipm::make::Action::PluginEnabled { plugin_key, .. } => {
                let _ = writeln!(stdout, "Enabled {plugin_key} in settings");
            },
            libaipm::make::Action::PluginAlreadyEnabled { plugin_key } => {
                let _ = writeln!(stdout, "Plugin {plugin_key} already enabled");
            },
            libaipm::make::Action::PluginCreated { name, features, engine, .. } => {
                let _ = writeln!(
                    stdout,
                    "Created plugin '{name}' (engine: {engine}, features: {})",
                    features.join(", ")
                );
            },
        }
    }

    Ok(())
}

fn cmd_pack_init(
    yes: bool,
    name: Option<&str>,
    r#type: Option<&str>,
    dir: PathBuf,
) -> Result<(), error::CliError> {
    let plugin_type = r#type.map(str::parse::<libaipm::manifest::types::PluginType>).transpose()?;

    let dir = resolve_dir(dir)?;
    let interactive = !yes && std::io::stdin().is_terminal();

    let (final_name, final_type) =
        wizard_tty::resolve_pack_init(interactive, &dir, name.map(String::from), plugin_type)?;

    let opts =
        libaipm::init::Options { dir: &dir, name: final_name.as_deref(), plugin_type: final_type };

    libaipm::init::init(&opts, &libaipm::fs::Real)?;

    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "Initialized plugin package in {}", dir.display());
    Ok(())
}

// =========================================================================
// Entry point
// =========================================================================

fn run() -> Result<(), error::CliError> {
    let cli = Cli::parse();

    // Initialize logging from CLI flags
    let verbosity = cli.verbose.tracing_level_filter();
    let log_fmt = match cli.log_format.as_str() {
        "json" => libaipm::logging::LogFormat::Json,
        _ => libaipm::logging::LogFormat::Text,
    };
    libaipm::logging::init(verbosity, log_fmt)?;

    match cli.command {
        Some(Commands::Init { yes, workspace, marketplace, no_starter, manifest, name, dir }) => {
            let flags = InitWizardFlags { yes, workspace, marketplace, no_starter };
            cmd_init(&flags, manifest, name.as_deref(), dir)
        },
        Some(Commands::Install {
            package,
            locked,
            registry,
            global,
            engine,
            plugin_cache,
            dir,
        }) => {
            if registry.is_some() {
                let mut stderr = std::io::stderr();
                let _ = writeln!(
                    stderr,
                    "warning: --registry is not yet supported and will be ignored"
                );
            }
            if global {
                cmd_install_global(package, engine, plugin_cache, dir)
            } else {
                let _ = (engine, plugin_cache); // Consumed by global path only for now
                cmd_install(package, locked, dir)
            }
        },
        Some(Commands::Update { package, dir }) => cmd_update(package, dir),
        Some(Commands::Link { path, dir }) => cmd_link(path, dir),
        Some(Commands::Uninstall { package, global, engine, dir }) => {
            if global {
                cmd_uninstall_global(&package, engine.as_deref(), dir)
            } else {
                cmd_unlink(&package, dir)
            }
        },
        Some(Commands::Unlink { package, dir }) => cmd_unlink(&package, dir),
        Some(Commands::List { linked, global, dir }) => {
            if global {
                cmd_list_global()
            } else {
                cmd_list(linked, dir)
            }
        },
        Some(Commands::Lint { dir, source, reporter, color, format, max_depth }) => {
            cmd_lint(dir, source, &reporter, &color, format.as_deref(), max_depth)
        },
        Some(Commands::Migrate { dry_run, destructive, source, max_depth, manifest, dir }) => {
            cmd_migrate(dry_run, destructive, source.as_deref(), max_depth, manifest, dir)
        },
        Some(Commands::Make { subcommand }) => match subcommand {
            MakeSubcommand::Plugin { name, engine, features, yes, dir } => {
                cmd_make_plugin(name.as_deref(), engine.as_deref(), &features, yes, dir)
            },
        },
        Some(Commands::Pack { subcommand }) => match subcommand {
            PackSubcommand::Init { yes, name, r#type, dir } => {
                cmd_pack_init(yes, name.as_deref(), r#type.as_deref(), dir)
            },
        },
        Some(Commands::Lsp) => Ok(lsp::run()?),
        None => {
            let mut stdout = std::io::stdout();
            let _ = writeln!(stdout, "aipm {}", libaipm::version());
            let _ = writeln!(stdout, "Use --help for usage information.");
            Ok(())
        },
    }
}

fn main() -> std::process::ExitCode {
    if let Err(e) = run() {
        let mut stderr = std::io::stderr();
        let _ = writeln!(stderr, "error: {e}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shared mutex for any test that mutates `HOME` (or other process-global
    /// env vars). Previously each test declared its own function-local
    /// `static ENV_LOCK`, which meant two tests setting `HOME` ran concurrently
    /// and clobbered each other — the flake only surfaced under the slower
    /// nightly `llvm-cov` instrumented binary where scheduling was different.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// `load_lint_config` returns a default config when the aipm.toml exists
    /// but contains no `[workspace]` table.
    #[test]
    fn load_lint_config_no_workspace_section_returns_default() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("aipm.toml"),
            "[package]\nname = \"my-plugin\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let config = load_lint_config(tmp.path());
        assert!(config.rule_overrides.is_empty());
        assert!(config.ignore_paths.is_empty());
    }

    /// `load_lint_config` returns a default config when the `[workspace]` table
    /// exists but contains no `lints` key.
    #[test]
    fn load_lint_config_workspace_without_lints_returns_default() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("aipm.toml"), "[workspace]\nmembers = [\".ai/*\"]\n")
            .unwrap();

        let config = load_lint_config(tmp.path());
        assert!(config.rule_overrides.is_empty());
        assert!(config.ignore_paths.is_empty());
    }

    /// `load_lint_config` returns a default config when `workspace.lints` is
    /// present but is not a TOML table (e.g., a bare string).
    #[test]
    fn load_lint_config_lints_not_a_table_returns_default() {
        let tmp = tempfile::tempdir().unwrap();
        // `lints = "something"` makes it a string value, not a table.
        std::fs::write(
            tmp.path().join("aipm.toml"),
            "[workspace]\nmembers = [\".ai/*\"]\nlints = \"not-a-table\"\n",
        )
        .unwrap();

        let config = load_lint_config(tmp.path());
        assert!(config.rule_overrides.is_empty());
        assert!(config.ignore_paths.is_empty());
    }

    /// `cmd_lint` returns an error immediately when given an unrecognised reporter string.
    #[test]
    fn cmd_lint_unknown_reporter_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        let result = cmd_lint(tmp.path().to_path_buf(), None, "not-a-reporter", "auto", None, None);
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown reporter"), "unexpected error: {err}");
    }

    /// `cmd_lint` returns an error when `--source` is provided but is not one of
    /// the supported values, covering the `if let Some(ref src) = source` True
    /// branch (line 719) and the `if !SUPPORTED_SOURCES.contains` True branch
    /// (line 721).
    #[test]
    fn cmd_lint_unsupported_source_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        let result = cmd_lint(
            tmp.path().to_path_buf(),
            Some("not-a-source".to_string()),
            "human",
            "auto",
            None,
            None,
        );
        assert!(result.is_err(), "cmd_lint with unsupported source should fail");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("unsupported source"), "expected 'unsupported source' in: {msg}");
    }

    /// `load_lint_config` forwards unknown TOML keys (beyond level/ignore) into
    /// the `options` map of `RuleOverride::Detailed`.
    #[test]
    fn load_lint_config_custom_keys_forwarded_to_options() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("aipm.toml"),
            "[workspace.lints.\"instructions/oversized\"]\nlevel = \"warn\"\nlines = 200\ncharacters = 20000\nresolve-imports = true\n",
        )
        .unwrap();

        let config = load_lint_config(tmp.path());
        let opts = config.rule_options("instructions/oversized");
        assert_eq!(opts.get("lines"), Some(&toml::Value::Integer(200)));
        assert_eq!(opts.get("characters"), Some(&toml::Value::Integer(20_000)));
        assert_eq!(opts.get("resolve-imports"), Some(&toml::Value::Boolean(true)));
        // level and ignore must NOT appear in options
        assert!(!opts.contains_key("level"));
        assert!(!opts.contains_key("ignore"));
    }

    /// Existing aipm.toml files with only level/ignore continue to work; the
    /// `options` BTreeMap in `RuleOverride::Detailed` is empty but not absent.
    #[test]
    fn load_lint_config_backward_compatible_level_ignore_only() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("aipm.toml"),
            "[workspace.lints.\"skill/oversized\"]\nlevel = \"error\"\nignore = [\"examples/**\"]\n",
        )
        .unwrap();

        let config = load_lint_config(tmp.path());
        let opts = config.rule_options("skill/oversized");
        assert!(opts.is_empty(), "options must be empty for level/ignore-only config");
        assert_eq!(
            config.severity_override("skill/oversized"),
            Some(libaipm::lint::Severity::Error)
        );
    }

    /// Custom option keys are recorded even when `level` is absent, so that
    /// users can configure e.g. `lines = 200` without also specifying `level`.
    #[test]
    fn load_lint_config_options_recorded_without_level() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("aipm.toml"),
            "[workspace.lints.\"instructions/oversized\"]\nlines = 200\ncharacters = 20000\n",
        )
        .unwrap();

        let config = load_lint_config(tmp.path());
        let opts = config.rule_options("instructions/oversized");
        assert_eq!(opts.get("lines"), Some(&toml::Value::Integer(200)));
        assert_eq!(opts.get("characters"), Some(&toml::Value::Integer(20_000)));
        // No level specified → severity_override returns None (rule uses its default)
        assert_eq!(config.severity_override("instructions/oversized"), None);
    }

    /// `resolve_plugins_dir` falls back to `.ai` when the manifest has a
    /// `[workspace]` table but no `plugins_dir` key.
    #[test]
    fn resolve_plugins_dir_no_plugins_dir_falls_back_to_dot_ai() {
        let tmp = tempfile::tempdir().unwrap();
        // Manifest has [workspace] but no plugins_dir field.
        std::fs::write(tmp.path().join("aipm.toml"), "[workspace]\nmembers = [\".ai/*\"]\n")
            .unwrap();

        let result = resolve_plugins_dir(tmp.path());
        assert_eq!(result, tmp.path().join(".ai"));
    }

    /// `resolve_plugins_dir` returns the custom path when `plugins_dir` is set
    /// in the `[workspace]` table.
    #[test]
    fn resolve_plugins_dir_uses_custom_plugins_dir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("aipm.toml"),
            "[workspace]\nmembers = []\nplugins_dir = \"my-plugins\"\n",
        )
        .unwrap();

        let result = resolve_plugins_dir(tmp.path());
        assert_eq!(result, tmp.path().join("my-plugins"));
    }

    /// `resolve_plugins_dir` falls back to `.ai` when the manifest has no
    /// `[workspace]` section at all.
    #[test]
    fn resolve_plugins_dir_no_workspace_section_falls_back_to_dot_ai() {
        let tmp = tempfile::tempdir().unwrap();
        // Manifest with only [dependencies], no [workspace] section.
        std::fs::write(tmp.path().join("aipm.toml"), "[dependencies]\n").unwrap();

        let result = resolve_plugins_dir(tmp.path());
        assert_eq!(result, tmp.path().join(".ai"));
    }

    /// `load_lint_config` handles a `[workspace.lints.ignore]` section that lacks
    /// a `paths` array — the inner branch is skipped, `ignore_paths` stays empty.
    #[test]
    fn load_lint_config_ignore_section_without_paths_array() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("aipm.toml"),
            "[workspace.lints.ignore]\ncomment = \"no paths key here\"\n",
        )
        .unwrap();

        let config = load_lint_config(tmp.path());
        assert!(config.ignore_paths.is_empty());
    }

    /// `load_lint_config` silently skips non-string entries in the `paths` array;
    /// only string values are added to `ignore_paths`.
    #[test]
    fn load_lint_config_ignore_paths_non_string_entry_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("aipm.toml"),
            "[workspace.lints.ignore]\npaths = [42, \"valid/path\"]\n",
        )
        .unwrap();

        let config = load_lint_config(tmp.path());
        assert_eq!(config.ignore_paths, vec!["valid/path"]);
    }

    /// A rule override that is a string but not "allow" and not a known severity
    /// level is silently ignored — no override is recorded.
    #[test]
    fn load_lint_config_string_rule_unknown_value_is_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("aipm.toml"),
            "[workspace.lints]\n\"some-rule\" = \"garbage_value\"\n",
        )
        .unwrap();

        let config = load_lint_config(tmp.path());
        assert!(!config.rule_overrides.contains_key("some-rule"));
    }

    /// A rule override value that is neither a string nor a table (e.g., an
    /// integer) is silently skipped.
    #[test]
    fn load_lint_config_non_string_non_table_rule_value_is_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("aipm.toml"), "[workspace.lints]\n\"some-rule\" = 42\n")
            .unwrap();

        let config = load_lint_config(tmp.path());
        assert!(!config.rule_overrides.contains_key("some-rule"));
    }

    /// A rule set to the bare string `"allow"` in `[workspace.lints]` maps to
    /// `RuleOverride::Allow`, which marks the rule as suppressed.
    /// This covers the `if s == "allow"` True branch inside `load_lint_config`.
    #[test]
    fn load_lint_config_string_allow_inserts_allow_override() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("aipm.toml"),
            "[workspace.lints]\n\"skill/oversized\" = \"allow\"\n",
        )
        .unwrap();

        let config = load_lint_config(tmp.path());
        assert!(config.is_suppressed("skill/oversized"), "rule set to 'allow' must be suppressed");
        assert_eq!(
            config.severity_override("skill/oversized"),
            None,
            "suppressed rule must not have a severity override"
        );
    }

    /// An empty rule table (no `level`, no `ignore`, no custom keys) does not
    /// produce any rule override — the guard in `load_lint_config` requires at
    /// least one meaningful field to record an override.
    #[test]
    fn load_lint_config_empty_rule_table_produces_no_override() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("aipm.toml"), "[workspace.lints.\"some-rule\"]\n").unwrap();

        let config = load_lint_config(tmp.path());
        assert!(!config.rule_overrides.contains_key("some-rule"));
    }

    /// `load_installed_registry` parses a valid JSON registry file when the path
    /// exists, covering the `path.exists()` → read-and-parse branch.
    #[test]
    fn load_installed_registry_parses_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let registry_path = tmp.path().join("installed.json");
        std::fs::write(
            &registry_path,
            r#"{"plugins":[{"spec":"github:owner/repo","engines":["claude"]}]}"#,
        )
        .unwrap();

        let registry = load_installed_registry(&registry_path).unwrap();
        assert_eq!(registry.plugins.len(), 1);
        assert_eq!(registry.plugins[0].spec, "github:owner/repo");
        assert_eq!(registry.plugins[0].engines, vec!["claude"]);
    }

    /// `load_lint_config` returns a default config when reading `aipm.toml`
    /// fails with a non-`NotFound` IO error (e.g., the path is a directory),
    /// covering the `Err(e)` arm after the `NotFound` guard fails.
    #[test]
    fn load_lint_config_non_not_found_io_error_returns_default() {
        let tmp = tempfile::tempdir().unwrap();
        // Create a directory named "aipm.toml" — read_to_string on a directory
        // fails with an IO error that is NOT ErrorKind::NotFound.
        std::fs::create_dir(tmp.path().join("aipm.toml")).unwrap();

        let config = load_lint_config(tmp.path());
        assert!(config.rule_overrides.is_empty());
        assert!(config.ignore_paths.is_empty());
    }

    /// A rule override table with only an `ignore` array (no `level`, no custom
    /// options) must produce a `RuleOverride::Detailed` entry. This exercises the
    /// `!ignore.is_empty()` True branch in the short-circuit condition
    /// `level.is_some() || !ignore.is_empty() || !options.is_empty()` that guards
    /// creation of the `Detailed` variant.
    #[test]
    fn load_lint_config_ignore_only_rule_creates_detailed_override() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("aipm.toml"),
            "[workspace.lints.\"some-rule\"]\nignore = [\"examples/**\", \"docs/**\"]\n",
        )
        .unwrap();

        let config = load_lint_config(tmp.path());
        assert!(config.rule_overrides.contains_key("some-rule"), "override must be recorded");
        assert_eq!(config.rule_ignore_paths("some-rule"), &["examples/**", "docs/**"]);
        assert_eq!(config.severity_override("some-rule"), None);
        assert!(config.rule_options("some-rule").is_empty());
    }

    /// `day_of_year_to_month_day` returns early via the `if remaining < days_in_month`
    /// branch when a day falls within the first month.  With `day = 0` the very
    /// first guard fires (0 < 31) and returns January 1, exercising the True
    /// branch of that inner `if`.
    #[test]
    fn day_of_year_to_month_day_within_month_hits_early_return() {
        // day = 0 → January 1 via the True branch of `if remaining < days_in_month`
        let (month, day) = day_of_year_to_month_day(0);
        assert_eq!(month, 1, "day 0 should be January");
        assert_eq!(day, 1, "day 0 should be the 1st");
    }

    /// `day_of_year_to_month_day` has a post-loop fallback branch (after
    /// iterating all 12 months) that is reached when `day >= 365`.  With
    /// `day = 365` the December guard (`31 < 31`) evaluates to false, the
    /// subtraction leaves `remaining = 0`, and the loop exits without an
    /// early return.  The post-loop `(12, remaining + 1)` then fires.
    #[test]
    fn day_of_year_to_month_day_overflow_hits_post_loop_fallback() {
        // day = 365 exhausts all 12 months without the inner guard firing for
        // December (31 < 31 is false), so the function exits via the fallback.
        let (month, day) = day_of_year_to_month_day(365);
        assert_eq!(month, 12, "overflow should land in December");
        assert_eq!(day, 1, "remaining after subtracting all months should be 0, giving day 1");
    }

    /// `resolve_dir` with `"."` returns the current working directory, covering
    /// the `if dir.as_os_str() == "."` True branch in `resolve_dir`.
    #[test]
    fn resolve_dir_dot_returns_current_dir() {
        let result = resolve_dir(PathBuf::from("."));
        assert!(result.is_ok(), "resolve_dir('.') should succeed");
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(result.unwrap(), cwd, "resolve_dir('.') should equal current_dir()");
    }

    /// `cmd_make_plugin` returns an `UnsupportedFeature` error when a feature
    /// not supported by the target engine is requested, exercising the
    /// `if let Some(first) = unsupported.first()` True branch.
    /// `"lsp"` is valid for Copilot but unsupported by the `"claude"` engine.
    #[test]
    fn cmd_make_plugin_unsupported_feature_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let result = cmd_make_plugin(
            Some("my-plugin"),
            Some("claude"),
            &["lsp".to_string()],
            true, // yes → non-interactive, skips wizard
            tmp.path().to_path_buf(),
        );
        assert!(result.is_err(), "expected an error for unsupported feature");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("lsp"), "error message should name the unsupported feature");
    }

    /// `cmd_uninstall_global` covers the False branch of `if !changed` (line 657):
    /// when the registry entry is found and removed, `changed` is `true` and the
    /// function writes the updated registry then returns `Ok`.
    ///
    /// A static mutex serialises the `HOME` env-var mutation so this test is safe
    /// to run alongside other parallel tests.
    #[test]
    fn cmd_uninstall_global_success_returns_ok() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

        let tmp = tempfile::tempdir().unwrap_or_else(|_| panic!("tempdir creation failed"));
        let aipm_dir = tmp.path().join(".aipm");
        std::fs::create_dir_all(&aipm_dir).unwrap_or_else(|e| panic!("create_dir_all failed: {e}"));

        // Seed the registry with one plugin so resolve_spec + uninstall both succeed.
        let registry_json = r#"{"plugins":[{"spec":"local:./my-plugin"}]}"#;
        std::fs::write(aipm_dir.join("installed.json"), registry_json)
            .unwrap_or_else(|e| panic!("write failed: {e}"));

        let prev_home = std::env::var("HOME").ok();
        // SAFETY: no other thread modifies HOME while ENV_LOCK is held.
        std::env::set_var("HOME", tmp.path());

        let result = cmd_uninstall_global("local:./my-plugin", None, PathBuf::from("/tmp"));

        match prev_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }

        assert!(result.is_ok(), "uninstall of existing plugin should succeed: {result:?}");
    }

    /// `cmd_uninstall_global` with `engine = Some("claude")` covers the True branches
    /// of both `if let Some(eng) = engine` checks (lines 651 and 666):
    /// - line 651: `registry.uninstall_engine(&spec, &[eng.to_string()])` is called
    ///   instead of `registry.uninstall(&spec)`.
    /// - line 666: the output message is engine-scoped ("Removed … from claude engine").
    ///
    /// The registry is seeded with a plugin pinned to the "claude" engine so that
    /// `resolve_spec` succeeds and `uninstall_engine` returns `true`.
    #[test]
    fn cmd_uninstall_global_engine_specific_covers_engine_branches() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

        let tmp = tempfile::tempdir().expect("tempdir");
        let aipm_dir = tmp.path().join(".aipm");
        std::fs::create_dir_all(&aipm_dir).expect("create .aipm dir");

        // Seed the registry with a plugin scoped to the "claude" engine.
        let registry_json = r#"{"plugins":[{"spec":"local:./my-plugin","engines":["claude"]}]}"#;
        std::fs::write(aipm_dir.join("installed.json"), registry_json)
            .expect("write installed.json");

        let prev_home = std::env::var("HOME").ok();
        // SAFETY: no other thread modifies HOME while ENV_LOCK is held.
        std::env::set_var("HOME", tmp.path());

        let result =
            cmd_uninstall_global("local:./my-plugin", Some("claude"), PathBuf::from("/tmp"));

        match prev_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }

        assert!(
            result.is_ok(),
            "engine-specific uninstall of existing plugin should succeed: {result:?}"
        );
    }

    /// `cmd_lint` with the `"ci-github"` reporter exercises the `CiGitHub.report()`
    /// branch in the `match effective_reporter` block. On a clean (empty) directory
    /// the linter finds no violations and the reporter produces no output.
    #[test]
    fn cmd_lint_ci_github_reporter_succeeds_on_clean_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let result = cmd_lint(tmp.path().to_path_buf(), None, "ci-github", "auto", None, None);
        assert!(result.is_ok(), "ci-github reporter should succeed on clean dir: {result:?}");
    }

    /// `cmd_lint` with the `"ci-azure"` reporter exercises the `CiAzure.report()`
    /// branch in the `match effective_reporter` block. On a clean (empty) directory
    /// the linter finds no violations and the reporter produces no output.
    #[test]
    fn cmd_lint_ci_azure_reporter_succeeds_on_clean_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let result = cmd_lint(tmp.path().to_path_buf(), None, "ci-azure", "auto", None, None);
        assert!(result.is_ok(), "ci-azure reporter should succeed on clean dir: {result:?}");
    }

    /// `cmd_lint` returns `Err` when the linter finds at least one error-severity
    /// diagnostic, covering the `if outcome.error_count > 0` True branch (line 781).
    /// A plugin.json that is missing required fields (`name`, `author`) triggers
    /// `plugin/required-fields` at `Severity::Error`.
    #[test]
    fn cmd_lint_with_error_diagnostics_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        // Create a plugin.json missing required fields; this fires plugin/required-fields
        // at Severity::Error, causing outcome.error_count > 0.
        let plugin_dir = tmp.path().join(".ai").join("my-plugin").join(".claude-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.json"),
            r#"{"version":"0.1.0","description":"A plugin"}"#,
        )
        .unwrap();

        let result = cmd_lint(tmp.path().to_path_buf(), None, "human", "never", None, None);
        assert!(result.is_err(), "cmd_lint must return Err when lint errors are found");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("lint found"), "error message must mention lint errors: {msg}");
    }

    /// A rule override value that is a bare valid-severity string (e.g. `"warn"`)
    /// records a `RuleOverride::Level` entry — covering the
    /// `else if let Some(severity) = Severity::from_str_config(s)` True branch
    /// inside `load_lint_config` (the path reached when the value is a non-`"allow"`
    /// string that IS recognised as a severity level).
    #[test]
    fn load_lint_config_string_severity_inserts_level_override() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("aipm.toml"),
            "[workspace.lints]\n\"skill/oversized\" = \"warn\"\n",
        )
        .unwrap();

        let config = load_lint_config(tmp.path());
        assert!(
            !config.is_suppressed("skill/oversized"),
            "warn severity must not suppress the rule"
        );
        assert_eq!(
            config.severity_override("skill/oversized"),
            Some(libaipm::lint::Severity::Warning),
            "bare 'warn' string must map to a Warning severity override"
        );
    }

    /// `cmd_lint` with the legacy `"text"` reporter maps it to `"human"`, covering
    /// the `"text" => "human"` match arm inside `cmd_lint`.
    #[test]
    fn cmd_lint_text_reporter_maps_to_human() {
        let tmp = tempfile::tempdir().unwrap();
        let result = cmd_lint(tmp.path().to_path_buf(), None, "text", "auto", None, None);
        assert!(
            result.is_ok(),
            "\"text\" reporter (mapped to human) should succeed on a clean dir: {result:?}"
        );
    }

    /// `cmd_lint` with `color = "always"` selects `ColorChoice::Always`, covering
    /// the `"always" => ColorChoice::Always` match arm inside `cmd_lint`.
    #[test]
    fn cmd_lint_color_always_selects_color_choice() {
        let tmp = tempfile::tempdir().unwrap();
        let result = cmd_lint(tmp.path().to_path_buf(), None, "human", "always", None, None);
        assert!(result.is_ok(), "color=always should succeed on a clean dir: {result:?}");
    }
}
