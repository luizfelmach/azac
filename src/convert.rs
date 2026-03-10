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
        ConvertCommand::Dotnet { file } => {
            if let Err(err) = convert_dotnet(&file) {
                eprintln!("{}", err);
            }
        }
    }
}

fn convert_env(path: &Path) -> Result<(), ConvertError> {
    let file = File::open(path).map_err(|err| ConvertError::Io(path.to_path_buf(), err))?;

    let mut vars = BTreeMap::new();

    for item in dotenvy::from_read_iter(file) {
        let (key, value) = item.map_err(ConvertError::EnvParse)?;
        insert_or_warn(&mut vars, key, value);
    }

    let payload = to_yaml_payload(&vars);
    let output = serde_yaml::to_string(&payload).map_err(ConvertError::Serialize)?;
    print!("{}", output);
    Ok(())
}

fn convert_dotnet(path: &Path) -> Result<(), ConvertError> {
    let file = File::open(path).map_err(|err| ConvertError::Io(path.to_path_buf(), err))?;
    let json: Value =
        serde_json::from_reader(file).map_err(|err| ConvertError::Json(path.to_path_buf(), err))?;

    let vars = flatten_dotnet_json(&json, path)?;
    let payload = to_yaml_payload(&vars);
    let output = serde_yaml::to_string(&payload).map_err(ConvertError::Serialize)?;
    print!("{}", output);
    Ok(())
}

fn flatten_dotnet_json(value: &Value, path: &Path) -> Result<BTreeMap<String, String>, ConvertError> {
    let root = value
        .as_object()
        .ok_or_else(|| ConvertError::UnsupportedRoot(path.to_path_buf()))?;

    let mut vars = BTreeMap::new();
    for (key, child) in root {
        flatten_value(child, key, &mut vars);
    }
    Ok(vars)
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

fn flatten_value(value: &Value, prefix: &str, vars: &mut BTreeMap<String, String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let next = join_key(prefix, key);
                flatten_value(child, &next, vars);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                let next = join_key(prefix, &index.to_string());
                flatten_value(child, &next, vars);
            }
        }
        _ => {
            let value_str = match value {
                Value::String(s) => s.clone(),
                Value::Number(num) => num.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Null => String::new(),
                _ => value.to_string(),
            };
            insert_or_warn(vars, prefix.to_string(), value_str);
        }
    }
}

fn join_key(prefix: &str, segment: &str) -> String {
    if prefix.is_empty() {
        segment.to_string()
    } else {
        format!("{prefix}__{segment}")
    }
}

fn insert_or_warn(map: &mut BTreeMap<String, String>, key: String, value: String) {
    if map.insert(key.clone(), value).is_some() {
        eprintln!("Warning: duplicate key '{key}' found. Last value wins.");
    }
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
        /// Convert a .NET appsettings.json file into the azac YAML format
        Dotnet {
            #[arg(value_name = "FILE", value_parser = clap::value_parser!(PathBuf))]
            file: PathBuf,
        },
    }
}

#[derive(Debug)]
enum ConvertError {
    Io(PathBuf, io::Error),
    EnvParse(dotenvy::Error),
    Json(PathBuf, serde_json::Error),
    UnsupportedRoot(PathBuf),
    Serialize(serde_yaml::Error),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::Io(path, err) => {
                write!(f, "Failed to read {}: {}", path.display(), err)
            }
            ConvertError::EnvParse(err) => write!(f, "Failed to parse .env file: {err}"),
            ConvertError::Json(path, err) => {
                write!(f, "Failed to parse JSON from {}: {}", path.display(), err)
            }
            ConvertError::UnsupportedRoot(path) => write!(
                f,
                "Expected a JSON object at the root of {}, but found another type.",
                path.display()
            ),
            ConvertError::Serialize(err) => write!(f, "Failed to serialize YAML: {err}"),
        }
    }
}
