use std::io;
use thiserror::Error;

pub type ResultAzCli<T> = Result<T, ErrorAzCli>;

#[derive(Debug, Error)]
pub enum ErrorAzCli {
    #[error("Azure CLI (az) executable not found. Install Azure CLI to continue.")]
    AzNotInstalled,
    #[error("Azure CLI returned that you are not logged in. Run `az login`.")]
    NotLoggedIn,
    #[error("Azure CLI command failed with code {code:?}: {stderr}")]
    CommandFailure { code: Option<i32>, stderr: String },
    #[error("Failed to parse Azure CLI response: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("Failed to execute Azure CLI: {0}")]
    Io(#[from] io::Error),
}
