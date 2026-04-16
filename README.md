# azac

Opinionated CLI for managing **Azure App Configuration** with **Key Vault** support.

## What this tool does

`azac` uses Azure CLI (`az`) to:
- configure an active context (subscription + App Configuration + app + label + key vault);
- list, inspect, create, update, and delete keys;
- promote/demote values between `plain` and `keyvault` reference types;
- export, plan, and import configuration in YAML;
- convert `.env` and `appsettings.json` files to the `azac` import format.

---

## How to install

### Prerequisites

- Azure CLI installed (`az`)
- authenticated session: `az login`
- access to App Configuration and Key Vault resources in your subscription

### Option 1: install via Cargo (crates.io)

```bash
cargo install azac
```

### Option 2: install from source

```bash
git clone https://github.com/luizfelmach/azac.git
cd azac
cargo install --path .
```

### Option 3: download release binary

Download the binary from **GitHub Releases** and add it to your `PATH`.

---

## How to use

### 1) Initial setup

```bash
azac setup
```

`setup` opens interactive prompts to select:
- App Configuration
- key separator (for example `:`)
- Application (prefix)
- Label
- Key Vault

> Tip: `azac sync` refreshes local Azure metadata cache used during setup.

### 2) Main commands

```bash
azac list
azac ls
azac show <KEY>
azac set <KEY> <VALUE>
azac set <KEY> <VALUE> --keyvault
azac delete <KEY1> [KEY2...]
azac promote <KEY>
azac demote <KEY>
```

### 3) Export, plan, and import

```bash
azac export config.yaml
azac plan config.yaml
azac import config.yaml
```

Accepted YAML format for import:

```yaml
MY_KEY:
  type: plain
  value: value
DB_PASSWORD:
  type: keyvault
  value: secret
OPTIONAL_KEY:
  type: prompt
  value: choose-during-import
```

Short form is also accepted:

```yaml
MY_KEY: value
```

### 4) Convert files to `azac` format

```bash
azac convert env .env > config.yaml
azac convert dotnet appsettings.json > config.yaml
```

---

## Quick command reference

- `azac setup` — configure active context
- `azac sync` — refresh subscriptions/appconfigs/keyvaults cache
- `azac list` / `azac ls` — list keys in current context
- `azac show <KEY>` — show one key
- `azac set <KEY> <VALUE> [--keyvault]` — set value
- `azac delete <KEYS>...` — delete keys
- `azac promote <KEY>` — convert `plain` to `keyvault`
- `azac demote <KEY>` — convert `keyvault` to `plain`
- `azac export <FILE>` — export YAML
- `azac plan <FILE>` — compare current state vs file
- `azac import <FILE>` — apply YAML
- `azac convert env <FILE>` — convert `.env`
- `azac convert dotnet <FILE>` — convert `appsettings.json`
