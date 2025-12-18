pub mod cfg {
    pub fn list_configs() {
        unimplemented!()
    }

    pub fn use_config(_name: &str) {
        unimplemented!()
    }

    pub fn show_config(_name: &str) {
        unimplemented!()
    }

    pub fn set_separator(_separator: &str) {
        unimplemented!()
    }

    pub fn show_current_config() {
        unimplemented!()
    }
}

pub mod app {
    pub fn list_apps() {
        unimplemented!()
    }

    pub fn use_app(_name: &str) {
        unimplemented!()
    }

    pub fn show_app(_name: &str) {
        unimplemented!()
    }

    pub fn set_label(_label: &str) {
        unimplemented!()
    }

    pub fn set_keyvault(_vault: &str) {
        unimplemented!()
    }

    pub fn show_current_app() {
        unimplemented!()
    }
}

pub mod kv {
    use std::path::Path;

    use clap::ValueEnum;

    #[derive(Clone, Debug, ValueEnum)]
    pub enum ExportFormat {
        Json,
        Yaml,
        Toml,
    }

    pub fn list_keys() {
        unimplemented!()
    }

    pub fn show_key(_key: &str) {
        unimplemented!()
    }

    pub fn set_key(_key: &str, _value: &str, _use_keyvault: bool) {
        unimplemented!()
    }

    pub fn delete_key(_key: &str) {
        unimplemented!()
    }

    pub fn export_entries(_format: ExportFormat) {
        unimplemented!()
    }

    pub fn import_entries(_path: &Path) {
        unimplemented!()
    }
}
