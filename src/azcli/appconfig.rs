use super::{error::ResultAzCli, run::az};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub id: String,
    pub name: String,
}

pub fn list_appconfig(subscription: &str) -> ResultAzCli<Vec<AppConfig>> {
    az([
        "appconfig",
        "list",
        "--subscription",
        subscription,
        "-o",
        "json",
    ])
}
