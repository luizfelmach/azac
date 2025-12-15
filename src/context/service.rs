use super::store::{load, write};
use super::{Context, ContextError, ContextResult};

pub fn save(ctx: Context) -> ContextResult<()> {
    let mut store = load()?;

    if store.contexts.contains_key(&ctx.alias) {
        return Err(ContextError::DuplicateAlias(ctx.alias));
    }

    store.contexts.insert(ctx.alias.clone(), ctx);
    write(&store)
}

pub fn get(alias: &str) -> ContextResult<Context> {
    let store = load()?;

    store
        .contexts
        .get(alias)
        .cloned()
        .ok_or_else(|| ContextError::UnknownAlias(alias.to_string()))
}

pub fn set(alias: &str) -> ContextResult<()> {
    let mut store = load()?;

    if !store.contexts.contains_key(alias) {
        return Err(ContextError::UnknownAlias(alias.to_string()));
    }

    store.current = Some(alias.to_owned());
    write(&store)
}

pub fn current() -> ContextResult<Option<Context>> {
    let store = load()?;

    let Some(alias) = store.current.as_deref() else {
        return Ok(None);
    };

    let ctx = store
        .contexts
        .get(alias)
        .cloned()
        .ok_or_else(|| ContextError::CurrentContextMissing(alias.to_string()))?;

    Ok(Some(ctx))
}

pub fn list() -> ContextResult<Vec<(Context, bool)>> {
    let store = load()?;
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

pub fn update(original_alias: &str, ctx: Context) -> ContextResult<()> {
    let mut store = load()?;

    if !store.contexts.contains_key(original_alias) {
        return Err(ContextError::UnknownAlias(original_alias.to_string()));
    }

    if original_alias != ctx.alias && store.contexts.contains_key(&ctx.alias) {
        return Err(ContextError::DuplicateAlias(ctx.alias));
    }

    let was_current = store.current.as_deref() == Some(original_alias);

    let new_alias = ctx.alias.clone();
    store.contexts.remove(original_alias);
    store.contexts.insert(new_alias.clone(), ctx);

    if was_current {
        store.current = Some(new_alias);
    }

    write(&store)
}

pub fn rename(original_alias: &str, new_alias: &str) -> ContextResult<()> {
    if original_alias == new_alias {
        return Ok(());
    }

    let mut store = load()?;

    let mut ctx = store
        .contexts
        .remove(original_alias)
        .ok_or_else(|| ContextError::UnknownAlias(original_alias.to_string()))?;

    if store.contexts.contains_key(new_alias) {
        return Err(ContextError::DuplicateAlias(new_alias.to_string()));
    }

    ctx.alias = new_alias.to_owned();
    store.contexts.insert(new_alias.to_owned(), ctx);

    if store.current.as_deref() == Some(original_alias) {
        store.current = Some(new_alias.to_owned());
    }

    write(&store)
}

pub fn clone(source_alias: &str, new_alias: &str) -> ContextResult<()> {
    let mut store = load()?;

    let ctx = store
        .contexts
        .get(source_alias)
        .cloned()
        .ok_or_else(|| ContextError::UnknownAlias(source_alias.to_string()))?;

    if store.contexts.contains_key(new_alias) {
        return Err(ContextError::DuplicateAlias(new_alias.to_string()));
    }

    let mut cloned = ctx;
    cloned.alias = new_alias.to_owned();
    store.contexts.insert(new_alias.to_owned(), cloned);

    write(&store)
}

pub fn delete(alias: &str) -> ContextResult<()> {
    let mut store = load()?;

    if store.contexts.remove(alias).is_none() {
        return Err(ContextError::UnknownAlias(alias.to_string()));
    }

    if store.current.as_deref() == Some(alias) {
        store.current = None;
    }

    write(&store)
}
