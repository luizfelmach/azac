use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

const DEFAULT_SEPARATOR: &str = ":";

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("Could not determine configuration directory for azac")]
    MissingProjectDirs,
    #[error("Failed to read context file: {0}")]
    Read(#[from] std::io::Error),
    #[error("Failed to serialize or deserialize context file: {0}")]
    Serde(#[from] serde_yaml::Error),
}

pub type ContextResult<T> = Result<T, ContextError>;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Context {
    #[serde(flatten)]
    pub subscriptions: BTreeMap<String, SubscriptionContext>,
}

impl Context {
    pub fn load_or_default(store: &ContextStore) -> ContextResult<Self> {
        if !store.path.exists() {
            return Ok(Default::default());
        }

        let data = fs::read_to_string(&store.path)?;
        if data.trim().is_empty() {
            return Ok(Default::default());
        }

        Ok(serde_yaml::from_str(&data)?)
    }

    pub fn save(&self, store: &ContextStore) -> ContextResult<()> {
        if let Some(parent) = store.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let payload = serde_yaml::to_string(self)?;
        fs::write(&store.path, payload)?;
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SubscriptionContext {
    pub current: Option<String>,
    #[serde(flatten)]
    pub configs: BTreeMap<String, AppConfigurationContext>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppConfigurationContext {
    #[serde(default)]
    pub current: Option<String>,
    #[serde(default = "default_separator")]
    pub separator: String,
    #[serde(default)]
    pub apps: BTreeMap<String, AppContext>,
}

impl Default for AppConfigurationContext {
    fn default() -> Self {
        Self {
            current: None,
            separator: default_separator(),
            apps: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppContext {
    pub label: Option<String>,
    pub keyvault: Option<String>,
}

pub struct ContextStore {
    path: PathBuf,
}

impl ContextStore {
    pub fn new() -> ContextResult<Self> {
        let dirs = project_dirs().ok_or(ContextError::MissingProjectDirs)?;
        Ok(Self {
            path: dirs.config_dir().join("context.yaml"),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("dev", "azac", "azac")
}

fn default_separator() -> String {
    DEFAULT_SEPARATOR.to_string()
}
