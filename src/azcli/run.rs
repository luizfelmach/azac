use super::error::{AzCliError, AzCliResult};
use serde::de::DeserializeOwned;
use std::process::{Command, Output};
use std::{ffi::OsStr, io};

fn run<I, S>(args: I) -> AzCliResult<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    match Command::new("az").args(args).output() {
        Ok(output) => Ok(output),
        Err(err) => match err.kind() {
            io::ErrorKind::NotFound => Err(AzCliError::AzNotInstalled),
            _ => Err(AzCliError::Io(err)),
        },
    }
}

fn authenticated() -> AzCliResult<bool> {
    let output = run(["account", "show", "-o", "json"])?;

    match output.status.success() {
        true => Ok(true),
        _ => Ok(false),
    }
}

fn az_raw<I, S>(args: I) -> AzCliResult<Vec<u8>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    if !authenticated()? {
        return Err(AzCliError::NotLoggedIn);
    }

    let output = run(args)?;

    if output.status.success() {
        return Ok(output.stdout);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    Err(AzCliError::CommandFailure {
        code: output.status.code(),
        stderr,
    })
}

pub fn az<T, I, S>(args: I) -> AzCliResult<T>
where
    T: DeserializeOwned,
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let stdout = az_raw(args)?;
    Ok(serde_json::from_slice(&stdout)?)
}
