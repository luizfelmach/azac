mod azcli;
mod cache;
mod commands;
mod context;

use clap::{Parser, Subcommand};
use commands::kv;
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
    /// Export configuration data
    Export {
        #[arg(short = 'o', long = "output", value_enum)]
        format: Option<kv::ExportFormat>,
        file: PathBuf,
    },
    /// Import configuration data from a file
    Import { file: PathBuf },
    /// Promote a plain value to a Key Vault reference
    Promote { key: String },
    /// Demote a Key Vault reference to a plain value
    Demote { key: String },
    /// Refresh cached Azure metadata used during setup
    Sync,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Setup => commands::setup(),
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
        Command::Export { format, file } => kv::export_entries(format, &file),
        Command::Import { file } => kv::import_entries(&file),
        Command::Sync => commands::sync(),
    }
}
