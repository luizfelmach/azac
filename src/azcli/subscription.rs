use super::{error::ResultAzCli, run::az};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub name: String,
    #[serde(rename = "tenantId")]
    pub tenant_id: String,
}

pub fn list_subscription() -> ResultAzCli<Vec<Subscription>> {
    az(["account", "list", "-o", "json"])
}
