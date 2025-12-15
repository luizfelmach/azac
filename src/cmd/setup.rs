use crate::{context, prompt::setup_context};

pub fn handle() {
    let ctx = setup_context();
    let alias = ctx.alias.clone();

    match context::save(ctx) {
        Ok(_) => match context::set(&alias) {
            Ok(_) => println!("Context saved and set as current: '{alias}'."),
            Err(err) => eprintln!("Context saved but failed to set current: {err}"),
        },
        Err(err) => eprintln!("Failed to save context: {err}"),
    }
}
