mod azcli;
mod cache;
mod commands;
mod context;
mod convert;

use clap::{Parser, Subcommand};
use commands::kv;
use convert::ConvertCommand;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "azac",
    version,
    about = "better azure cli app configuration",
    long_about = "Opinionated tooling for managing Azure App Configuration contexts and orchestrating App Configuration workflows.",
    author = "Luiz Felipe Machado"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Configure the active App Configuration context and application
    Setup,
    /// Refresh cached Azure metadata used during setup
    Sync,
    /// List keys for the current App Configuration/App context
    #[command(alias = "ls")]
    List,
    /// Show a key by name
    Show { key: String },
    /// Set a key/value pair (optionally storing the value in Key Vault)
    Set {
        key: String,
        value: String,
        #[arg(long)]
        keyvault: bool,
    },
    /// Delete a key
    Delete {
        #[arg(required = true)]
        keys: Vec<String>,
    },
    /// Compare the current configuration against a saved export
    Plan { file: PathBuf },
    /// Export configuration data as YAML
    Export { file: PathBuf },
    /// Import configuration data from a file
    Import { file: PathBuf },
    /// Promote a plain value to a Key Vault reference
    Promote { key: String },
    /// Demote a Key Vault reference to a plain value
    Demote { key: String },
    /// Convert configuration files into the azac YAML schema
    Convert {
        #[command(subcommand)]
        target: ConvertCommand,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Setup => commands::setup(),
        Command::Sync => commands::sync(),
        Command::List => kv::list_keys(),
        Command::Show { key } => kv::show_key(&key),
        Command::Set {
            key,
            value,
            keyvault,
        } => kv::set_key(&key, &value, keyvault),
        Command::Promote { key } => kv::promote_key(&key),
        Command::Demote { key } => kv::demote_key(&key),
        Command::Delete { keys } => kv::delete_keys(&keys),
        Command::Plan { file } => kv::plan(&file),
        Command::Export { file } => kv::export_entries(&file),
        Command::Import { file } => kv::import_entries(&file),
        Command::Convert { target } => convert::run(target),
    }
}
