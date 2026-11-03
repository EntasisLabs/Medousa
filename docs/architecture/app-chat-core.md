# Application chat core (first slice)

Medousa keeps two contracts:

1. **Daemon API** (`medousa-types` / generated ops) — durable workshop authority.
2. **Application API** (`medousa-app-core`) — canonical client chat timeline.

Renderers (Svelte today) consume snapshots from the application core. They do
not own live↔persisted reconciliation, turn identity, or tool/agent causal
links.

See `crates/medousa-app-core/README.md` for regenerate commands and host wiring.
