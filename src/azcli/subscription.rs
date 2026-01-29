use super::{error::AzCliResult, run::az};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Subscription {
    pub id: String,
    pub name: String,
}

pub fn list_subscription() -> AzCliResult<Vec<Subscription>> {
    az(["account", "list", "-o", "json"])
}
