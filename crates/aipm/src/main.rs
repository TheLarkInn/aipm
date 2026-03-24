//! `aipm` — consumer CLI for AI plugin management.
//!
//! Commands: init, install, validate, doctor, link, update, uninstall.

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

        /// Directory to initialize (defaults to current directory).
        #[arg(default_value = ".")]
        dir: PathBuf,
    },

    /// Migrate AI tool configurations into marketplace plugins.
    Migrate {
        /// Preview migration without writing files (generates report).
        #[arg(long)]
        dry_run: bool,

        /// Source folder to scan (e.g., ".claude").
        /// When omitted, recursively discovers all .claude/ directories.
        #[arg(long)]
        source: Option<String>,

        /// Maximum directory depth for recursive discovery.
        /// Ignored when --source is provided.
        #[arg(long)]
        max_depth: Option<usize>,

        /// Project directory.
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init { yes, workspace, marketplace, no_starter, dir }) => {
            let dir = if dir.as_os_str() == "." { std::env::current_dir()? } else { dir };

            let interactive = !yes && std::io::stdin().is_terminal();

            let (do_workspace, do_marketplace, do_no_starter) =
                wizard_tty::resolve(interactive, (workspace, marketplace, no_starter))?;

            let adaptors = libaipm::workspace_init::adaptors::defaults();

            let opts = libaipm::workspace_init::Options {
                dir: &dir,
                workspace: do_workspace,
                marketplace: do_marketplace,
                no_starter: do_no_starter,
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
                            "Created .ai/ marketplace (no starter plugin)".to_string()
                        } else {
                            "Created .ai/ marketplace with starter plugin".to_string()
                        }
                    },
                    libaipm::workspace_init::InitAction::ToolConfigured(name) => {
                        format!("Configured {name} settings")
                    },
                };
                let _ = writeln!(stdout, "{msg}");
            }
            Ok(())
        },
        Some(Commands::Migrate { dry_run, source, max_depth, dir }) => {
            let dir = if dir.as_os_str() == "." { std::env::current_dir()? } else { dir };

            let opts = libaipm::migrate::Options {
                dir: &dir,
                source: source.as_deref(),
                dry_run,
                max_depth,
            };

            let result = libaipm::migrate::migrate(&opts, &libaipm::fs::Real)?;

            let mut stdout = std::io::stdout();
            for action in &result.actions {
                match action {
                    libaipm::migrate::Action::PluginCreated { name, source, plugin_type } => {
                        let _ = writeln!(
                            stdout,
                            "Migrated {plugin_type} '{name}' from {}",
                            source.display()
                        );
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
                }
            }
            Ok(())
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
