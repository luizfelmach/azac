use super::error::{ErrorAzCli, ResultAzCli};
use serde::de::DeserializeOwned;
use std::process::{Command, Output};
use std::{ffi::OsStr, io};

fn run<I, S>(args: I) -> ResultAzCli<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    match Command::new("az").args(args).output() {
        Ok(output) => Ok(output),
        Err(err) => match err.kind() {
            io::ErrorKind::NotFound => Err(ErrorAzCli::AzNotInstalled),
            _ => Err(ErrorAzCli::Io(err)),
        },
    }
}

fn authenticated() -> ResultAzCli<bool> {
    let output = run(["account", "show", "-o", "json"])?;

    match output.status.success() {
        true => Ok(true),
        _ => Ok(false),
    }
}

fn az_raw<I, S>(args: I) -> ResultAzCli<Vec<u8>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    if !authenticated()? {
        return Err(ErrorAzCli::NotLoggedIn);
    }

    let output = run(args)?;

    if output.status.success() {
        return Ok(output.stdout);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    Err(ErrorAzCli::CommandFailure {
        code: output.status.code(),
        stderr,
    })
}

pub fn az<T, I, S>(args: I) -> ResultAzCli<T>
where
    T: DeserializeOwned,
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let stdout = az_raw(args)?;
    Ok(serde_json::from_slice(&stdout)?)
}
