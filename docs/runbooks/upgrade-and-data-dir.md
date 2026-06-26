# Upgrade & data directory

**Audience:** operator

---

## Default paths

| Platform | Config | Data |
|----------|--------|------|
| macOS/Linux | `~/.config/medousa/` | `~/.local/share/medousa/` |
| Windows | `%APPDATA%\medousa\` | `%LOCALAPPDATA%\medousa\` |

`medousa doctor` prints resolved paths.

---

## `MEDOUSA_DATA_DIR`

Override the data root for multi-engine or portable installs:

```bash
export MEDOUSA_DATA_DIR=/var/lib/medousa-prod
medousa start daemon
```

Each engine instance gets isolated SurrealKV, vault roots, and session stores.

Plan: [data-dir-multi-engine-multi-vault-plan.md](../architecture/data-dir-multi-engine-multi-vault-plan.md)  
ADR: [adr-003-multi-workshop-connections.md](../architecture/decisions/adr-003-multi-workshop-connections.md)

---

## Multi-workshop (app)

The Medousa app maintains a **workshop registry** — multiple paired desktops. `workshops_set_active` switches the active engine.

Tauri: `workshops_load`, `workshops_set_active`, …

---

## Upgrade checklist

1. Stop daemon / app
2. Backup `MEDOUSA_DATA_DIR` (or default data path)
3. Install new binaries (`install.sh` or app update)
4. `medousa doctor` — verify health and paths
5. `medousa doctor --local-engine` if using offline brain

---

## Vault roots

Vault content lives under configured roots (`GET /v1/vault/roots`). Adding a root does not move existing notes — set **active** root for default writes (`PUT /v1/vault/active`).

[cookbook/vault-and-library.md](../cookbook/vault-and-library.md)
