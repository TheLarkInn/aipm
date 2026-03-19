//! `aipm` — consumer CLI for AI plugin management.
//!
//! Commands: init, install, validate, doctor, link, update, uninstall.

use std::io::Write;
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
        /// Generate a workspace manifest (aipm.toml with [workspace] section).
        #[arg(long)]
        workspace: bool,

        /// Generate a .ai/ local marketplace with tool settings.
        #[arg(long)]
        marketplace: bool,

        /// Directory to initialize (defaults to current directory).
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init { workspace, marketplace, dir }) => {
            let dir = if dir.as_os_str() == "." { std::env::current_dir()? } else { dir };

            // If neither flag is set, default to marketplace only
            let (do_workspace, do_marketplace) =
                if !workspace && !marketplace { (false, true) } else { (workspace, marketplace) };

            let adaptors = libaipm::workspace_init::adaptors::defaults();

            let opts = libaipm::workspace_init::Options {
                dir: &dir,
                workspace: do_workspace,
                marketplace: do_marketplace,
            };

            let result = libaipm::workspace_init::init(&opts, &adaptors)?;

            let mut stdout = std::io::stdout();
            for action in &result.actions {
                let msg = match action {
                    libaipm::workspace_init::InitAction::WorkspaceCreated => {
                        format!("Initialized workspace in {}", dir.display())
                    },
                    libaipm::workspace_init::InitAction::MarketplaceCreated => {
                        "Created .ai/ marketplace with starter plugin".to_string()
                    },
                    libaipm::workspace_init::InitAction::ToolConfigured(name) => {
                        format!("Configured {name} settings")
                    },
                };
                let _ = writeln!(stdout, "{msg}");
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
