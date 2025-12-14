use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf};

pub type ContextResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    pub alias: String,
    pub sub: String,
    pub name: String,
    pub base: String,
    pub separator: String,
    pub label: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ContextStore {
    current: Option<String>,
    contexts: HashMap<String, Context>,
}

fn validate_alias(alias: &str) -> ContextResult<()> {
    if alias.trim().is_empty() {
        return Err("Alias cannot be empty".into());
    }
    Ok(())
}

fn store_path() -> ContextResult<PathBuf> {
    let dirs = ProjectDirs::from("com", "azac", "azac")
        .ok_or("Unable to determine configuration directory")?;

    Ok(dirs.config_dir().join("contexts.toml"))
}

fn load_store() -> ContextResult<ContextStore> {
    let path = store_path()?;
    let contents = fs::read_to_string(&path).unwrap_or_default();

    if contents.trim().is_empty() {
        return Ok(ContextStore::default());
    }

    Ok(toml::from_str(&contents)?)
}

fn write_store(store: &ContextStore) -> ContextResult<()> {
    let path = store_path()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let data = toml::to_string_pretty(store)?;
    fs::write(path, data)?;

    Ok(())
}

pub fn save_context(ctx: Context) -> ContextResult<()> {
    validate_alias(&ctx.alias)?;
    let mut store = load_store()?;

    if store.contexts.contains_key(&ctx.alias) {
        return Err(format!("Alias '{}' already exists", ctx.alias).into());
    }

    store.contexts.insert(ctx.alias.clone(), ctx);
    write_store(&store)
}

pub fn get_context(alias: &str) -> ContextResult<Context> {
    let store = load_store()?;

    store
        .contexts
        .get(alias)
        .cloned()
        .ok_or_else(|| format!("Alias '{alias}' not found").into())
}

pub fn set_current(alias: &str) -> ContextResult<()> {
    let mut store = load_store()?;

    if !store.contexts.contains_key(alias) {
        return Err(format!("Alias '{alias}' not found").into());
    }

    store.current = Some(alias.to_owned());
    write_store(&store)
}

pub fn current_context() -> ContextResult<Option<Context>> {
    let store = load_store()?;

    let Some(alias) = store.current.as_deref() else {
        return Ok(None);
    };

    let ctx = store
        .contexts
        .get(alias)
        .cloned()
        .ok_or_else(|| format!("Current context '{alias}' not found in store"))?;

    Ok(Some(ctx))
}

pub fn list_contexts() -> ContextResult<Vec<(Context, bool)>> {
    let store = load_store()?;
    let current = store.current.as_deref();

    let mut entries: Vec<(Context, bool)> = store
        .contexts
        .values()
        .cloned()
        .map(|ctx| {
            let is_current = current.map(|c| c == ctx.alias.as_str()).unwrap_or(false);
            (ctx, is_current)
        })
        .collect();

    entries.sort_by(|a, b| a.0.alias.cmp(&b.0.alias));

    Ok(entries)
}

pub fn update_context(original_alias: &str, ctx: Context) -> ContextResult<()> {
    let mut store = load_store()?;
    validate_alias(&ctx.alias)?;

    if !store.contexts.contains_key(original_alias) {
        return Err(format!("Alias '{original_alias}' not found").into());
    }

    if original_alias != ctx.alias && store.contexts.contains_key(&ctx.alias) {
        return Err(format!("Alias '{}' already exists", ctx.alias).into());
    }

    let was_current = store.current.as_deref() == Some(original_alias);

    let new_alias = ctx.alias.clone();
    store.contexts.remove(original_alias);
    store.contexts.insert(new_alias.clone(), ctx);

    if was_current {
        store.current = Some(new_alias);
    }

    write_store(&store)
}

pub fn rename_context(original_alias: &str, new_alias: &str) -> ContextResult<()> {
    if original_alias == new_alias {
        return Ok(()); // nothing to do
    }

    validate_alias(new_alias)?;

    let mut store = load_store()?;

    let mut ctx = store
        .contexts
        .remove(original_alias)
        .ok_or_else(|| format!("Alias '{original_alias}' not found"))?;

    if store.contexts.contains_key(new_alias) {
        return Err(format!("Alias '{}' already exists", new_alias).into());
    }

    ctx.alias = new_alias.to_owned();
    store.contexts.insert(new_alias.to_owned(), ctx);

    if store.current.as_deref() == Some(original_alias) {
        store.current = Some(new_alias.to_owned());
    }

    write_store(&store)
}

pub fn clone_context(source_alias: &str, new_alias: &str) -> ContextResult<()> {
    validate_alias(new_alias)?;

    let mut store = load_store()?;

    let ctx = store
        .contexts
        .get(source_alias)
        .cloned()
        .ok_or_else(|| format!("Alias '{source_alias}' not found"))?;

    if store.contexts.contains_key(new_alias) {
        return Err(format!("Alias '{}' already exists", new_alias).into());
    }

    let mut cloned = ctx;
    cloned.alias = new_alias.to_owned();
    store.contexts.insert(new_alias.to_owned(), cloned);

    write_store(&store)
}

pub fn delete_context(alias: &str) -> ContextResult<()> {
    let mut store = load_store()?;

    if store.contexts.remove(alias).is_none() {
        return Err(format!("Alias '{alias}' not found").into());
    }

    if store.current.as_deref() == Some(alias) {
        store.current = None;
    }

    write_store(&store)
}
