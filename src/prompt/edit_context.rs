use crate::context::Context;
use inquire::Text;

pub fn edit_context(existing: &Context) -> inquire::error::InquireResult<Context> {
    let sub = Text::new("Azure subscription")
        .with_default(&existing.sub)
        .prompt()?;

    let name = Text::new("App Configuration name")
        .with_default(&existing.name)
        .prompt()?;

    let separator = Text::new("Key separator")
        .with_default(&existing.separator)
        .prompt()?;

    let base = Text::new("Base key prefix (e.g. app1:prd)")
        .with_default(&existing.base)
        .prompt()?;

    let label = Text::new("Default label")
        .with_default(&existing.label)
        .prompt()?;

    let alias = Text::new("Config alias (e.g. app1)")
        .with_default(&existing.alias)
        .prompt()?;

    Ok(Context {
        alias,
        sub,
        name,
        base,
        separator,
        label,
    })
}
