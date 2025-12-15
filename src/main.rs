use crate::cmd::{Cli, Command};
use clap::Parser;

mod azcli;
mod cmd;
mod context;
mod prompt;

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Setup => cmd::setup::handle(),
        Command::Context { action } => cmd::context::handle(action),
    }
}
