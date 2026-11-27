mod asar;
mod desktop;
mod download;
mod installer;
mod launcher;
mod patcher;
mod paths;
mod runtime;
mod setup;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::installer::InstallOptions;
use crate::paths::Paths;

#[derive(Parser)]
#[command(name = "kitbash-cargo-linux", version, about = "Runs Cargo by KitBash on Linux")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Build and install Cargo from the Windows setup")]
    Install {
        #[arg(long, value_name = "SETUP_EXE", help = "Use a local Windows setup instead of downloading the latest one")]
        setup: Option<PathBuf>,
        #[arg(long, value_name = "VERSION", help = "Override the detected Electron version")]
        electron_version: Option<String>,
        #[arg(long, value_name = "VERSIONS", value_delimiter = ',', help = "Also allow these Blender versions, for example 5.3")]
        blender_versions: Vec<String>,
    },
    #[command(about = "Install the latest Cargo release if a newer one is available")]
    Update {
        #[arg(long, help = "Reinstall even if the installed version is current")]
        force: bool,
        #[arg(long, value_name = "VERSIONS", value_delimiter = ',', help = "Replace the additionally allowed Blender versions")]
        blender_versions: Option<Vec<String>>,
    },
    #[command(about = "Start Cargo")]
    Launch {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<OsString>,
    },
    #[command(about = "Show installed and latest version")]
    Status,
    #[command(about = "Remove Cargo and its desktop integration, keeping user data")]
    Uninstall,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let paths = Paths::detect()?;
    match cli.command {
        Commands::Install {
            setup,
            electron_version,
            blender_versions,
        } => installer::install(
            &paths,
            InstallOptions {
                setup,
                electron_version,
                extra_blender_versions: blender_versions,
            },
        ),
        Commands::Update {
            force,
            blender_versions,
        } => installer::update(&paths, force, blender_versions),
        Commands::Launch { arguments } => launcher::launch(&paths, arguments),
        Commands::Status => installer::status(&paths),
        Commands::Uninstall => installer::uninstall(&paths),
    }
}
