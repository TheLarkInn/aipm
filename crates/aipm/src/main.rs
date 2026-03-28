//! `aipm` — consumer CLI for AI plugin management.
//!
//! Commands: init, install, update, link, unlink, list, migrate.

mod wizard;
mod wizard_tty;

use std::io::{IsTerminal, Write};
use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "aipm", version = libaipm::version(), about = "AI Plugin Manager — consumer CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
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

    /// Install packages from the registry.
    Install {
        /// Package to install (e.g., "code-review", "@org/tool@^1.0").
        package: Option<String>,

        /// CI mode: fail if lockfile doesn't match manifest.
        #[arg(long)]
        locked: bool,

        /// Use a specific registry.
        #[arg(long)]
        registry: Option<String>,

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

        /// Project directory.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
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

    let config = libaipm::installer::pipeline::InstallConfig {
        manifest_path: dir.join("aipm.toml"),
        lockfile_path: dir.join("aipm.lock"),
        store_path: home_store_path()?,
        links_dir: dir.join(".aipm/links"),
        plugins_dir: dir.join(".ai"),
        gitignore_path: dir.join(".ai/.gitignore"),
        link_state_path: dir.join(".aipm/links.toml"),
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

    let config = libaipm::installer::pipeline::UpdateConfig {
        manifest_path: dir.join("aipm.toml"),
        lockfile_path: dir.join("aipm.lock"),
        store_path: home_store_path()?,
        links_dir: dir.join(".aipm/links"),
        plugins_dir: dir.join(".ai"),
        gitignore_path: dir.join(".ai/.gitignore"),
        link_state_path: dir.join(".aipm/links.toml"),
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

    let plugins_dir = dir.join(".ai");
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
    let plugins_dir = dir.join(".ai");
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
            libaipm::migrate::Action::PluginCreated { name, source, plugin_type } => {
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
            libaipm::migrate::Action::SourceFileRemoved { .. }
            | libaipm::migrate::Action::SourceDirRemoved { .. } => {
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
                    let _ = writeln!(stdout, "Removed source: {}", path.display());
                },
                libaipm::migrate::Action::SourceDirRemoved { path } => {
                    let _ = writeln!(stdout, "Removed empty directory: {}", path.display());
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

    match cli.command {
        Some(Commands::Init { yes, workspace, marketplace, no_starter, manifest, name, dir }) => {
            let flags = InitWizardFlags { yes, workspace, marketplace, no_starter };
            cmd_init(&flags, manifest, name.as_deref(), dir)
        },
        Some(Commands::Install { package, locked, registry, dir }) => {
            if registry.is_some() {
                let mut stderr = std::io::stderr();
                let _ = writeln!(
                    stderr,
                    "warning: --registry is not yet supported and will be ignored"
                );
            }
            cmd_install(package, locked, dir)
        },
        Some(Commands::Update { package, dir }) => cmd_update(package, dir),
        Some(Commands::Link { path, dir }) => cmd_link(path, dir),
        Some(Commands::Unlink { package, dir }) => cmd_unlink(&package, dir),
        Some(Commands::List { linked, dir }) => cmd_list(linked, dir),
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
