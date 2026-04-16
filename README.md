# azac

Opinionated CLI para gerenciar **Azure App Configuration** com suporte a **Key Vault**.

## O que a ferramenta faz

O `azac` usa o Azure CLI (`az`) para:
- configurar um contexto ativo (subscription + App Configuration + app + label + key vault);
- listar, consultar, criar, atualizar e remover chaves;
- promover/demover valores entre `plain` e referência de `keyvault`;
- exportar, planejar e importar configurações em YAML;
- converter arquivos `.env` e `appsettings.json` para o formato de importação do `azac`.

---

## How to install

### Pré-requisitos

- Azure CLI instalado (`az`)
- sessão autenticada: `az login`
- acesso aos recursos de App Configuration e Key Vault na subscription

### Opção 1: instalar via Cargo (crates.io)

```bash
cargo install azac
```

### Opção 2: instalar a partir do código-fonte

```bash
git clone https://github.com/kauanmodolo/azac.git
cd azac
cargo install --path .
```

### Opção 3: baixar binário de release

Baixe o binário em **GitHub Releases** e adicione ao seu `PATH`.

---

## How to use

### 1) Configuração inicial

```bash
azac setup
```

O `setup` abre prompts interativos para escolher:
- App Configuration
- separador de chave (ex: `:`)
- Application (prefixo)
- Label
- Key Vault

> Dica: `azac sync` atualiza o cache local de metadados do Azure usado durante o setup.

### 2) Comandos principais

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

### 3) Exportar, planejar e importar

```bash
azac export config.yaml
azac plan config.yaml
azac import config.yaml
```

Formato YAML aceito no import:

```yaml
MY_KEY:
  type: plain
  value: valor
DB_PASSWORD:
  type: keyvault
  value: segredo
OPTIONAL_KEY:
  type: prompt
  value: escolha-no-import
```

Também aceita forma curta:

```yaml
MY_KEY: valor
```

### 4) Conversão de arquivos para o formato `azac`

```bash
azac convert env .env > config.yaml
azac convert dotnet appsettings.json > config.yaml
```

---

## Referência rápida de comandos

- `azac setup` — configura contexto ativo
- `azac sync` — atualiza cache de subscriptions/appconfigs/keyvaults
- `azac list` / `azac ls` — lista chaves do contexto atual
- `azac show <KEY>` — exibe uma chave
- `azac set <KEY> <VALUE> [--keyvault]` — define valor
- `azac delete <KEYS>...` — remove chaves
- `azac promote <KEY>` — converte `plain` para `keyvault`
- `azac demote <KEY>` — converte `keyvault` para `plain`
- `azac export <FILE>` — exporta YAML
- `azac plan <FILE>` — compara estado atual vs arquivo
- `azac import <FILE>` — aplica YAML
- `azac convert env <FILE>` — converte `.env`
- `azac convert dotnet <FILE>` — converte `appsettings.json`
