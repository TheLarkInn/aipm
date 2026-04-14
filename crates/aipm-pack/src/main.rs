//! `aipm-pack` — author CLI for AI plugin packages.
//!
//! Commands: init.
//! Planned (not yet implemented): pack, publish, yank, login.

mod error;
mod wizard;
mod wizard_tty;

use std::io::{IsTerminal, Write};
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use libaipm::init::{self, Options};
use libaipm::manifest::types::PluginType;

#[derive(Parser)]
#[command(name = "aipm-pack", version = libaipm::version(), about = "AI Plugin Manager — author CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
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

fn run() -> Result<(), error::CliError> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init { yes, name, r#type, dir }) => {
            let plugin_type = r#type.as_deref().map(str::parse::<PluginType>).transpose()?;

            let dir = if dir.as_os_str() == "." { std::env::current_dir()? } else { dir };

            let interactive = !yes && std::io::stdin().is_terminal();

            let (final_name, final_type) =
                wizard_tty::resolve(interactive, &dir, name, plugin_type)?;

            let opts = Options { dir: &dir, name: final_name.as_deref(), plugin_type: final_type };

            init::init(&opts, &libaipm::fs::Real)?;

            let mut stdout = std::io::stdout();
            let _ = writeln!(stdout, "Initialized plugin package in {}", dir.display());
            Ok(())
        },
        None => {
            let mut stdout = std::io::stdout();
            let _ = writeln!(stdout, "aipm-pack {}", libaipm::version());
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
