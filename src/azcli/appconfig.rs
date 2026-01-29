use super::{error::AzCliResult, run::az};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct AppConfig {
    pub name: String,
    #[serde(default)]
    pub endpoint: String,
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
