use crate::context::Context;
use inquire::{Select, Text};

pub fn setup_context() -> Context {
    let subscriptions = vec!["MEDSENIOR_NETWORK_TI".into(), "MEDSENIOR_IA_TI".into()];
    let sub = Select::new("Azure subscription", subscriptions)
        .prompt()
        .unwrap();

    let names = vec!["app1-prd".into(), "app1-hml".into()];
    let name = Select::new("App Configuration name", names)
        .prompt()
        .unwrap();

    let separator = Text::new("Key separator")
        .with_default(":")
        .prompt()
        .unwrap();

    let base = Text::new("Base key prefix (e.g. app1:prd)")
        .prompt()
        .unwrap();

    let label = Text::new("Default label")
        .with_default("default")
        .prompt()
        .unwrap();

    let alias = Text::new("Config alias (e.g. app1)").prompt().unwrap();

    Context {
        alias,
        sub,
        name,
        base,
        separator,
        label,
    }
}
