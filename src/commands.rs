use crate::{
    azcli::{appconfig, error::AzCliResult, run::az, subscription},
    cache::{CacheStore, CachedAppConfig, CachedKeyVault, SetupCache},
    context::{
        default_appconfig_endpoint, default_separator, ActiveContext, AppSelection, Context,
        ContextStore, SubscriptionMetadata, DEFAULT_APP_CONFIG_ENDPOINT,
    },
};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use inquire::{InquireError, Select, Text};
use owo_colors::OwoColorize;
use serde::Deserialize;
use std::{fmt, sync::mpsc, thread, time::Duration};

pub fn setup() {
    let cache = match ensure_cache_ready() {
        Some(cache) => cache,
        None => return,
    };

    let mut options: Vec<ConfigOption> = cache
        .appconfigs
        .iter()
        .map(|entry| ConfigOption {
            subscription_id: entry.subscription_id.clone(),
            subscription_name: entry.subscription_name.clone(),
            config_name: entry.name.clone(),
            endpoint: normalize_appconfig_endpoint(&entry.name, &entry.endpoint),
        })
        .collect();

    if options.is_empty() {
        eprintln!("No App Configuration instances were found across your subscriptions.");
        return;
    }

    options.sort_by(|a, b| {
        a.config_name
            .cmp(&b.config_name)
            .then_with(|| a.subscription_name.cmp(&b.subscription_name))
    });

    let select_options: Vec<MenuItem<usize>> = options
        .iter()
        .enumerate()
        .map(|(index, option)| {
            let sub = option.subscription_name.dimmed().to_string();
            let label = format!("{} ({})", option.config_name, sub);
            MenuItem::new(index, label)
        })
        .collect();

    let selection = Select::new("App Configuration", select_options)
        .with_starting_cursor(0)
        .prompt();

    let selected = match selection {
        Ok(choice) => &options[choice.value],
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => return,
        Err(err) => {
            eprintln!("Selection failed: {err}");
            return;
        }
    };

    let default_sep = default_separator();
    let separator_input: String = match Text::new("Key Separator")
        .with_default(&default_sep)
        .prompt()
    {
        Ok(value) => value.trim().to_string(),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => return,
        Err(err) => {
            eprintln!("Separator prompt failed: {err}");
            return;
        }
    };
    let separator = if separator_input.is_empty() {
        default_sep
    } else {
        separator_input
    };

    let (store, mut context) = match load_context() {
        Some(value) => value,
        None => return,
    };

    let mut preserved_app = AppSelection::default();

    if let Some(existing) = context.active.take() {
        if existing.subscription.id == selected.subscription_id
            && existing.config_name == selected.config_name
        {
            preserved_app = existing.app;
        }
    }

    let active = ActiveContext {
        subscription: SubscriptionMetadata {
            id: selected.subscription_id.clone(),
            name: selected.subscription_name.clone(),
        },
        config_name: selected.config_name.clone(),
        endpoint: selected.endpoint.clone(),
        separator,
        app: preserved_app,
    };

    context.active = Some(active);

    if save_context(&store, &context) {
        app::select_app();
    }
}

pub fn sync() {
    let store = match CacheStore::new() {
        Ok(store) => store,
        Err(err) => {
            eprintln!("Failed to locate cache store: {err}");
            return;
        }
    };

    if refresh_cache(store, CacheRefreshKind::Manual).is_none() {
        eprintln!("Unable to refresh Azure metadata cache.");
    }
}

struct ConfigOption {
    subscription_id: String,
    subscription_name: String,
    config_name: String,
    endpoint: String,
}

#[derive(Clone)]
struct MenuItem<T> {
    value: T,
    label: String,
}

impl<T> MenuItem<T> {
    fn new(value: T, label: String) -> Self {
        Self { value, label }
    }
}

impl<T> fmt::Display for MenuItem<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label)
    }
}

pub mod app {
    use std::{collections::{BTreeMap, BTreeSet}, time::Duration};

    use inquire::{
        autocompletion::Replacement, Autocomplete, CustomUserError, InquireError, Select, Text,
        validator::Validation,
    };
    use indicatif::ProgressBar;
    use owo_colors::OwoColorize;
    use serde::Deserialize;
    use serde_json;

    use crate::{
        azcli::{error::AzCliResult, run::az, subscription},
        cache::CachedKeyVault,
    };
    use super::MenuItem;

    pub fn select_app() {
        let (store, mut context) = match super::load_context() {
            Some(value) => value,
            None => return,
        };

        let (config_name, endpoint, separator, current_app, current_label) = {
            let Some(active) = context.active.as_ref() else {
                super::missing_setup_message();
                return;
            };

            (
                active.config_name.clone(),
                super::normalize_appconfig_endpoint(&active.config_name, &active.endpoint),
                active.separator.clone(),
                active.app.name.clone(),
                active.app.label.clone(),
            )
        };
        let current_keyvault = {
            let Some(active) = context.active.as_ref() else {
                super::missing_setup_message();
                return;
            };

            (
                active.app.keyvault.clone(),
                active.app.keyvault_subscription.clone(),
            )
        };

        let cache = match super::ensure_cache_ready() {
            Some(cache) => cache,
            None => return,
        };
        let subscriptions = cache.subscriptions;
        let cached_keyvaults = cache.keyvaults;

        let spinner = ProgressBar::new_spinner();
        spinner.set_style(super::standard_spinner_style());
        spinner.enable_steady_tick(Duration::from_millis(80));
        spinner.set_message(format!(
            "Inspecting keys in '{}'...",
            config_name
        ));

        let entries = match fetch_all_keys(&endpoint) {
            Ok(entries) => entries,
            Err(err) => {
                spinner.finish_and_clear();
                eprintln!("Failed to list applications: {err}");
                return;
            }
        };

        spinner.finish_and_clear();

        let mut apps: BTreeMap<String, AppStats> = BTreeMap::new();
        for entry in entries {
            if let Some(idx) = entry.key.rfind(&separator) {
                let prefix = entry.key[..idx].to_string();
                let stats = apps.entry(prefix).or_insert_with(AppStats::default);
                if let Some(label) = entry.label.as_deref().map(str::trim) {
                    if !label.is_empty() {
                        stats.labels.insert(label.to_string());
                    }
                }
                if let Some(vault) = keyvault_name(&entry) {
                    stats.keyvaults.insert(vault);
                }
            }
        }

        if let Some(current) = current_app.as_ref() {
            let stats = apps.entry(current.clone()).or_insert_with(AppStats::default);
            if let Some(label) = current_label.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                stats.labels.insert(label.to_string());
            }
        }

        let app_names: Vec<String> = apps.keys().cloned().collect();

        let app_prompt = Text::new("Application")
        .with_autocomplete(ValueAutocomplete::new(&app_names))
        .with_validator(|value: &str| {
            if value.trim().is_empty() {
                Ok(Validation::Invalid("Application name cannot be empty".into()))
            } else {
                Ok(Validation::Valid)
            }
        });

        let selected_app_input = match app_prompt.prompt() {
            Ok(value) => value.trim().to_string(),
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => return,
            Err(err) => {
                eprintln!("Application selection failed: {err}");
                return;
            }
        };

        let selected_app = selected_app_input.clone();

        let mut all_labels: Vec<String> = apps
            .values()
            .flat_map(|stats| stats.labels.iter().cloned())
            .collect();
        if let Some(saved) = current_label.as_ref() {
            all_labels.push(saved.clone());
        }
        all_labels.sort();
        all_labels.dedup();

        let selected_label = match prompt_label_entry("Label", &all_labels) {
            Some(value) => value,
            None => return,
        };

        let selected_keyvault = match select_keyvault(
            "Key Vault",
            &subscriptions,
            &cached_keyvaults,
            current_keyvault.clone(),
        ) {
            Ok(value) => value,
            Err(err) => {
                eprintln!("Key Vault selection failed: {err}");
                return;
            }
        };

        {
            let Some(active) = context.active.as_mut() else {
                super::missing_setup_message();
                return;
            };

            active.app.name = Some(selected_app.clone());
            active.app.label = selected_label.clone();
            active.app.keyvault = selected_keyvault.as_ref().map(|kv| kv.name.clone());
            active.app.keyvault_subscription =
                selected_keyvault.as_ref().map(|kv| kv.subscription_id.clone());
        }

        if !super::save_context(&store, &context) {
            return;
        }
    }

    fn select_keyvault(
        prompt_label: &str,
        subscriptions: &[subscription::Subscription],
        keyvaults: &[CachedKeyVault],
        current: (Option<String>, Option<String>),
    ) -> AzCliResult<Option<KeyVaultSelection>> {
        if keyvaults.is_empty() {
            println!("No Key Vaults found for your account.");
            return Ok(None);
        }

        let mut vaults: Vec<KeyVaultSelection> = keyvaults
            .iter()
            .map(|kv| KeyVaultSelection {
                name: kv.name.clone(),
                subscription_id: kv.subscription_id.clone(),
            })
            .collect();

        let mut sub_map = BTreeMap::new();
        for sub in subscriptions {
            sub_map.insert(sub.id.clone(), sub.name.clone());
        }

        vaults.sort_by(|a, b| a.name.cmp(&b.name));

        let menu_items: Vec<MenuItem<KeyVaultSelection>> = vaults
            .iter()
            .map(|kv| {
                let sub_label = sub_map
                    .get(&kv.subscription_id)
                    .cloned()
                    .unwrap_or_else(|| kv.subscription_id.clone());
                let label = format!("{} ({})", kv.name, sub_label.dimmed());
                MenuItem::new(kv.clone(), label)
            })
            .collect();

        let default_index = match (&current.0, &current.1) {
            (Some(name), Some(sub_id)) => vaults
                .iter()
                .position(|kv| kv.name == *name && kv.subscription_id == *sub_id),
            (Some(name), None) => vaults.iter().position(|kv| kv.name == *name),
            _ => None,
        }
        .unwrap_or(0);

        let len = menu_items.len();
        let selection = Select::new(prompt_label, menu_items)
        .with_starting_cursor(default_index.min(len.saturating_sub(1)))
        .prompt();

        match selection {
            Ok(choice) => Ok(Some(choice.value)),
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => Ok(None),
            Err(err) => Err(crate::azcli::error::AzCliError::CommandFailure {
                code: None,
                stderr: format!("{err}"),
            }),
        }
    }

    fn prompt_label_entry(prompt_label: &str, existing_labels: &[String]) -> Option<Option<String>> {
        let input = Text::new(prompt_label)
            .with_autocomplete(ValueAutocomplete::new(existing_labels))
            .prompt();

        match input {
            Ok(value) => {
                let trimmed = value.trim().to_string();
                if trimmed.is_empty() {
                    Some(None)
                } else {
                    Some(Some(trimmed))
                }
            }
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                println!("Label input aborted.");
                None
            }
            Err(err) => {
                eprintln!("Label input failed: {err}");
                None
            }
        }
    }

    #[derive(Clone)]
    struct ValueAutocomplete {
        suggestions: Vec<String>,
    }

    impl ValueAutocomplete {
        fn new(existing_apps: &[String]) -> Self {
            Self {
                suggestions: existing_apps.to_vec(),
            }
        }
    }

    impl Autocomplete for ValueAutocomplete {
        fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
            let trimmed = input.trim();
            if trimmed.is_empty() {
                return Ok(self.suggestions.clone());
            }

            let needle = trimmed.to_ascii_lowercase();

            Ok(self
                .suggestions
                .iter()
                .filter(|candidate| candidate.to_ascii_lowercase().contains(&needle))
                .cloned()
                .collect())
        }

        fn get_completion(
            &mut self,
            input: &str,
            highlighted_suggestion: Option<String>,
        ) -> Result<Replacement, CustomUserError> {
            if highlighted_suggestion.is_some() {
                return Ok(highlighted_suggestion);
            }

            let trimmed = input.trim();
            if trimmed.is_empty() {
                return Ok(self.suggestions.first().cloned());
            }

            let needle = trimmed.to_ascii_lowercase();

            if let Some(value) = self
                .suggestions
                .iter()
                .find(|candidate| candidate.to_ascii_lowercase().starts_with(&needle))
            {
                return Ok(Some(value.clone()));
            }

            Ok(self
                .suggestions
                .iter()
                .find(|candidate| candidate.to_ascii_lowercase().contains(&needle))
                .cloned())
        }
    }

    #[derive(Debug, Deserialize)]
    struct KeyValue {
        key: String,
        label: Option<String>,
        #[serde(rename = "contentType")]
        content_type: Option<String>,
        value: Option<String>,
    }

    #[derive(Clone, Debug)]
    struct KeyVaultSelection {
        name: String,
        subscription_id: String,
    }

    fn fetch_all_keys(endpoint: &str) -> AzCliResult<Vec<KeyValue>> {
        az([
            "appconfig",
            "kv",
            "list",
            "--endpoint",
            endpoint,
            "--auth-mode",
            "login",
            "--all",
            "-o",
            "json",
        ])
    }

    #[derive(Default)]
    struct AppStats {
        labels: BTreeSet<String>,
        keyvaults: BTreeSet<String>,
    }

    fn keyvault_name(entry: &KeyValue) -> Option<String> {
        if let Some(value) = entry.value.as_deref() {
            if let Some(uri) = parse_keyvault_reference(value).or_else(|| parse_keyvault_json(value))
            {
                return vault_name_from_uri(&uri);
            }
        }

        if entry
            .content_type
            .as_deref()
            .map(|ct| ct.contains("keyvaultref"))
            .unwrap_or(false)
        {
            if let Some(value) = entry.value.as_deref() {
                if let Some(uri) = parse_keyvault_json(value) {
                    return vault_name_from_uri(&uri);
                }
            }
        }

        None
    }

    fn parse_keyvault_reference(value: &str) -> Option<String> {
        const PREFIX: &str = "@Microsoft.KeyVault(";
        const SUFFIX: &str = ")";

        if !value.starts_with(PREFIX) || !value.ends_with(SUFFIX) {
            return None;
        }

        let inner = &value[PREFIX.len()..value.len() - SUFFIX.len()];
        let trimmed = inner.trim();
        if trimmed.is_empty() {
            return None;
        }

        let mut secret_uri = None;
        let mut vault_name = None;
        let mut secret_name = None;
        let mut secret_version = None;

        for part in trimmed.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            let (key, value) = match part.split_once('=') {
                Some((k, v)) => (k.trim().to_ascii_lowercase(), v.trim()),
                None => continue,
            };

            if value.is_empty() {
                continue;
            }

            match key.as_str() {
                "secreturi" => secret_uri = Some(value.to_string()),
                "vaultname" => vault_name = Some(value.to_string()),
                "secretname" => secret_name = Some(value.to_string()),
                "secretversion" => secret_version = Some(value.to_string()),
                _ => {}
            }
        }

        if let Some(uri) = secret_uri {
            return Some(uri);
        }

        match (vault_name, secret_name) {
            (Some(vault), Some(secret)) => {
                let base = if vault.starts_with("http://") || vault.starts_with("https://") {
                    vault.trim_end_matches('/').to_string()
                } else {
                    format!("https://{}.vault.azure.net", vault.trim_end_matches('/'))
                };

                let mut uri = format!("{}/secrets/{}", base, secret.trim_matches('/'));
                if let Some(version) = secret_version.filter(|v| !v.trim().is_empty()) {
                    uri.push('/');
                    uri.push_str(version.trim_matches('/'));
                }

                Some(uri)
            }
            _ => None,
        }
    }

    fn parse_keyvault_json(value: &str) -> Option<String> {
        let json: serde_json::Value = serde_json::from_str(value).ok()?;
        json.get("uri")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                json.get("secretUri")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
    }

    fn vault_name_from_uri(uri: &str) -> Option<String> {
        let without_scheme = uri.splitn(2, "://").nth(1)?;
        let host = without_scheme.split('/').next()?.trim();
        if host.is_empty() {
            return None;
        }

        let name = host.split('.').next().unwrap_or(host);
        if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        }
    }
}

pub mod kv {
    use std::{
        collections::VecDeque,
        fs,
        path::Path,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
        thread,
        time::Duration,
    };

    use clap::ValueEnum;
    use inquire::{InquireError, Select};
    use heck::ToUpperCamelCase;
    use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
    use owo_colors::OwoColorize;
    use serde::Deserialize;

    use crate::azcli::{
        error::{AzCliError, AzCliResult},
        run::az,
    };
    use super::MenuItem;

    #[derive(Clone, Copy, Debug, ValueEnum)]
    pub enum ExportFormat {
        Json,
        Yaml,
        Toml,
        Env,
    }

    #[derive(Debug, Deserialize)]
    struct KeyValue {
        key: String,
        value: Option<String>,
        #[serde(rename = "contentType")]
        content_type: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    struct SecretValue {
        value: String,
    }

    #[derive(Debug)]
    struct ActiveKvContext {
        config_name: String,
        endpoint: String,
        separator: String,
        app_name: Option<String>,
        label: Option<String>,
        keyvault: Option<String>,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum EntryValueType {
        Plain,
        KeyVault,
        Prompt,
    }

    #[derive(Debug)]
    struct PreparedExportEntry {
        key: String,
        value: String,
        from_keyvault: bool,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ImportFormat {
        Json,
        Yaml,
        Toml,
        Env,
    }

    fn create_spinner(initial_message: &str) -> ProgressBar {
        let spinner = ProgressBar::new_spinner();
        let style = ProgressStyle::with_template("{spinner:.green} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner());
        spinner.set_style(style);
        spinner.enable_steady_tick(Duration::from_millis(80));
        spinner.set_message(initial_message.to_string());
        spinner
    }

    fn truncate_value(value: &str, limit: usize) -> String {
        if value.chars().count() <= limit {
            return value.to_string();
        }

        let keep = limit.saturating_sub(3);
        let prefix: String = value.chars().take(keep).collect();
        format!("{prefix}...")
    }

    fn format_key_line(name: &str, preview: &str, is_secret: bool, is_empty: bool) -> String {
        let mut left = format!("{}", name.bold().bright_white());
        if is_secret {
            left.push(' ');
            left.push_str(&format!("{}", "[keyvault]".yellow()));
        }

        let quoted = format!("\"{preview}\"");
        let styled_preview = if is_empty {
            format!("{}", quoted.dimmed())
        } else if is_secret {
            format!("{}", quoted.yellow())
        } else {
            format!("{}", quoted.cyan())
        };

        format!("{left}: {styled_preview}")
    }

    pub fn list_keys() {
        let spinner = create_spinner("Resolving configuration context...");
        let ctx = match resolve_active_context(true, false) {
            Some(ctx) => ctx,
            None => {
                spinner.finish_and_clear();
                return;
            }
        };

        spinner.set_message("Fetching configuration entries...");
        let entries = match fetch_entries(&ctx) {
            Ok(entries) => entries,
            Err(err) => {
                spinner.finish_and_clear();
                eprintln!("Failed to list keys: {err}");
                return;
            }
        };

        spinner.finish_with_message("Entries fetched.");

        if entries.is_empty() {
            let app = ctx.app_name.as_deref().unwrap_or("(none)");
            let label = ctx.label.as_deref().unwrap_or("(none)");
            println!(
                "No keys found (label: {}, app: {}).",
                label, app
            );
            return;
        }

        for entry in entries {
            let key = strip_prefix(&ctx, &entry.key);
            let (value, from_keyvault) = resolve_value(&entry, false, false);

            let detail = if from_keyvault {
                keyvault_uri_from_entry(&entry)
                    .map(|uri| truncate_value(&uri, 80))
                    .unwrap_or_else(|| "[key vault reference]".to_string())
            } else if value.is_empty() {
                "(empty)".to_string()
            } else {
                truncate_value(&value, 80)
            };

            let line = format_key_line(&key, &detail, from_keyvault, detail == "(empty)");
            println!("{line}");
        }
    }

    pub fn show_key(key: &str) {
        let spinner = create_spinner("Resolving configuration context...");
        let ctx = match resolve_active_context(true, false) {
            Some(ctx) => ctx,
            None => {
                spinner.finish_and_clear();
                return;
            }
        };

        spinner.set_message(format!("Fetching '{}'...", key));
        let full_key = prefix_key(&ctx, key);
        let entry = match show_entry(&ctx, &full_key) {
            Ok(entry) => entry,
            Err(err) => {
                spinner.finish_and_clear();
                eprintln!("Failed to fetch key: {err}");
                return;
            }
        };
        spinner.finish_with_message(format!("Fetched '{}'.", key));

        let display_key = strip_prefix(&ctx, &entry.key);
        let (value, from_keyvault) = resolve_value(&entry, true, true);
        let keyvault_uri = keyvault_uri_from_entry(&entry);

        let detail = if value.is_empty() {
            "(empty)".to_string()
        } else {
            truncate_value(&value, 120)
        };

        let line = format_key_line(&display_key, &detail, from_keyvault, detail == "(empty)");
        println!("{line}");
        if from_keyvault {
            if let Some(secret_uri) = keyvault_uri {
                println!(
                    "{}",
                    format!("  ↳ {}", truncate_value(&secret_uri, 120)).dimmed()
                );
            }
        }
    }

    pub fn set_key(key: &str, value: &str, use_keyvault: bool) {
        let Some(ctx) = resolve_active_context(true, false) else {
            return;
        };

        let full_key = prefix_key(&ctx, key);

        let existing_entry = show_entry(&ctx, &full_key).ok();
        if let Some(entry) = existing_entry.as_ref() {
            // If the stored value is a Key Vault reference, update the secret directly.
            if let Some(secret_uri) = keyvault_uri_from_entry(entry) {
                match set_secret_value(&secret_uri, value) {
                    Ok(_) => {
                        let label_display = ctx.label.as_deref().unwrap_or("(none)");
                        println!(
                            "Updated Key Vault secret for key '{}' in App Configuration '{}' (label: {}).",
                            key, ctx.config_name, label_display
                        );
                    }
                    Err(err) => eprintln!("Failed to update Key Vault secret for '{}': {err}", key),
                }
                return;
            }
        }

        let write_result = if use_keyvault {
            match build_keyvault_reference(&ctx, &full_key, value) {
                Some(secret_uri) => write_keyvault_entry(&ctx, &full_key, &secret_uri),
                None => return,
            }
        } else {
            write_entry(&ctx, &full_key, value, None)
        };

        match write_result {
            Ok(_) => {
                let label_display = ctx.label.as_deref().unwrap_or("(none)");
                println!(
                    "Set key '{}' in App Configuration '{}' (label: {}).",
                    key, ctx.config_name, label_display
                );
            }
            Err(err) => eprintln!("Failed to set key: {err}"),
        }
    }

    pub fn promote_key(key: &str) {
        let Some(ctx) = resolve_active_context(true, false) else {
            return;
        };

        let full_key = prefix_key(&ctx, key);
        let entry = match show_entry(&ctx, &full_key) {
            Ok(entry) => entry,
            Err(err) => {
                eprintln!("Failed to fetch key '{}': {err}", key);
                return;
            }
        };

        if keyvault_uri_from_entry(&entry).is_some() {
            println!("Key '{}' is already stored as a Key Vault reference.", key);
            return;
        }

        let Some(value) = entry.value.as_deref() else {
            eprintln!("Key '{}' has no value to promote.", key);
            return;
        };

        let secret_uri = match build_keyvault_reference(&ctx, &full_key, value) {
            Some(uri) => uri,
            None => return,
        };

        match write_keyvault_entry(&ctx, &full_key, &secret_uri) {
            Ok(_) => {
                let label_display = ctx.label.as_deref().unwrap_or("(none)");
                println!(
                    "Promoted key '{}' in App Configuration '{}' (label: {}) to Key Vault.",
                    key, ctx.config_name, label_display
                );
            }
            Err(err) => eprintln!("Failed to promote key '{}': {err}", key),
        }
    }

    pub fn demote_key(key: &str) {
        let Some(ctx) = resolve_active_context(true, false) else {
            return;
        };

        let full_key = prefix_key(&ctx, key);
        let entry = match show_entry(&ctx, &full_key) {
            Ok(entry) => entry,
            Err(err) => {
                eprintln!("Failed to fetch key '{}': {err}", key);
                return;
            }
        };

        let Some(secret_uri) = keyvault_uri_from_entry(&entry) else {
            println!("Key '{}' is already stored as a plain value.", key);
            return;
        };

        let secret_value = match fetch_secret_value(&secret_uri) {
            Ok(value) => value,
            Err(err) => {
                eprintln!("Failed to fetch Key Vault secret for '{}': {}", key, err);
                return;
            }
        };

        // Clear content type so we drop the Key Vault reference type.
        match write_entry(&ctx, &full_key, &secret_value, Some("")) {
            Ok(_) => {
                let label_display = ctx.label.as_deref().unwrap_or("(none)");
                println!(
                    "Demoted key '{}' in App Configuration '{}' (label: {}) to a plain value. Key Vault secret was left untouched.",
                    key, ctx.config_name, label_display
                );
            }
            Err(err) => eprintln!("Failed to demote key '{}': {err}", key),
        }
    }

    pub fn delete_keys(keys: &[String]) {
        let Some(ctx) = resolve_active_context(true, false) else {
            return;
        };

        let mut deleted = 0usize;

        for key in keys {
            let full_key = prefix_key(&ctx, key);

            match delete_entry(&ctx, &full_key) {
                Ok(_) => {
                    deleted += 1;
                    println!("Deleted key '{}' from '{}'.", key, ctx.config_name);
                }
                Err(err) => eprintln!("Failed to delete key '{}': {err}", key),
            }
        }

        if deleted > 1 {
            println!("Deleted {} keys from '{}'.", deleted, ctx.config_name);
        }
    }

    pub fn export_entries(format: Option<ExportFormat>, file: &Path) {
        let format = format.unwrap_or(ExportFormat::Toml);
        let Some(ctx) = resolve_active_context(true, false) else {
            return;
        };

        let spinner = ProgressBar::new_spinner();
        if let Ok(style) = ProgressStyle::with_template("{spinner:.green} {msg}") {
            spinner.set_style(style);
        }
        spinner.enable_steady_tick(Duration::from_millis(80));
        spinner.set_message("Fetching configuration entries...");

        let entries = match fetch_entries(&ctx) {
            Ok(entries) => entries,
            Err(err) => {
                eprintln!("Failed to export entries: {err}");
                return;
            }
        };

        spinner.set_message("Preparing export payload...");

        let mut prepared_entries = Vec::new();
        let mut total = 0usize;
        let mut keyvault_count = 0usize;
        let mut plain_count = 0usize;

        for entry in entries {
            let key = strip_prefix(&ctx, &entry.key);
            let (value, from_keyvault) = resolve_value(&entry, true, false);
            prepared_entries.push(PreparedExportEntry {
                key,
                value,
                from_keyvault,
            });

            total += 1;
            if from_keyvault {
                keyvault_count += 1;
            } else {
                plain_count += 1;
            }
        }

        spinner.finish_with_message(format!(
            "Prepared {} entries (plain {}, keyvault {}).",
            total, plain_count, keyvault_count
        ));

        let serialized = match format {
            ExportFormat::Json => serde_json::to_string_pretty(&build_export_payload(&prepared_entries))
                .map_err(|err| format!("Failed to serialize JSON: {err}")),
            ExportFormat::Yaml => serde_yaml::to_string(&build_export_payload(&prepared_entries))
                .map_err(|err| format!("Failed to serialize YAML: {err}")),
            ExportFormat::Toml => toml::to_string_pretty(&build_export_payload(&prepared_entries))
                .map_err(|err| format!("Failed to serialize TOML: {err}")),
            ExportFormat::Env => Ok(serialize_env_entries(&prepared_entries)),
        };

        let data = match serialized {
            Ok(data) => data,
            Err(err) => {
                eprintln!("{err}");
                return;
            }
        };

        if let Err(err) = fs::write(file, data.as_bytes()) {
            eprintln!("Failed to write {}: {}", file.display(), err);
            return;
        }

        if total == 0 {
            println!(
                "No keys found for App Configuration '{}' (label: {}).",
                ctx.config_name,
                ctx.label.as_deref().unwrap_or("(none)")
            );
        } else {
            println!(
                "Exported {} entries (plain {}, keyvault {}) as {:?} → '{}'.",
                total, plain_count, keyvault_count, format, file.display()
            );
        }
    }

    fn build_export_payload(entries: &[PreparedExportEntry]) -> serde_json::Value {
        let mut map = serde_json::Map::new();

        for entry in entries {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "type".to_string(),
                serde_json::Value::String(if entry.from_keyvault {
                    "keyvault".to_string()
                } else {
                    "plain".to_string()
                }),
            );
            obj.insert(
                "value".to_string(),
                serde_json::Value::String(entry.value.clone()),
            );
            map.insert(entry.key.clone(), serde_json::Value::Object(obj));
        }

        serde_json::Value::Object(map)
    }

    fn serialize_env_entries(entries: &[PreparedExportEntry]) -> String {
        let mut lines = Vec::with_capacity(entries.len());

        for entry in entries {
            lines.push(format!(
                "{}={}",
                entry.key,
                encode_env_value(&entry.value)
            ));
        }

        let mut data = lines.join("\n");
        if !data.is_empty() {
            data.push('\n');
        }
        data
    }

    fn encode_env_value(value: &str) -> String {
        if !needs_env_quotes(value) {
            return value.to_string();
        }

        format!("\"{}\"", escape_double_quoted(value))
    }

    fn needs_env_quotes(value: &str) -> bool {
        if value.is_empty() {
            return true;
        }

        !value.chars().all(|ch| matches!(
            ch,
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '.' | '-' | '/' | ':'
        ))
    }

    pub fn import_entries(path: &Path) {
        let Some(ctx) = resolve_active_context(true, false) else {
            return;
        };

        let Some(entries) = parse_import_map(path) else {
            return;
        };

        let mut prepared_entries = Vec::new();
        let mut skipped = 0usize;

        for mut entry in entries {
            if entry.value_type == EntryValueType::Prompt {
                match prompt_value_type(&entry.key) {
                    Some(kind) => entry.value_type = kind,
                    None => {
                        println!("Skipping '{}' as requested.", entry.key);
                        skipped += 1;
                        continue;
                    }
                }
            }

            prepared_entries.push(entry);
        }

        if prepared_entries.is_empty() {
            if skipped > 0 {
                println!(
                    "No entries to import; skipped {} {} during prompting.",
                    skipped,
                    if skipped == 1 { "entry" } else { "entries" }
                );
            } else {
                println!("No entries to import.");
            }
            return;
        }

        let total = prepared_entries.len();
        let config_name = ctx.config_name.clone();
        let ctx = Arc::new(ctx);
        let queue = Arc::new(Mutex::new(VecDeque::from(prepared_entries)));
        let successes = Arc::new(AtomicUsize::new(0));
        let failures = Arc::new(AtomicUsize::new(0));

        let multi = MultiProgress::new();
        let summary = multi.add(ProgressBar::new(total as u64));
        if let Ok(style) = ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] {wide_bar:.cyan/blue} {pos}/{len} {msg}",
        ) {
            summary.set_style(style);
        }
        summary.set_message("Import summary");
        summary.enable_steady_tick(Duration::from_millis(80));

        let entry_style = ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner());
        let entry_style = Arc::new(entry_style);

        let mut worker_count = thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(4);
        worker_count = worker_count.min(total).max(1);

        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let ctx = Arc::clone(&ctx);
            let queue = Arc::clone(&queue);
            let success_counter = Arc::clone(&successes);
            let failure_counter = Arc::clone(&failures);
            let summary_bar = summary.clone();
            let mp = multi.clone();
            let entry_style = Arc::clone(&entry_style);

            handles.push(thread::spawn(move || loop {
                let entry = {
                    let mut guard = queue.lock().expect("import queue poisoned");
                    guard.pop_front()
                };

                let Some(entry) = entry else {
                    break;
                };

                let spinner = mp.add(ProgressBar::new_spinner());
                spinner.set_style(entry_style.as_ref().clone());
                spinner.set_message(format!(
                    "{} {}",
                    entry.key,
                    entry.value_type.label()
                ));
                spinner.enable_steady_tick(Duration::from_millis(80));

                if process_import_entry(ctx.as_ref(), &entry) {
                    success_counter.fetch_add(1, Ordering::Relaxed);
                    spinner.finish_with_message(format!(
                        "✔ {} {}",
                        entry.key,
                        entry.value_type.label()
                    ));
                } else {
                    failure_counter.fetch_add(1, Ordering::Relaxed);
                    spinner.finish_with_message(format!(
                        "✖ {} (see logs)",
                        entry.key
                    ));
                }

                summary_bar.inc(1);
            }));
        }

        for handle in handles {
            if let Err(err) = handle.join() {
                eprintln!("An import worker thread panicked: {err:?}");
            }
        }

        summary.finish_with_message(format!(
            "Imported {} of {} entries into '{}'.",
            successes.load(Ordering::Relaxed),
            total,
            config_name
        ));
        if skipped > 0 {
            println!(
                "Skipped {} {} during prompting.",
                skipped,
                if skipped == 1 { "entry" } else { "entries" }
            );
        }

        let failed = failures.load(Ordering::Relaxed);
        if failed > 0 {
            eprintln!(
                "Failed to import {} {}. See logs above for details.",
                failed,
                if failed == 1 { "entry" } else { "entries" }
            );
        }
    }

    fn resolve_active_context(require_app: bool, require_label: bool) -> Option<ActiveKvContext> {
        let (_, context) = match super::load_context() {
            Some(value) => value,
            None => return None,
        };

        let Some(active) = context.active.as_ref() else {
            super::missing_setup_message();
            return None;
        };

        let app_name = active.app.name.clone();

        if require_app && app_name.is_none() {
            eprintln!("No application selected. Run `azac setup` to pick one.");
            return None;
        }

        let label = active
            .app
            .label
            .clone()
            .filter(|lbl| !lbl.is_empty());
        if require_label && label.is_none() {
            eprintln!("No label configured for the current application.");
            return None;
        }

        let keyvault = active
            .app
            .keyvault
            .clone()
            .filter(|kv| !kv.is_empty());

        Some(ActiveKvContext {
            config_name: active.config_name.clone(),
            endpoint: super::normalize_appconfig_endpoint(&active.config_name, &active.endpoint),
            separator: active.separator.clone(),
            app_name,
            label,
            keyvault,
        })
    }

    fn fetch_entries(ctx: &ActiveKvContext) -> AzCliResult<Vec<KeyValue>> {
        let mut args = vec![
            "appconfig".to_string(),
            "kv".to_string(),
            "list".to_string(),
            "--endpoint".to_string(),
            ctx.endpoint.clone(),
            "--auth-mode".to_string(),
            "login".to_string(),
            "--all".to_string(),
            "-o".to_string(),
            "json".to_string(),
        ];

        if let Some(label) = &ctx.label {
            args.push("--label".to_string());
            args.push(label.clone());
        }

        if let Some(app) = &ctx.app_name {
            let filter = format!("{}{}*", app, ctx.separator);
            args.push("--key".to_string());
            args.push(filter);
        }

        az(args)
    }

    fn show_entry(ctx: &ActiveKvContext, full_key: &str) -> AzCliResult<KeyValue> {
        let mut args = vec![
            "appconfig".to_string(),
            "kv".to_string(),
            "show".to_string(),
            "--endpoint".to_string(),
            ctx.endpoint.clone(),
            "--auth-mode".to_string(),
            "login".to_string(),
            "--key".to_string(),
            full_key.to_string(),
            "-o".to_string(),
            "json".to_string(),
        ];

        if let Some(label) = &ctx.label {
            args.push("--label".to_string());
            args.push(label.clone());
        }

        az(args)
    }

    fn write_entry(
        ctx: &ActiveKvContext,
        full_key: &str,
        value: &str,
        content_type: Option<&str>,
    ) -> AzCliResult<KeyValue> {
        let mut args = vec![
            "appconfig".to_string(),
            "kv".to_string(),
            "set".to_string(),
            "--endpoint".to_string(),
            ctx.endpoint.clone(),
            "--auth-mode".to_string(),
            "login".to_string(),
            "--key".to_string(),
            full_key.to_string(),
            "--value".to_string(),
            value.to_string(),
            "--yes".to_string(),
            "-o".to_string(),
            "json".to_string(),
        ];

        if let Some(ct) = content_type {
            args.push("--content-type".to_string());
            args.push(ct.to_string());
        }

        if let Some(label) = &ctx.label {
            args.push("--label".to_string());
            args.push(label.clone());
        }

        az(args)
    }

    fn write_keyvault_entry(
        ctx: &ActiveKvContext,
        full_key: &str,
        secret_uri: &str,
    ) -> AzCliResult<KeyValue> {
        let mut args = vec![
            "appconfig".to_string(),
            "kv".to_string(),
            "set-keyvault".to_string(),
            "--endpoint".to_string(),
            ctx.endpoint.clone(),
            "--auth-mode".to_string(),
            "login".to_string(),
            "--key".to_string(),
            full_key.to_string(),
            "--secret-identifier".to_string(),
            secret_uri.to_string(),
            "--yes".to_string(),
            "-o".to_string(),
            "json".to_string(),
        ];

        if let Some(label) = &ctx.label {
            args.push("--label".to_string());
            args.push(label.clone());
        }

        az(args)
    }

    fn delete_entry(ctx: &ActiveKvContext, full_key: &str) -> AzCliResult<()> {
        let mut args = vec![
            "appconfig".to_string(),
            "kv".to_string(),
            "delete".to_string(),
            "--endpoint".to_string(),
            ctx.endpoint.clone(),
            "--auth-mode".to_string(),
            "login".to_string(),
            "--key".to_string(),
            full_key.to_string(),
            "--yes".to_string(),
            "-o".to_string(),
            "json".to_string(),
        ];

        if let Some(label) = &ctx.label {
            args.push("--label".to_string());
            args.push(label.clone());
        }

        let _: serde_json::Value = az(args)?;
        Ok(())
    }

    fn resolve_value(entry: &KeyValue, fetch_secret: bool, show_activity: bool) -> (String, bool) {
        if let Some(uri) = keyvault_uri_from_entry(entry) {
            if fetch_secret {
                let spinner = show_activity.then(|| create_spinner("Fetching Key Vault secret..."));

                let result = fetch_secret_value(&uri);

                if let Some(spinner) = spinner {
                    match &result {
                        Ok(_) => spinner.finish_with_message("Key Vault secret fetched."),
                        Err(_) => spinner.finish_and_clear(),
                    }
                }

                match result {
                    Ok(secret) => return (secret, true),
                    Err(err) => {
                        eprintln!("Failed to resolve Key Vault secret {}: {}", uri, err);
                        return (uri, true);
                    }
                }
            }
            return (uri, true);
        }

        let Some(value) = entry.value.as_deref() else {
            return (String::new(), false);
        };

        (value.to_string(), false)
    }

    fn keyvault_uri_from_entry(entry: &KeyValue) -> Option<String> {
        if let Some(value) = entry.value.as_deref() {
            if let Some(uri) = parse_keyvault_reference(value) {
                return Some(uri);
            }
            if let Some(uri) = parse_keyvault_json(value) {
                return Some(uri);
            }
        }

        if entry
            .content_type
            .as_deref()
            .map(|ct| ct.contains("keyvaultref"))
            .unwrap_or(false)
        {
            if let Some(value) = entry.value.as_deref() {
                if let Some(uri) = parse_keyvault_json(value) {
                    return Some(uri);
                }
            }
        }

        None
    }

    fn parse_keyvault_reference(value: &str) -> Option<String> {
        const PREFIX: &str = "@Microsoft.KeyVault(SecretUri=";
        const SUFFIX: &str = ")";

        if value.starts_with(PREFIX) && value.ends_with(SUFFIX) {
            let inner = &value[PREFIX.len()..value.len() - SUFFIX.len()];
            if inner.is_empty() {
                None
            } else {
                Some(inner.to_string())
            }
        } else {
            None
        }
    }

    fn parse_keyvault_json(value: &str) -> Option<String> {
        let json: serde_json::Value = serde_json::from_str(value).ok()?;
        json.get("uri")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                json.get("secretUri")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
    }

    fn fetch_secret_value(uri: &str) -> AzCliResult<String> {
        let secret: SecretValue = az(["keyvault", "secret", "show", "--id", uri, "-o", "json"])?;
        Ok(secret.value)
    }

    fn set_secret_value(uri: &str, value: &str) -> AzCliResult<()> {
        let (vault_name, secret_name) = parse_secret_uri(uri).ok_or_else(|| AzCliError::CommandFailure {
            code: None,
            stderr: format!("Invalid Key Vault secret URI: {uri}"),
        })?;

        let _: serde_json::Value = az([
            "keyvault",
            "secret",
            "set",
            "--vault-name",
            &vault_name,
            "--name",
            &secret_name,
            "--value",
            value,
            "-o",
            "json",
        ])?;
        Ok(())
    }

    fn prefix_key(ctx: &ActiveKvContext, key: &str) -> String {
        match ctx.app_name.as_deref() {
            Some(app) => format!("{}{}{}", app, ctx.separator, key),
            None => key.to_string(),
        }
    }

    fn strip_prefix(ctx: &ActiveKvContext, key: &str) -> String {
        match ctx.app_name.as_deref() {
            Some(app) => {
                let prefix = format!("{}{}", app, ctx.separator);
                key.strip_prefix(&prefix).unwrap_or(key).to_string()
            }
            None => key.to_string(),
        }
    }

    #[derive(Debug)]
    struct ImportEntry {
        key: String,
        value: String,
        value_type: EntryValueType,
    }

    fn parse_import_map(path: &Path) -> Option<Vec<ImportEntry>> {
        let contents = match fs::read_to_string(path) {
            Ok(data) => data,
            Err(err) => {
                eprintln!("Failed to read {}: {err}", path.display());
                return None;
            }
        };

        if contents.trim().is_empty() {
            eprintln!("Import file {} is empty.", path.display());
            return None;
        }

        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");

        let mut errors = Vec::new();
        for format in format_detection_order(&ext, file_name) {
            match parse_with_format(format, &contents) {
                Ok(entries) if entries.is_empty() => {
                    eprintln!("No entries found in {}.", path.display());
                    return None;
                }
                Ok(entries) => return Some(entries),
                Err(err) => errors.push((format, err)),
            }
        }

        if let Some((format, err)) = errors.last() {
            eprintln!(
                "Failed to parse {} as {}: {}",
                path.display(),
                format.label(),
                err
            );
        } else {
            eprintln!(
                "Failed to parse {} as JSON, YAML, TOML, or env.",
                path.display()
            );
        }
        None
    }

    fn format_detection_order(ext: &str, file_name: &str) -> Vec<ImportFormat> {
        let mut order = Vec::new();
        let mut push_unique = |fmt| {
            if !order.contains(&fmt) {
                order.push(fmt);
            }
        };

        match ext {
            "json" => push_unique(ImportFormat::Json),
            "yaml" | "yml" => push_unique(ImportFormat::Yaml),
            "toml" => push_unique(ImportFormat::Toml),
            "env" => push_unique(ImportFormat::Env),
            _ => {}
        }

        if is_env_like(file_name) {
            push_unique(ImportFormat::Env);
        }

        push_unique(ImportFormat::Json);
        push_unique(ImportFormat::Yaml);
        push_unique(ImportFormat::Toml);
        push_unique(ImportFormat::Env);

        order
    }

    fn is_env_like(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        let lowered = name.to_ascii_lowercase();
        lowered == "env"
            || lowered == ".env"
            || lowered.ends_with(".env")
            || lowered.contains(".env.")
    }

    fn parse_with_format(format: ImportFormat, contents: &str) -> Result<Vec<ImportEntry>, String> {
        match format {
            ImportFormat::Json => parse_json_entries(contents),
            ImportFormat::Yaml => parse_yaml_entries(contents),
            ImportFormat::Toml => parse_toml_entries(contents),
            ImportFormat::Env => parse_env_entries(contents),
        }
    }

    fn parse_json_entries(contents: &str) -> Result<Vec<ImportEntry>, String> {
        let value: serde_json::Value =
            serde_json::from_str(contents).map_err(|err| err.to_string())?;
        entries_from_json_value(value)
    }

    fn parse_yaml_entries(contents: &str) -> Result<Vec<ImportEntry>, String> {
        let value: serde_json::Value =
            serde_yaml::from_str(contents).map_err(|err| err.to_string())?;
        entries_from_json_value(value)
    }

    fn parse_toml_entries(contents: &str) -> Result<Vec<ImportEntry>, String> {
        let value: toml::Value = toml::from_str(contents).map_err(|err| err.to_string())?;
        let json_value =
            serde_json::to_value(value).map_err(|err| format!("TOML conversion failed: {err}"))?;
        entries_from_json_value(json_value)
    }

    fn parse_env_entries(contents: &str) -> Result<Vec<ImportEntry>, String> {
        let mut entries = Vec::new();
        for (idx, raw_line) in contents.lines().enumerate() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let line = trimmed
                .strip_prefix("export ")
                .unwrap_or(trimmed)
                .trim();

            let Some(eq_idx) = line.find('=') else {
                eprintln!(
                    "Skipping line {} ({}): missing '='.",
                    idx + 1,
                    raw_line.trim()
                );
                continue;
            };

            let key = line[..eq_idx].trim();
            if key.is_empty() {
                eprintln!("Skipping line {}: missing key before '='.", idx + 1);
                continue;
            }

            let raw_value = line[eq_idx + 1..].trim();
            let value = parse_env_value(raw_value);

            entries.push(ImportEntry {
                key: key.to_string(),
                value,
                value_type: EntryValueType::Prompt,
            });
        }

        if entries.is_empty() {
            Err("no key=value pairs found".to_string())
        } else {
            Ok(entries)
        }
    }

    fn entries_from_json_value(value: serde_json::Value) -> Result<Vec<ImportEntry>, String> {
        let map = value.as_object().ok_or_else(|| {
            "Import file must contain a mapping of keys to values.".to_string()
        })?;

        Ok(map_to_entries(map))
    }

    fn map_to_entries(map: &serde_json::Map<String, serde_json::Value>) -> Vec<ImportEntry> {
        let mut entries = Vec::new();

        for (key, value) in map {
            if let Some(obj) = value.as_object() {
                let value_type = value_type_from_str(obj.get("type").and_then(|v| v.as_str()));
                let val_str = obj
                    .get("value")
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default();
                entries.push(ImportEntry {
                    key: key.to_string(),
                    value: val_str,
                    value_type,
                });
            } else if let Some(val_str) = value.as_str() {
                entries.push(ImportEntry {
                    key: key.to_string(),
                    value: val_str.to_string(),
                    value_type: EntryValueType::Plain,
                });
            } else {
                entries.push(ImportEntry {
                    key: key.to_string(),
                    value: value.to_string(),
                    value_type: EntryValueType::Plain,
                });
            }
        }

        entries
    }

    fn process_import_entry(ctx: &ActiveKvContext, entry: &ImportEntry) -> bool {
        let full_key = prefix_key(ctx, &entry.key);

        if let Some(existing_entry) = show_entry(ctx, &full_key).ok() {
            if let Some(secret_uri) = keyvault_uri_from_entry(&existing_entry) {
                return match set_secret_value(&secret_uri, &entry.value) {
                    Ok(_) => true,
                    Err(err) => {
                        eprintln!(
                            "Failed to update Key Vault secret for '{}': {err}",
                            entry.key
                        );
                        false
                    }
                };
            }

            return match write_entry(ctx, &full_key, &entry.value, None) {
                Ok(_) => true,
                Err(err) => {
                    eprintln!("Failed to import '{}': {err}", entry.key);
                    false
                }
            };
        }

        let write_result = match entry.value_type {
            EntryValueType::KeyVault => match build_keyvault_reference(ctx, &full_key, &entry.value)
            {
                Some(secret_uri) => write_keyvault_entry(ctx, &full_key, &secret_uri),
                None => {
                    eprintln!(
                        "Skipping '{}' (keyvault type) because no Key Vault is configured.",
                        entry.key
                    );
                    return false;
                }
            },
            EntryValueType::Plain => write_entry(ctx, &full_key, &entry.value, None),
            EntryValueType::Prompt => {
                eprintln!(
                    "Internal error: unresolved prompt for '{}'. Skipping entry.",
                    entry.key
                );
                return false;
            }
        };

        match write_result {
            Ok(_) => true,
            Err(err) => {
                eprintln!("Failed to import '{}': {err}", entry.key);
                false
            }
        }
    }

    fn value_type_from_str(value: Option<&str>) -> EntryValueType {
        let lower = value
            .map(|s| s.trim().to_ascii_lowercase())
            .unwrap_or_else(|| "plain".to_string());

        match lower.as_str() {
            "keyvault" => EntryValueType::KeyVault,
            "prompt" => EntryValueType::Prompt,
            _ => EntryValueType::Plain,
        }
    }

    fn parse_env_value(raw_value: &str) -> String {
        let without_comment = strip_env_comment(raw_value);
        let trimmed = without_comment.trim();

        if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
            return unescape_double_quoted(&trimmed[1..trimmed.len() - 1]);
        }

        if trimmed.len() >= 2 && trimmed.starts_with('\'') && trimmed.ends_with('\'') {
            return trimmed[1..trimmed.len() - 1].to_string();
        }

        trimmed.to_string()
    }

    fn strip_env_comment(value: &str) -> &str {
        let mut in_single = false;
        let mut in_double = false;
        let mut escaped = false;

        for (idx, ch) in value.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }

            match ch {
                '\\' if in_double => {
                    escaped = true;
                }
                '\'' if !in_double => in_single = !in_single,
                '"' if !in_single => in_double = !in_double,
                '#' if !in_single && !in_double => return value[..idx].trim_end(),
                _ => {}
            }
        }

        value
    }

    fn unescape_double_quoted(input: &str) -> String {
        let mut result = String::with_capacity(input.len());
        let mut chars = input.chars();

        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    match next {
                        'n' => result.push('\n'),
                        'r' => result.push('\r'),
                        't' => result.push('\t'),
                        '\\' => result.push('\\'),
                        '"' => result.push('"'),
                        _ => {
                            result.push('\\');
                            result.push(next);
                        }
                    }
                } else {
                    result.push('\\');
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    fn escape_double_quoted(input: &str) -> String {
        let mut result = String::with_capacity(input.len());

        for ch in input.chars() {
            match ch {
                '\n' => result.push_str("\\n"),
                '\r' => result.push_str("\\r"),
                '\t' => result.push_str("\\t"),
                '\\' => result.push_str("\\\\"),
                '"' => result.push_str("\\\""),
                _ => result.push(ch),
            }
        }

        result
    }

    fn prompt_value_type(key: &str) -> Option<EntryValueType> {
        #[derive(Clone)]
        enum ValueChoice {
            Plain,
            KeyVault,
            Skip,
        }

        let options = vec![
            MenuItem::new(ValueChoice::Plain, "Store as plain value".to_string()),
            MenuItem::new(ValueChoice::KeyVault, "Store in Key Vault".to_string()),
            MenuItem::new(ValueChoice::Skip, "Skip this entry".to_string()),
        ];

        let prompt = format!("Where should key '{}' be stored?", key);
        let selection = Select::new(&prompt, options)
            .with_starting_cursor(0)
            .prompt();

        match selection {
            Ok(choice) => match choice.value {
                ValueChoice::Plain => Some(EntryValueType::Plain),
                ValueChoice::KeyVault => Some(EntryValueType::KeyVault),
                ValueChoice::Skip => None,
            },
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                Some(EntryValueType::Plain)
            }
            Err(err) => {
                eprintln!("Prompt failed for '{}': {}", key, err);
                None
            }
        }
    }

    impl EntryValueType {
        fn label(self) -> &'static str {
            match self {
                EntryValueType::Plain => "[plain]",
                EntryValueType::KeyVault => "[keyvault]",
                EntryValueType::Prompt => "[prompt]",
            }
        }
    }

    impl ImportFormat {
        fn label(self) -> &'static str {
            match self {
                ImportFormat::Json => "JSON",
                ImportFormat::Yaml => "YAML",
                ImportFormat::Toml => "TOML",
                ImportFormat::Env => ".env",
            }
        }
    }

    fn build_keyvault_reference(
        ctx: &ActiveKvContext,
        full_key: &str,
        secret_value: &str,
    ) -> Option<String> {
        let vault_base = match ensure_vault_base(ctx) {
            Some(base) => base,
            None => {
                eprintln!("No Key Vault configured for the current application.");
                return None;
            }
        };

        let secret_name = secret_name_from_key(full_key);
        let secret_uri = format!("{}/secrets/{}", vault_base, secret_name);

        if let Err(err) = create_or_update_secret(&vault_base, &secret_name, secret_value) {
            eprintln!("Failed to create secret '{}': {}", secret_name, err);
            return None;
        }

        Some(secret_uri)
    }

    fn secret_name_from_key(full_key: &str) -> String {
        let sanitized: String = full_key
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { ' ' })
            .collect();
        sanitized.to_upper_camel_case()
    }

    fn ensure_vault_base(ctx: &ActiveKvContext) -> Option<String> {
        let vault = ctx.keyvault.as_deref()?.trim();
        if vault.is_empty() {
            return None;
        }

        let normalized = if vault.starts_with("http://") || vault.starts_with("https://") {
            vault.trim_end_matches('/').to_string()
        } else {
            format!("https://{}.vault.azure.net", vault.trim_end_matches('/'))
        };

        Some(normalized)
    }

    fn parse_secret_uri(uri: &str) -> Option<(String, String)> {
        let without_scheme = uri.splitn(2, "://").nth(1)?;
        let mut parts = without_scheme.split('/');
        let host = parts.next()?.trim();
        if host.is_empty() {
            return None;
        }

        let first = parts.next()?;
        if first != "secrets" {
            return None;
        }

        let name = parts.next()?.trim();
        if name.is_empty() {
            return None;
        }

        let vault_name = host.split('.').next()?.to_string();
        Some((vault_name, name.to_string()))
    }

    fn create_or_update_secret(vault_base: &str, secret_name: &str, value: &str) -> AzCliResult<()> {
        let vault_name = vault_base
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('.')
            .next()
            .unwrap_or(vault_base);

        let _: serde_json::Value = az([
            "keyvault",
            "secret",
            "set",
            "--vault-name",
            vault_name,
            "--name",
            secret_name,
            "--value",
            value,
            "-o",
            "json",
        ])?;
        Ok(())
    }
}

fn load_context() -> Option<(ContextStore, Context)> {
    let store = match ContextStore::new() {
        Ok(store) => store,
        Err(err) => {
            eprintln!("Failed to locate context store: {err}");
            return None;
        }
    };

    let context = match Context::load_or_default(&store) {
        Ok(context) => context,
        Err(err) => {
            eprintln!("Failed to load context: {err}");
            return None;
        }
    };

    Some((store, context))
}

fn save_context(store: &ContextStore, context: &Context) -> bool {
    match context.save(store) {
        Ok(_) => true,
        Err(err) => {
            eprintln!("Failed to save context: {err}");
            false
        }
    }
}

fn missing_setup_message() {
    eprintln!("No App Configuration context configured. Run `azac setup` first.");
}

fn standard_spinner_style() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.green} {msg}")
        .unwrap_or_else(|_| ProgressStyle::default_spinner())
}

fn ensure_cache_ready() -> Option<SetupCache> {
    let (store, cache) = match load_cache_state() {
        Some(value) => value,
        None => return None,
    };

    if cache.is_ready() {
        return Some(cache);
    }

    refresh_cache(store, CacheRefreshKind::Auto)
}

fn load_cache_state() -> Option<(CacheStore, SetupCache)> {
    let store = match CacheStore::new() {
        Ok(store) => store,
        Err(err) => {
            eprintln!("Failed to locate cache store: {err}");
            return None;
        }
    };

    let cache = match SetupCache::load_or_default(&store) {
        Ok(cache) => cache,
        Err(err) => {
            eprintln!("Failed to load cache: {err}");
            return None;
        }
    };

    Some((store, cache))
}

fn save_cache(store: &CacheStore, cache: &SetupCache) -> bool {
    match cache.save(store) {
        Ok(_) => true,
        Err(err) => {
            eprintln!("Failed to save cache: {err}");
            false
        }
    }
}

fn refresh_cache(store: CacheStore, kind: CacheRefreshKind) -> Option<SetupCache> {
    if let Some(msg) = kind.start_message() {
        println!("{msg}");
    }

    let spinner_style = standard_spinner_style();
    let multi = MultiProgress::new();

    let sub_bar = multi.add(ProgressBar::new_spinner());
    sub_bar.set_style(spinner_style.clone());
    sub_bar.set_message("Fetching Azure subscriptions...");
    sub_bar.enable_steady_tick(Duration::from_millis(80));

    let subscriptions = match subscription::list_subscription() {
        Ok(list) => list,
        Err(err) => {
            sub_bar.finish_and_clear();
            eprintln!("Failed to list Azure subscriptions: {err}");
            return None;
        }
    };
    sub_bar.finish_and_clear();

    if subscriptions.is_empty() {
        eprintln!("No Azure subscriptions available.");
        return None;
    }

    let (tx, rx) = mpsc::channel();

    thread::scope(|scope| {
        for subscription in subscriptions.iter().cloned() {
            let bar = multi.add(ProgressBar::new_spinner());
            bar.set_style(spinner_style.clone());
            let tx = tx.clone();

            scope.spawn(move || {
                bar.enable_steady_tick(Duration::from_millis(80));
                bar.set_message(format!(
                    "Fetching App Configurations for '{}'",
                    subscription.name
                ));

                match appconfig::list_appconfig(&subscription.id) {
                    Ok(configs) => {
                        bar.finish_and_clear();
                        let _ = tx.send((subscription, configs));
                    }
                    Err(err) => {
                        bar.finish_and_clear();
                        eprintln!(
                            "Failed to list App Configurations for '{}': {}",
                            subscription.name, err
                        );
                    }
                }
            });
        }
    });

    drop(tx);

    let mut cached_configs = Vec::new();

    for (subscription, configs) in rx {
        for cfg in configs {
            let endpoint = normalize_appconfig_endpoint(&cfg.name, &cfg.endpoint);
            cached_configs.push(CachedAppConfig {
                subscription_id: subscription.id.clone(),
                subscription_name: subscription.name.clone(),
                name: cfg.name,
                endpoint,
            });
        }
    }

    if cached_configs.is_empty() {
        eprintln!("No App Configuration instances were found across your subscriptions.");
        return None;
    }

    let kv_spinner = ProgressBar::new_spinner();
    kv_spinner.set_style(spinner_style.clone());
    kv_spinner.enable_steady_tick(Duration::from_millis(80));
    kv_spinner.set_message("Fetching Key Vaults...");

    let keyvaults = match fetch_keyvault_inventory(&subscriptions) {
        Ok(vaults) => vaults,
        Err(err) => {
            kv_spinner.finish_and_clear();
            eprintln!("Failed to list Key Vaults: {err}");
            return None;
        }
    };

    kv_spinner.finish_and_clear();

    let cache = SetupCache {
        subscriptions,
        appconfigs: cached_configs,
        keyvaults,
        ready: true,
    };

    if !save_cache(&store, &cache) {
        return None;
    }

    if let Some(msg) = kind.success_message() {
        println!("{msg}");
    }

    Some(cache)
}

enum CacheRefreshKind {
    Manual,
    Auto,
}

impl CacheRefreshKind {
    fn start_message(&self) -> Option<&'static str> {
        match self {
            CacheRefreshKind::Manual => None,
            CacheRefreshKind::Auto => None,
        }
    }

    fn success_message(&self) -> Option<&'static str> {
        match self {
            CacheRefreshKind::Manual => None,
            CacheRefreshKind::Auto => None,
        }
    }
}

fn fetch_keyvault_inventory(
    subscriptions: &[subscription::Subscription],
) -> AzCliResult<Vec<CachedKeyVault>> {
    #[derive(Debug, Deserialize)]
    struct KeyVaultResource {
        name: String,
        id: String,
    }

    let mut vaults = Vec::new();

    for sub in subscriptions {
        let resources: Vec<KeyVaultResource> = az([
            "resource",
            "list",
            "--resource-type",
            "Microsoft.KeyVault/vaults",
            "--subscription",
            &sub.id,
            "-o",
            "json",
        ])?;

        for res in resources {
            let sub_id =
                subscription_from_resource_id(&res.id).unwrap_or_else(|| sub.id.clone());
            vaults.push(CachedKeyVault {
                name: res.name,
                subscription_id: sub_id,
            });
        }
    }

    Ok(vaults)
}

fn subscription_from_resource_id(id: &str) -> Option<String> {
    let marker = "/subscriptions/";
    let start = id.find(marker)? + marker.len();
    let rest = &id[start..];
    let end = rest.find('/')?;
    let sub = &rest[..end];
    if sub.is_empty() {
        None
    } else {
        Some(sub.to_string())
    }
}

fn normalize_appconfig_endpoint(config_name: &str, endpoint: &str) -> String {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return derive_endpoint_from_name(config_name);
    }

    let normalized = trimmed.trim_end_matches('/');
    if normalized.is_empty() {
        return derive_endpoint_from_name(config_name);
    }

    if normalized == DEFAULT_APP_CONFIG_ENDPOINT {
        let derived = derive_endpoint_from_name(config_name);
        if derived != DEFAULT_APP_CONFIG_ENDPOINT {
            return derived;
        }
    }

    normalized.to_string()
}

fn derive_endpoint_from_name(config_name: &str) -> String {
    let trimmed_name = config_name.trim();
    if trimmed_name.is_empty() {
        default_appconfig_endpoint()
    } else {
        format!("https://{}.azconfig.io", trimmed_name)
    }
}
