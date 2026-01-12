mod azcli;
mod commands;
mod context;

use clap::{Parser, Subcommand};
use commands::{app, cfg, kv};
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
    /// Manage App Configuration instances
    #[command(subcommand)]
    Cfg(CfgCommand),
    /// Manage applications under the current App Configuration
    #[command(subcommand)]
    App(AppCommand),
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
}

#[derive(Subcommand)]
enum CfgCommand {
    /// List App Configuration instances
    List,
    /// Set the active App Configuration
    Use { cfg: String },
    /// Show information about an App Configuration
    Show { cfg: String },
    /// Set the key separator for the current App Configuration
    Separator { separator: String },
    /// Display the currently selected App Configuration
    Current,
}

#[derive(Subcommand)]
enum AppCommand {
    /// List available applications
    List,
    /// Set the active application
    Use { app: String },
    /// Show application details
    Show { app: String },
    /// Set the label for the current application
    Label { label: String },
    /// Set the Key Vault reference for the current application
    Keyvault { vault: String },
    /// Display the currently selected application
    Current,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Cfg(cfg_command) => match cfg_command {
            CfgCommand::List => cfg::list_configs(),
            CfgCommand::Use { cfg } => cfg::use_config(&cfg),
            CfgCommand::Show { cfg } => cfg::show_config(&cfg),
            CfgCommand::Separator { separator } => cfg::set_separator(&separator),
            CfgCommand::Current => cfg::show_current_config(),
        },
        Command::App(app_command) => match app_command {
            AppCommand::List => app::list_apps(),
            AppCommand::Use { app } => app::use_app(&app),
            AppCommand::Show { app } => app::show_app(&app),
            AppCommand::Label { label } => app::set_label(&label),
            AppCommand::Keyvault { vault } => app::set_keyvault(&vault),
            AppCommand::Current => app::show_current_app(),
        },
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
    }
}
