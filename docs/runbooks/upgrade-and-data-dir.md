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

### Session storage migration

After backing up and while the daemon/app is stopped, inventory legacy session
storage without changing it:

```bash
medousa session-storage
medousa session-storage --json
```

The command is dry-run by default. It writes a versioned report under
`session_migrations/h02-v1.json` in the resolved data directory. Names that are
malformed, ambiguous, link-backed, wrong-type, or collide with different
destination content are quarantined in the report and are never followed or
changed.

Apply only the unambiguous plan with:

```bash
medousa session-storage --apply
```

Migration copies through no-follow directory capabilities, verifies the
published content, journals each boundary, and retains the legacy source for
rollback. Re-running `--apply` resumes an interrupted planned copy. Inspect and
back up quarantined data; do not rename it into place while the daemon is
running.

### Other identifier-backed storage

Current releases also use opaque, versioned directory and file names for
profiles, feeds, components, packages, downloaded models, pairing records,
workshops, Forge items, manuscripts, skills, provider state, and Grapheme
scripts. The logical ID remains visible in the app and APIs; the opaque disk
name is an internal integrity boundary and should not be renamed to the logical
ID.

During the rollback window, Medousa reads a legacy direct-name entry only when
the name is cross-platform safe and the stored record proves that it belongs to
the requested logical ID. Ambiguous names produced by older character-
replacement sanitizers are left untouched. Back up these entries before an
upgrade and keep them for rollback; do not merge or rename two apparent aliases
by hand.

Manuscript YAML and skill directories created by older releases with readable
IDs remain supported during this window. Newly created or imported manuscripts
have opaque disk names; use the manuscript ID shown by Medousa rather than the
filename when invoking CLI or API operations.

---

## Vault roots

Vault content lives under configured roots (`GET /v1/vault/roots`). Adding a root does not move existing notes — set **active** root for default writes (`PUT /v1/vault/active`).

[cookbook/vault-and-library.md](../cookbook/vault-and-library.md)
