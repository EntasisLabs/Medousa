# Application core (chat + shell)

Medousa keeps two contracts:

1. **Daemon API** (`medousa-types` / generated ops) — durable workshop authority.
2. **Application API** (`medousa-app-core`) — canonical client projections.

Domains today:

- **Chat** — timeline, live↔persisted reconciliation, turn/worker identity.
- **Shell layout** — desktop/tab/pane identity + focus (`ShellCore`). Persistence
  I/O and LME/browser ports stay in Home; the core owns the layout graph.
- **Vault buffers** — path-keyed unsaved identity + dirty/conflict reconcile
  (`VaultBufferCore`). Filesystem bytes stay daemon authority; CodeMirror paint
  stays in Home.

Renderers consume snapshots. They do not own reconciliation or causal identity.

See `crates/medousa-app-core/README.md` for regenerate commands and host wiring.
