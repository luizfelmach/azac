use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    pub alias: String,
    pub sub: String,
    pub name: String,
    pub base: String,
    pub separator: String,
    pub label: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct ContextStore {
    pub current: Option<String>,
    pub contexts: HashMap<String, Context>,
}
