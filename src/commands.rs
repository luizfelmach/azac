use crate::{
    azcli::subscription,
    context::{Context, ContextStore},
};

pub mod cfg {
    use crate::azcli::appconfig;
    use crate::context::AppConfigurationContext;

    pub fn list_configs() {
        let Some(subscription) = super::current_subscription() else {
            eprintln!("Could not determine the active Azure subscription.");
            return;
        };

        let (_, context) = match super::load_context() {
            Some(value) => value,
            None => return,
        };

        let current_cfg = context
            .subscriptions
            .get(&subscription.id)
            .and_then(|ctx| ctx.current.as_deref())
            .map(|value| value.to_string());

        match appconfig::list_appconfig(&subscription.id) {
            Ok(configs) => {
                if configs.is_empty() {
                    println!(
                        "No App Configuration instances found for subscription '{}'.",
                        subscription.name
                    );
                    return;
                }

                for cfg in configs {
                    let marker = if current_cfg.as_deref() == Some(cfg.name.as_str()) {
                        "*"
                    } else {
                        " "
                    };
                    println!("[{}] {}", marker, cfg.name);
                }
            }
            Err(err) => eprintln!("Failed to list App Configuration instances: {err}"),
        }
    }

    pub fn use_config(name: &str) {
        let Some(subscription) = super::current_subscription() else {
            eprintln!("Could not determine the active Azure subscription.");
            return;
        };

        let (store, mut context) = match super::load_context() {
            Some(value) => value,
            None => return,
        };

        let sub_ctx = context
            .subscriptions
            .entry(subscription.id.clone())
            .or_default();
        sub_ctx.current = Some(name.to_string());
        sub_ctx
            .configs
            .entry(name.to_string())
            .or_insert_with(AppConfigurationContext::default);

        if super::save_context(&store, &context) {
            println!(
                "Using App Configuration '{}' for subscription '{}'.",
                name, subscription.name
            );
        }
    }

    pub fn show_config(name: &str) {
        let Some(subscription) = super::current_subscription() else {
            eprintln!("Could not determine the active Azure subscription.");
            return;
        };

        let (_, context) = match super::load_context() {
            Some(value) => value,
            None => return,
        };

        let Some(sub_ctx) = context.subscriptions.get(&subscription.id) else {
            eprintln!(
                "No stored context for subscription '{}'.",
                subscription.name
            );
            return;
        };

        let Some(cfg_ctx) = sub_ctx.configs.get(name) else {
            eprintln!(
                "Configuration '{}' is not tracked yet. Use `azac cfg use {}` first.",
                name, name
            );
            return;
        };

        println!("Subscription: {} ({})", subscription.name, subscription.id);
        println!("App Configuration: {}", name);
        println!("Separator: {}", cfg_ctx.separator);
        if let Some(current_app) = &cfg_ctx.current {
            println!("Current app: {}", current_app);
        }

        if cfg_ctx.apps.is_empty() {
            println!("Apps: none defined");
        } else {
            println!("Apps:");
            for (app_name, app_ctx) in &cfg_ctx.apps {
                let marker = if cfg_ctx.current.as_deref() == Some(app_name.as_str()) {
                    "*"
                } else {
                    " "
                };
                println!("  [{}] {}", marker, app_name);
                if let Some(label) = &app_ctx.label {
                    println!("    label: {}", label);
                }
                if let Some(vault) = &app_ctx.keyvault {
                    println!("    keyvault: {}", vault);
                }
            }
        }
    }

    pub fn set_separator(separator: &str) {
        let Some(subscription) = super::current_subscription() else {
            eprintln!("Could not determine the active Azure subscription.");
            return;
        };

        let (store, mut context) = match super::load_context() {
            Some(value) => value,
            None => return,
        };

        let Some(sub_ctx) = context.subscriptions.get_mut(&subscription.id) else {
            eprintln!("No App Configuration selected. Use `azac cfg use <name>` first.");
            return;
        };

        let Some(cfg_name) = sub_ctx.current.clone() else {
            eprintln!("No App Configuration selected. Use `azac cfg use <name>` first.");
            return;
        };

        let cfg_ctx = sub_ctx
            .configs
            .entry(cfg_name.clone())
            .or_insert_with(AppConfigurationContext::default);
        cfg_ctx.separator = separator.to_string();

        if super::save_context(&store, &context) {
            println!("Separator for '{}' set to '{}'.", cfg_name, separator);
        }
    }

    pub fn show_current_config() {
        let Some(subscription) = super::current_subscription() else {
            eprintln!("Could not determine the active Azure subscription.");
            return;
        };

        let (_, context) = match super::load_context() {
            Some(value) => value,
            None => return,
        };

        let Some(sub_ctx) = context.subscriptions.get(&subscription.id) else {
            eprintln!(
                "No App Configuration selected for subscription '{}'.",
                subscription.name
            );
            return;
        };

        let Some(cfg_name) = &sub_ctx.current else {
            eprintln!(
                "No App Configuration selected for subscription '{}'.",
                subscription.name
            );
            return;
        };

        println!("{}", cfg_name);
    }
}

pub mod app {
    use std::collections::BTreeSet;

    use serde::Deserialize;

    use crate::azcli::{error::AzCliResult, run::az};
    use crate::context::AppConfigurationContext;

    pub fn list_apps() {
        let Some(subscription) = super::current_subscription() else {
            eprintln!("Could not determine the active Azure subscription.");
            return;
        };

        let (_, context) = match super::load_context() {
            Some(value) => value,
            None => return,
        };

        let Some(sub_ctx) = context.subscriptions.get(&subscription.id) else {
            eprintln!("No App Configuration selected. Use `azac cfg use <name>` first.");
            return;
        };

        let Some(cfg_name) = &sub_ctx.current else {
            eprintln!("No App Configuration selected. Use `azac cfg use <name>` first.");
            return;
        };

        let default_ctx;
        let cfg_ctx = match sub_ctx.configs.get(cfg_name) {
            Some(ctx) => ctx,
            None => {
                default_ctx = AppConfigurationContext::default();
                &default_ctx
            }
        };

        let entries = match fetch_all_keys(cfg_name, &subscription.id) {
            Ok(entries) => entries,
            Err(err) => {
                eprintln!("Failed to list applications: {err}");
                return;
            }
        };

        let mut apps = BTreeSet::new();
        for entry in entries {
            if let Some(idx) = entry.key.rfind(&cfg_ctx.separator) {
                let prefix = &entry.key[..idx];
                apps.insert(prefix.to_string());
            }
        }

        if apps.is_empty() {
            println!("No applications inferred from keys in '{}'.", cfg_name);
            return;
        }

        for app_name in apps {
            let marker = if cfg_ctx.current.as_deref() == Some(app_name.as_str()) {
                "*"
            } else {
                " "
            };
            println!("[{}] {}", marker, app_name);
        }
    }

    pub fn use_app(name: &str) {
        let Some(subscription) = super::current_subscription() else {
            eprintln!("Could not determine the active Azure subscription.");
            return;
        };

        let (store, mut context) = match super::load_context() {
            Some(value) => value,
            None => return,
        };

        let Some(sub_ctx) = context.subscriptions.get_mut(&subscription.id) else {
            eprintln!("No App Configuration selected. Use `azac cfg use <name>` first.");
            return;
        };

        let Some(cfg_name) = sub_ctx.current.clone() else {
            eprintln!("No App Configuration selected. Use `azac cfg use <name>` first.");
            return;
        };

        let cfg_ctx = sub_ctx
            .configs
            .entry(cfg_name.clone())
            .or_insert_with(AppConfigurationContext::default);
        cfg_ctx.current = Some(name.to_string());
        cfg_ctx.apps.entry(name.to_string()).or_default();

        if super::save_context(&store, &context) {
            println!(
                "Using application '{}' under App Configuration '{}'.",
                name, cfg_name
            );
        }
    }

    pub fn show_app(name: &str) {
        let Some(subscription) = super::current_subscription() else {
            eprintln!("Could not determine the active Azure subscription.");
            return;
        };

        let (_, context) = match super::load_context() {
            Some(value) => value,
            None => return,
        };

        let Some(sub_ctx) = context.subscriptions.get(&subscription.id) else {
            eprintln!("No App Configuration selected. Use `azac cfg use <name>` first.");
            return;
        };

        let Some(cfg_name) = &sub_ctx.current else {
            eprintln!("No App Configuration selected. Use `azac cfg use <name>` first.");
            return;
        };

        let Some(cfg_ctx) = sub_ctx.configs.get(cfg_name) else {
            eprintln!(
                "Configuration '{}' is not tracked. Use `azac cfg use {}` first.",
                cfg_name, cfg_name
            );
            return;
        };

        let Some(app_ctx) = cfg_ctx.apps.get(name) else {
            eprintln!("Application '{}' not defined for '{}'.", name, cfg_name);
            return;
        };

        println!("App: {}", name);
        println!("Label: {}", app_ctx.label.as_deref().unwrap_or("(none)"));
        println!(
            "Key Vault: {}",
            app_ctx.keyvault.as_deref().unwrap_or("(none)")
        );
    }

    pub fn set_label(label: &str) {
        let Some(subscription) = super::current_subscription() else {
            eprintln!("Could not determine the active Azure subscription.");
            return;
        };

        let (store, mut context) = match super::load_context() {
            Some(value) => value,
            None => return,
        };

        let Some(sub_ctx) = context.subscriptions.get_mut(&subscription.id) else {
            eprintln!("No App Configuration selected. Use `azac cfg use <name>` first.");
            return;
        };

        let Some(cfg_name) = sub_ctx.current.clone() else {
            eprintln!("No App Configuration selected. Use `azac cfg use <name>` first.");
            return;
        };

        let cfg_ctx = sub_ctx
            .configs
            .entry(cfg_name.clone())
            .or_insert_with(AppConfigurationContext::default);

        let Some(app_name) = cfg_ctx.current.clone() else {
            eprintln!("No application selected. Use `azac app use <name>` first.");
            return;
        };

        let app_ctx = cfg_ctx.apps.entry(app_name.clone()).or_default();
        app_ctx.label = Some(label.to_string());

        if super::save_context(&store, &context) {
            println!("Set label for '{}' to '{}'.", app_name, label);
        }
    }

    pub fn set_keyvault(vault: &str) {
        let Some(subscription) = super::current_subscription() else {
            eprintln!("Could not determine the active Azure subscription.");
            return;
        };

        let (store, mut context) = match super::load_context() {
            Some(value) => value,
            None => return,
        };

        let Some(sub_ctx) = context.subscriptions.get_mut(&subscription.id) else {
            eprintln!("No App Configuration selected. Use `azac cfg use <name>` first.");
            return;
        };

        let Some(cfg_name) = sub_ctx.current.clone() else {
            eprintln!("No App Configuration selected. Use `azac cfg use <name>` first.");
            return;
        };

        let cfg_ctx = sub_ctx
            .configs
            .entry(cfg_name.clone())
            .or_insert_with(AppConfigurationContext::default);

        let Some(app_name) = cfg_ctx.current.clone() else {
            eprintln!("No application selected. Use `azac app use <name>` first.");
            return;
        };

        let app_ctx = cfg_ctx.apps.entry(app_name.clone()).or_default();
        app_ctx.keyvault = Some(vault.to_string());

        if super::save_context(&store, &context) {
            println!("Set Key Vault for '{}' to '{}'.", app_name, vault);
        }
    }

    pub fn show_current_app() {
        let Some(subscription) = super::current_subscription() else {
            eprintln!("Could not determine the active Azure subscription.");
            return;
        };

        let (_, context) = match super::load_context() {
            Some(value) => value,
            None => return,
        };

        let Some(sub_ctx) = context.subscriptions.get(&subscription.id) else {
            eprintln!("No App Configuration selected. Use `azac cfg use <name>` first.");
            return;
        };

        let Some(cfg_name) = &sub_ctx.current else {
            eprintln!("No App Configuration selected. Use `azac cfg use <name>` first.");
            return;
        };

        let Some(cfg_ctx) = sub_ctx.configs.get(cfg_name) else {
            eprintln!(
                "Configuration '{}' is not tracked. Use `azac cfg use {}` first.",
                cfg_name, cfg_name
            );
            return;
        };

        let Some(app_name) = &cfg_ctx.current else {
            eprintln!("No application selected for '{}'.", cfg_name);
            return;
        };

        let app_ctx = cfg_ctx.apps.get(app_name);
        let label = app_ctx
            .and_then(|ctx| ctx.label.as_deref())
            .unwrap_or("(none)");
        let keyvault = app_ctx
            .and_then(|ctx| ctx.keyvault.as_deref())
            .unwrap_or("(none)");

        println!("App: {}", app_name);
        println!("Label: {}", label);
        println!("Key Vault: {}", keyvault);
    }

    #[derive(Debug, Deserialize)]
    struct KeyValue {
        key: String,
    }

    fn fetch_all_keys(config_name: &str, subscription_id: &str) -> AzCliResult<Vec<KeyValue>> {
        az([
            "appconfig",
            "kv",
            "list",
            "--name",
            config_name,
            "--subscription",
            subscription_id,
            "--all",
            "-o",
            "json",
        ])
    }
}

pub mod kv {
    use std::{fs, path::Path};

    use clap::ValueEnum;
    use heck::ToUpperCamelCase;
    use serde::Deserialize;

    use crate::azcli::{error::AzCliResult, run::az};
    use crate::context::AppConfigurationContext;

    #[derive(Clone, Debug, ValueEnum)]
    pub enum ExportFormat {
        Json,
        Yaml,
        Toml,
    }

    #[derive(Debug, Deserialize)]
    struct KeyValue {
        key: String,
        label: Option<String>,
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
        subscription_id: String,
        config_name: String,
        separator: String,
        app_name: Option<String>,
        label: Option<String>,
        keyvault: Option<String>,
    }

    pub fn list_keys() {
        let Some(ctx) = resolve_active_context(true, true) else {
            return;
        };

        let entries = match fetch_entries(&ctx) {
            Ok(entries) => entries,
            Err(err) => {
                eprintln!("Failed to list keys: {err}");
                return;
            }
        };

        if entries.is_empty() {
            let app_suffix = ctx
                .app_name
                .as_ref()
                .map(|app| format!(" and app '{}'", app))
                .unwrap_or_default();
            println!(
                "No keys found for App Configuration '{}'{}.",
                ctx.config_name, app_suffix
            );
            return;
        }

        for entry in entries {
            let key = strip_prefix(&ctx, &entry.key);
            let (value, from_keyvault) = resolve_value(&entry, false);
            if from_keyvault {
                println!("- {} [keyvault]", key);
            } else {
                println!("- {} = {}", key, value);
            }
        }
    }

    pub fn show_key(key: &str) {
        let Some(ctx) = resolve_active_context(true, true) else {
            return;
        };

        let full_key = prefix_key(&ctx, key);
        let entry = match show_entry(&ctx, &full_key) {
            Ok(entry) => entry,
            Err(err) => {
                eprintln!("Failed to fetch key: {err}");
                return;
            }
        };

        let display_key = strip_prefix(&ctx, &entry.key);
        let (value, from_keyvault) = resolve_value(&entry, true);
        if from_keyvault {
            println!("- {} [keyvault]", display_key);
            println!("  value: {}", value);
        } else {
            println!("- {} = {}", display_key, value);
        }
    }

    pub fn set_key(key: &str, value: &str, use_keyvault: bool) {
        let Some(ctx) = resolve_active_context(true, true) else {
            return;
        };

        let full_key = prefix_key(&ctx, key);

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

    pub fn delete_keys(keys: &[String]) {
        let Some(ctx) = resolve_active_context(true, true) else {
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

    pub fn export_entries(format: ExportFormat) {
        let Some(ctx) = resolve_active_context(true, true) else {
            return;
        };

        let entries = match fetch_entries(&ctx) {
            Ok(entries) => entries,
            Err(err) => {
                eprintln!("Failed to export entries: {err}");
                return;
            }
        };

        let mut map = serde_json::Map::new();
        for entry in entries {
            let key = strip_prefix(&ctx, &entry.key);
            let (value, from_keyvault) = resolve_value(&entry, true);
            let mut obj = serde_json::Map::new();
            obj.insert(
                "type".to_string(),
                serde_json::Value::String(if from_keyvault {
                    "keyvault".to_string()
                } else {
                    "plain".to_string()
                }),
            );
            obj.insert("value".to_string(), serde_json::Value::String(value));
            map.insert(key, serde_json::Value::Object(obj));
        }

        let payload = serde_json::Value::Object(map);

        match format {
            ExportFormat::Json => match serde_json::to_string_pretty(&payload) {
                Ok(data) => println!("{data}"),
                Err(err) => eprintln!("Failed to serialize JSON: {err}"),
            },
            ExportFormat::Yaml => match serde_yaml::to_string(&payload) {
                Ok(data) => println!("{data}"),
                Err(err) => eprintln!("Failed to serialize YAML: {err}"),
            },
            ExportFormat::Toml => match toml::to_string_pretty(&payload) {
                Ok(data) => println!("{data}"),
                Err(err) => eprintln!("Failed to serialize TOML: {err}"),
            },
        }
    }

    pub fn import_entries(path: &Path) {
        let Some(ctx) = resolve_active_context(true, true) else {
            return;
        };

        let Some(entries) = parse_import_map(path) else {
            return;
        };

        let mut imported = 0usize;
        for entry in entries {
            let full_key = prefix_key(&ctx, &entry.key);
            let lower_type = entry.value_type.to_ascii_lowercase();
            let write_result = match lower_type.as_str() {
                "keyvault" => match build_keyvault_reference(&ctx, &full_key, &entry.value) {
                    Some(secret_uri) => write_keyvault_entry(&ctx, &full_key, &secret_uri),
                    None => {
                        eprintln!(
                            "Skipping '{}' (keyvault type) because no Key Vault is configured.",
                            entry.key
                        );
                        continue;
                    }
                },
                _ => write_entry(&ctx, &full_key, &entry.value, None),
            };

            match write_result {
                Ok(_) => imported += 1,
                Err(err) => eprintln!("Failed to import '{}': {err}", entry.key),
            }
        }

        println!("Imported {} entries into '{}'.", imported, ctx.config_name);
    }

    fn resolve_active_context(require_app: bool, require_label: bool) -> Option<ActiveKvContext> {
        let subscription = match super::current_subscription() {
            Some(sub) => sub,
            None => return None,
        };

        let (_, context) = match super::load_context() {
            Some(value) => value,
            None => return None,
        };

        let sub_ctx = match context.subscriptions.get(&subscription.id) {
            Some(ctx) => ctx,
            None => {
                eprintln!(
                    "No stored context for subscription '{}'. Use `azac cfg use <name>` first.",
                    subscription.name
                );
                return None;
            }
        };

        let config_name = match sub_ctx.current.as_ref() {
            Some(name) => name.clone(),
            None => {
                eprintln!(
                    "No App Configuration selected for subscription '{}'. Use `azac cfg use <name>`.",
                    subscription.name
                );
                return None;
            }
        };

        let default_ctx;
        let cfg_ctx = match sub_ctx.configs.get(&config_name) {
            Some(ctx) => ctx,
            None => {
                default_ctx = AppConfigurationContext::default();
                &default_ctx
            }
        };

        let app_name = cfg_ctx.current.clone();

        if require_app && app_name.is_none() {
            eprintln!("No application selected. Use `azac app use <name>` first.");
            return None;
        }

        let app_ctx = app_name.as_ref().and_then(|name| cfg_ctx.apps.get(name));

        let label = app_ctx
            .and_then(|ctx| ctx.label.clone())
            .filter(|lbl| !lbl.is_empty());
        if require_label && label.is_none() {
            eprintln!("No label configured. Set one with `azac app label <label>` first.");
            return None;
        }

        let keyvault = app_ctx
            .and_then(|ctx| ctx.keyvault.clone())
            .filter(|kv| !kv.is_empty());

        Some(ActiveKvContext {
            subscription_id: subscription.id,
            config_name,
            separator: cfg_ctx.separator.clone(),
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
            "--name".to_string(),
            ctx.config_name.clone(),
            "--subscription".to_string(),
            ctx.subscription_id.clone(),
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
            "--name".to_string(),
            ctx.config_name.clone(),
            "--subscription".to_string(),
            ctx.subscription_id.clone(),
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
            "--name".to_string(),
            ctx.config_name.clone(),
            "--subscription".to_string(),
            ctx.subscription_id.clone(),
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
            "--name".to_string(),
            ctx.config_name.clone(),
            "--subscription".to_string(),
            ctx.subscription_id.clone(),
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
            "--name".to_string(),
            ctx.config_name.clone(),
            "--subscription".to_string(),
            ctx.subscription_id.clone(),
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

    fn resolve_value(entry: &KeyValue, fetch_secret: bool) -> (String, bool) {
        if let Some(uri) = keyvault_uri_from_entry(entry) {
            if fetch_secret {
                match fetch_secret_value(&uri) {
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
        value_type: String,
    }

    fn parse_import_map(path: &Path) -> Option<Vec<ImportEntry>> {
        let contents = match fs::read_to_string(path) {
            Ok(data) => data,
            Err(err) => {
                eprintln!("Failed to read {}: {err}", path.display());
                return None;
            }
        };

        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let parsed: Result<serde_json::Value, String> =
            match ext.as_str() {
                "yaml" | "yml" => serde_yaml::from_str::<serde_json::Value>(&contents)
                    .map_err(|err| err.to_string()),
                "toml" => toml::from_str::<toml::Value>(&contents)
                    .map_err(|err| err.to_string())
                    .and_then(|value| serde_json::to_value(value).map_err(|err| err.to_string())),
                _ => serde_json::from_str::<serde_json::Value>(&contents)
                    .map_err(|err| err.to_string()),
            };

        let value = match parsed {
            Ok(value) => value,
            Err(err) => {
                eprintln!("Failed to parse {}: {}", path.display(), err);
                return None;
            }
        };

        let map = match value.as_object() {
            Some(map) => map,
            None => {
                eprintln!("Import file must contain a mapping of keys to values.");
                return None;
            }
        };

        let mut entries = Vec::new();

        for (key, value) in map {
            if let Some(obj) = value.as_object() {
                let value_type = obj
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("plain")
                    .to_string();
                let val_str = obj
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                entries.push(ImportEntry {
                    key: key.to_string(),
                    value: val_str,
                    value_type,
                });
            } else if let Some(val_str) = value.as_str() {
                entries.push(ImportEntry {
                    key: key.to_string(),
                    value: val_str.to_string(),
                    value_type: "plain".to_string(),
                });
            } else {
                entries.push(ImportEntry {
                    key: key.to_string(),
                    value: value.to_string(),
                    value_type: "plain".to_string(),
                });
            }
        }

        Some(entries)
    }

    fn build_keyvault_reference(
        ctx: &ActiveKvContext,
        full_key: &str,
        secret_value: &str,
    ) -> Option<String> {
        let vault_base = match ensure_vault_base(ctx) {
            Some(base) => base,
            None => {
                eprintln!("No Key Vault configured. Set one with `azac app keyvault <vault>` first.");
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

fn current_subscription() -> Option<subscription::Subscription> {
    match subscription::list_subscription() {
        Ok(subscriptions) => {
            if subscriptions.is_empty() {
                eprintln!("No Azure subscriptions are available.");
                return None;
            }

            let default = subscriptions
                .iter()
                .find(|sub| sub.is_default)
                .cloned()
                .or_else(|| subscriptions.first().cloned());

            if default.is_none() {
                eprintln!("Could not find an active Azure subscription.");
            }

            default
        }
        Err(err) => {
            eprintln!("Failed to list Azure subscriptions: {err}");
            None
        }
    }
}
