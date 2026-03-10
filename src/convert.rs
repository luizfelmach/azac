use clap::Subcommand;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fmt,
    fs::File,
    io,
    path::{Path, PathBuf},
};

pub use cli::ConvertCommand;

pub fn run(command: ConvertCommand) {
    match command {
        ConvertCommand::Env { file } => {
            if let Err(err) = convert_env(&file) {
                eprintln!("{}", err);
            }
        }
        ConvertCommand::Dotnet { .. } => {
            eprintln!("dotnet config conversion is not implemented yet.");
        }
    }
}

fn convert_env(path: &Path) -> Result<(), ConvertError> {
    let file = File::open(path).map_err(|err| ConvertError::Io(path.to_path_buf(), err))?;

    let mut vars = BTreeMap::new();

    for item in dotenvy::from_read_iter(file) {
        let (key, value) = item.map_err(ConvertError::Parse)?;
        if vars.insert(key.clone(), value).is_some() {
            eprintln!("Warning: duplicate key '{key}' found. Last value wins.");
        }
    }

    let payload = to_yaml_payload(&vars);
    let output = serde_yaml::to_string(&payload).map_err(ConvertError::Serialize)?;
    print!("{}", output);
    Ok(())
}

fn to_yaml_payload(vars: &BTreeMap<String, String>) -> Value {
    let mut map = serde_json::Map::new();

    for (key, value) in vars {
        let mut entry = serde_json::Map::new();
        entry.insert("type".to_string(), Value::String("plain".to_string()));
        entry.insert("value".to_string(), Value::String(value.clone()));
        map.insert(key.clone(), Value::Object(entry));
    }

    Value::Object(map)
}

mod cli {
    use super::*;

    #[derive(Subcommand)]
    pub enum ConvertCommand {
        /// Convert a dotenv-style file into the azac YAML format
        Env {
            #[arg(value_name = "FILE", value_parser = clap::value_parser!(PathBuf))]
            file: PathBuf,
        },
        /// Placeholder for future .NET configuration conversion
        #[command(hide = true)]
        Dotnet {
            #[arg(value_name = "FILE", required = false)]
            _file: Option<PathBuf>,
        },
    }
}

#[derive(Debug)]
enum ConvertError {
    Io(PathBuf, io::Error),
    Parse(dotenvy::Error),
    Serialize(serde_yaml::Error),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::Io(path, err) => {
                write!(f, "Failed to read {}: {}", path.display(), err)
            }
            ConvertError::Parse(err) => write!(f, "Failed to parse .env file: {err}"),
            ConvertError::Serialize(err) => write!(f, "Failed to serialize YAML: {err}"),
        }
    }
}

impl std::error::Error for ConvertError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_basic_env_entries() {
        let vars = BTreeMap::from([
            ("FOO".to_string(), "bar".to_string()),
            ("BAZ".to_string(), "qux".to_string()),
        ]);
        let yaml = to_yaml_payload(&vars);
        let obj = yaml.as_object().unwrap();
        assert_eq!(obj.len(), 2);
        let foo = obj.get("FOO").unwrap().as_object().unwrap();
        assert_eq!(foo.get("type").unwrap(), "plain");
        assert_eq!(foo.get("value").unwrap(), "bar");
    }
}
