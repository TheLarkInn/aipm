//! `aipm` — consumer CLI for AI plugin management.
//!
//! Commands: init, install, update, link, unlink, list, lint, migrate.

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
fn resolve_dir(dir: PathBuf) -> Result<PathBuf, Box<dyn std::error::Error>> {
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
    match libaipm::manifest::load(&manifest_path) {
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
fn home_store_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
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
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
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
) -> Result<(), Box<dyn std::error::Error>> {
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

fn cmd_install(
    package: Option<String>,
    locked: bool,
    dir: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = resolve_dir(dir)?;

    // Discover workspace root if we're inside one
    let workspace_root = libaipm::workspace::find_workspace_root(&dir);
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
    let result = libaipm::installer::pipeline::install(&config, &registry)?;

    let mut stdout = std::io::stdout();
    let _ = writeln!(
        stdout,
        "Installed {} package(s), {} up-to-date, {} removed",
        result.installed, result.up_to_date, result.removed
    );
    Ok(())
}

fn cmd_update(package: Option<String>, dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
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
    let result = libaipm::installer::pipeline::update(&config, &registry)?;

    let mut stdout = std::io::stdout();
    let _ = writeln!(
        stdout,
        "Updated {} package(s), {} up-to-date, {} removed",
        result.installed, result.up_to_date, result.removed
    );
    Ok(())
}

fn cmd_link(path: PathBuf, dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
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
    let manifest = libaipm::manifest::load(&target_manifest)?;
    let pkg_name = manifest
        .package
        .as_ref()
        .map(|p| p.name.clone())
        .ok_or("manifest at linked path has no [package] section")?;

    let plugins_dir = resolve_plugins_dir(&dir);
    let link_target = plugins_dir.join(&pkg_name);

    // Create the directory link
    std::fs::create_dir_all(&plugins_dir)?;
    libaipm::linker::directory_link::create(&path, &link_target)
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    // Update link state
    let link_state_path = dir.join(".aipm/links.toml");
    let entry = libaipm::linker::link_state::LinkEntry {
        name: pkg_name.clone(),
        path: path.clone(),
        linked_at: timestamp_now(),
    };
    libaipm::linker::link_state::add(&link_state_path, entry)
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "Linked '{pkg_name}' → {}", path.display());
    Ok(())
}

fn cmd_unlink(package: &str, dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let dir = resolve_dir(dir)?;
    let plugins_dir = resolve_plugins_dir(&dir);
    let links_dir = dir.join(".aipm/links");

    libaipm::linker::pipeline::unlink_package(package, &links_dir, &plugins_dir)
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let link_state_path = dir.join(".aipm/links.toml");
    libaipm::linker::link_state::remove(&link_state_path, package)
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let gitignore_path = plugins_dir.join(".gitignore");
    if gitignore_path.exists() {
        libaipm::linker::gitignore::remove_entry(&gitignore_path, package)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
    }

    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "Unlinked '{package}'");
    Ok(())
}

fn cmd_list(linked: bool, dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let dir = resolve_dir(dir)?;
    let mut stdout = std::io::stdout();

    if linked {
        let link_state_path = dir.join(".aipm/links.toml");
        let entries = libaipm::linker::link_state::list(&link_state_path)
            .map_err(|e| std::io::Error::other(e.to_string()))?;

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
            let lf = libaipm::lockfile::read(&lockfile_path)
                .map_err(|e| std::io::Error::other(e.to_string()))?;

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
) -> Result<(), Box<dyn std::error::Error>> {
    let spec = package.ok_or("--global install requires a package spec")?;
    let engines: Vec<String> = engine.into_iter().collect();
    let cache_policy: Option<libaipm::cache::Policy> =
        plugin_cache.map(|s| s.parse()).transpose().map_err(|e: String| e)?;

    // Load or create the global installed registry
    let registry_path = home_aipm_path()?.join("installed.json");
    let mut registry = load_installed_registry(&registry_path);

    let added = registry.install(spec.clone(), &engines, cache_policy, None)?;

    // Save under lock
    let json = serde_json::to_string_pretty(&registry)?;
    let mut locked = libaipm::locked_file::LockedFile::open(&registry_path)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    locked.write_content(&json).map_err(|e| std::io::Error::other(e.to_string()))?;

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
) -> Result<(), Box<dyn std::error::Error>> {
    let registry_path = home_aipm_path()?.join("installed.json");
    let mut registry = load_installed_registry(&registry_path);

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
    let mut locked = libaipm::locked_file::LockedFile::open(&registry_path)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    locked.write_content(&json).map_err(|e| std::io::Error::other(e.to_string()))?;

    let mut stdout = std::io::stdout();
    if let Some(eng) = engine {
        let _ = writeln!(stdout, "Removed '{spec}' from {eng} engine globally");
    } else {
        let _ = writeln!(stdout, "Uninstalled '{spec}' globally");
    }
    Ok(())
}

fn cmd_list_global() -> Result<(), Box<dyn std::error::Error>> {
    let registry_path = home_aipm_path()?.join("installed.json");
    let registry = load_installed_registry(&registry_path);

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
fn home_aipm_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "could not determine home directory")?;
    Ok(PathBuf::from(home).join(".aipm"))
}

/// Load the global installed registry from disk (or empty default).
fn load_installed_registry(path: &Path) -> libaipm::installed::Registry {
    if !path.exists() {
        return libaipm::installed::Registry::default();
    }
    std::fs::read_to_string(path).map_or_else(
        |_| libaipm::installed::Registry::default(),
        |content| serde_json::from_str(&content).unwrap_or_default(),
    )
}

fn cmd_lint(
    dir: PathBuf,
    source: Option<String>,
    reporter: &str,
    color: &str,
    format: Option<&str>,
    max_depth: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
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

fn load_lint_config(dir: &Path) -> libaipm::lint::config::Config {
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
            let ignore = table
                .get("ignore")
                .and_then(toml::Value::as_array)
                .map(|arr| arr.iter().filter_map(toml::Value::as_str).map(String::from).collect())
                .unwrap_or_default();

            if let Some(lvl) = level {
                config.rule_overrides.insert(
                    key.clone(),
                    libaipm::lint::config::RuleOverride::Detailed { level: lvl, ignore },
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
) -> Result<(), Box<dyn std::error::Error>> {
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

// =========================================================================
// Entry point
// =========================================================================

fn run() -> Result<(), Box<dyn std::error::Error>> {
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
