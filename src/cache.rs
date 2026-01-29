use crate::azcli::subscription;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use thiserror::Error;

const CACHE_FILE: &str = "setup-cache.json";

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("Could not determine cache directory for azac")]
    MissingProjectDirs,
    #[error("Failed to read cache file: {0}")]
    Read(#[from] std::io::Error),
    #[error("Failed to serialize or deserialize cache file: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type CacheResult<T> = Result<T, CacheError>;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SetupCache {
    #[serde(default)]
    pub subscriptions: Vec<subscription::Subscription>,
    #[serde(default)]
    pub appconfigs: Vec<CachedAppConfig>,
    #[serde(default)]
    pub keyvaults: Vec<CachedKeyVault>,
    #[serde(default)]
    pub ready: bool,
}

impl SetupCache {
    pub fn load_or_default(store: &CacheStore) -> CacheResult<Self> {
        if !store.path.exists() {
            return Ok(Default::default());
        }

        let payload = fs::read_to_string(&store.path)?;
        if payload.trim().is_empty() {
            return Ok(Default::default());
        }

        Ok(serde_json::from_str(&payload)?)
    }

    pub fn save(&self, store: &CacheStore) -> CacheResult<()> {
        if let Some(parent) = store.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let payload = serde_json::to_string_pretty(self)?;
        fs::write(&store.path, payload)?;
        Ok(())
    }

    pub fn is_ready(&self) -> bool {
        self.ready
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedAppConfig {
    pub subscription_id: String,
    pub subscription_name: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedKeyVault {
    pub name: String,
    pub subscription_id: String,
}

pub struct CacheStore {
    path: PathBuf,
}

impl CacheStore {
    pub fn new() -> CacheResult<Self> {
        let dirs = project_dirs().ok_or(CacheError::MissingProjectDirs)?;
        Ok(Self {
            path: dirs.cache_dir().join(CACHE_FILE),
        })
    }
}

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("dev", "azac", "azac")
}
