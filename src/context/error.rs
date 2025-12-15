use thiserror::Error;

pub type ContextResult<T> = Result<T, ContextError>;

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("Unable to determine configuration directory")]
    MissingConfigDir,
    #[error("Alias '{0}' already exists")]
    DuplicateAlias(String),
    #[error("Alias '{0}' not found")]
    UnknownAlias(String),
    #[error("Current context '{0}' not found in store")]
    CurrentContextMissing(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serialize(#[from] toml::ser::Error),
    #[error(transparent)]
    Deserialize(#[from] toml::de::Error),
}
