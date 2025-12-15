use super::ContextStore;
use super::error::{ContextError, ContextResult};
use directories::ProjectDirs;
use std::{fs, path::PathBuf};

fn path() -> ContextResult<PathBuf> {
    ProjectDirs::from("com", "azac", "azac")
        .map(|dirs| dirs.config_dir().join("contexts.toml"))
        .ok_or(ContextError::MissingConfigDir)
}

pub fn load() -> ContextResult<ContextStore> {
    let path = path()?;
    let contents = fs::read_to_string(&path).unwrap_or_default();

    if contents.trim().is_empty() {
        return Ok(ContextStore::default());
    }

    Ok(toml::from_str(&contents)?)
}

pub fn write(store: &ContextStore) -> ContextResult<()> {
    let path = path()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let data = toml::to_string_pretty(store)?;
    fs::write(path, data)?;

    Ok(())
}
