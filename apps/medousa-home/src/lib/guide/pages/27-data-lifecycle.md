# Data locations, backup, and retention

Where workshop data lives, what you can back up, and what gets cleaned automatically.

Related: [Vault trash and versions](guide:vault-recovery) · [Workshops and connections](guide:workshops-connections) · [Settings reference](guide:settings-reference)

## Paths (desktop)

Settings → **Workshop** → **More on this device** → **Files & diagnostics** shows resolved paths. Typical layout under the engine data directory:

| What | Typical location |
|------|------------------|
| Engine data | OS app data / `medousa` (or `MEDOUSA_DATA_DIR`) |
| Vault | `{dataDir}/vault` |
| Charter / models / retention | `{dataDir}/tui_defaults.json` |
| Workshops registry | `{dataDir}/workshops.json` |
| Connection / autostart | `{dataDir}/connection_prefs.json` |
| Wizard state | `{dataDir}/wizard.json` |
| Pairing secrets | `{dataDir}/pairing_credentials.json` |
| Product / channels config | `{dataDir}/product_config.json` |
| Capabilities | `{configDir}/capabilities.toml` |
| MCP gateway | `{configDir}/mcp-gateway.toml` |

Sessions and artifacts live under the engine data tree (daemon-owned). Exact OS paths appear only after the desktop app resolves them.

## Backup habits

| Asset | How |
|-------|-----|
| Vault notes | Copy `{dataDir}/vault`, use Versions snapshots, or export PDF/Word per note |
| Layout / views | Settings → Sharing → **Canvas backup & send** (Rename / Skip / Overwrite on import) |
| Profile identity | You → overflow **Export profile backup** |
| Workshop switch | Separate `dataDir` per local engine — removing a workshop entry is not always “delete disk” |

## Retention and cleanup

| Knob | Where | Default / range |
|------|-------|-----------------|
| Work cards hide after done | Preferences → Work cards | 24 hours (1–168) |
| Work archives wipe | Preferences → Work cards | 7 days (1–90) |
| Presentations cleanup | Medousa Agent → Presentations | Configurable age / max per session |
| Trash | Library → Trash → Restore | List capped in UI (~80) |
| Versions history | Vault Versions panel | Git log capped in UI (~40) |

## Migration and wizard

- Existing installs may see **Welcome back** instead of a blank first-run — [Getting started](guide:getting-started#migration-welcome-back).
- **Re-run welcome wizard:** Workshop → More → Welcome wizard (desktop) for model choice / optional phone.
- Changing data dir / redirect files is advanced — use Files & diagnostics and the connection runbook; do not move folders while the engine is writing.

```callout
tone: warning
title: Engine restart vs delete
body: Restart pauses chats. Removing a workshop from the switcher or wiping a data directory can destroy vaults — confirm paths before deleting anything in Finder.
```

Next: [Troubleshooting](guide:troubleshooting) · [Known limits and FAQ](guide:faq-limits).
