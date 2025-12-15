use crate::{cmd::ContextCommand, context, prompt};

pub fn handle(action: ContextCommand) {
    match action {
        ContextCommand::Current => match context::current().unwrap() {
            Some(ctx) => {
                println!("Current context: {}", ctx.alias);
            }
            None => println!("No current context set."),
        },

        ContextCommand::Set { alias } => match context::set(&alias) {
            Ok(_) => println!("Current context set to '{alias}'."),
            Err(err) => eprintln!("Failed to set current context: {err}"),
        },
        ContextCommand::Edit { alias } => {
            let existing = match context::get(&alias) {
                Ok(ctx) => ctx,
                Err(err) => {
                    eprintln!("Failed to load context '{alias}': {err}");
                    return;
                }
            };

            let updated = prompt::edit_context(&existing).unwrap();
            let updated_alias = updated.alias.clone();

            match context::update(&alias, updated) {
                Ok(_) => println!("Updated context '{updated_alias}'."),
                Err(err) => eprintln!("Failed to update context: {err}"),
            }
        }
        ContextCommand::Rename { from, to } => match context::rename(&from, &to) {
            Ok(_) => println!("Renamed context '{from}' -> '{to}'."),
            Err(err) => eprintln!("Failed to rename context: {err}"),
        },
        ContextCommand::Clone { from, to } => match context::clone(&from, &to) {
            Ok(_) => println!("Cloned context '{from}' -> '{to}'."),
            Err(err) => eprintln!("Failed to clone context: {err}"),
        },
        ContextCommand::List => {
            let entries = context::list().unwrap();
            println!("Entries {entries:?}");
        }
        ContextCommand::Delete { alias } => match context::delete(&alias) {
            Ok(_) => println!("Deleted context '{alias}'."),
            Err(err) => eprintln!("Failed to delete context: {err}"),
        },
    }
}
