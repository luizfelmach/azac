use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};
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
    pub active: Option<ActiveContext>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActiveContext {
    pub subscription: SubscriptionMetadata,
    pub config_name: String,
    #[serde(default = "default_separator")]
    pub separator: String,
    #[serde(default)]
    pub current_app: Option<String>,
    #[serde(default)]
    pub apps: BTreeMap<String, AppContext>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SubscriptionMetadata {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppContext {
    pub label: Option<String>,
    pub keyvault: Option<String>,
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
}

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("dev", "azac", "azac")
}

pub fn default_separator() -> String {
    DEFAULT_SEPARATOR.to_string()
}
