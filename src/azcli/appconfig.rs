use super::{error::AzCliResult, run::az};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub id: String,
    pub name: String,
}

pub fn list_appconfig(subscription: &str) -> AzCliResult<Vec<AppConfig>> {
    az([
        "appconfig",
        "list",
        "--subscription",
        subscription,
        "-o",
        "json",
    ])
}
