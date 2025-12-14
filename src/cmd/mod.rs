pub mod context;
pub mod setup;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "azac")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Setup,
    Context {
        #[command(subcommand)]
        action: ContextCommand,
    },
}

#[derive(Subcommand, Default)]
pub enum ContextCommand {
    #[default]
    Current,
    Set {
        alias: String,
    },
    Edit {
        alias: String,
    },
    Rename {
        from: String,
        to: String,
    },
    Clone {
        from: String,
        to: String,
    },
    List,
    Delete {
        alias: String,
    },
}
