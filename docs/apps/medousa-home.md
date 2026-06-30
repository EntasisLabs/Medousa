# medousa-home — app integrator reference

**Audience:** integrator, contributor

Native desktop and mobile shell (Tauri v2 + SvelteKit). Product README: [../../README.md](../../README.md). Dev quickstart: [../cookbook/build-from-source.md](../cookbook/build-from-source.md).

## Surfaces

| Viewport | Shell | Primary surfaces |
|----------|-------|------------------|
| Desktop (>768px) | WorkshopShell | Chat, Work, Library, Workshop, Settings |
| Mobile (≤768px) | MobileShell | Pulse, Work, Chat, You (Library, Skills, Settings, …) |

Library on mobile includes **Notes** and **Presentations** tabs; presentations open fullscreen artifacts with safe-area chrome.

Active roadmap: [architecture/ROADMAP.md](../../architecture/ROADMAP.md).

## Transport stack

```
Svelte frontend
  → Tauri invoke
  → daemon/workshop_http.rs + daemon/sdk.rs
  → medousa-sdk MedousaClient + medousa-sdk-iroh WorkshopTransport
  → LAN / Iroh
```

- Typed artifact/runtime calls use `client.runtime().artifact_*()` ([`src-tauri/src/daemon/artifact.rs`](../../apps/medousa-home/src-tauri/src/daemon/artifact.rs)).
- JSON daemon traffic routes through [`workshop_http.rs`](../../apps/medousa-home/src-tauri/src/daemon/workshop_http.rs) and [`sdk.rs`](../../apps/medousa-home/src-tauri/src/daemon/sdk.rs) (`medousa-sdk-iroh` pooled transport).
- Interactive/workspace SSE uses Tauri event bridges. Reconnect discipline: [`src/lib/stream/reconnect.ts`](../../apps/medousa-home/src/lib/stream/reconnect.ts) — bounded backoff, overlap guard, `?since=<seq>` replay aligned with Rust/Python SDK helpers.
- `interactive_stream_start` may still fetch SSE bytes via legacy `workshop_transport` helpers internally; JSON paths use `medousa-sdk-iroh`.
- Stream types: [`scripts/gen-ts-types.py`](../../scripts/gen-ts-types.py) → `src/lib/types/generated/daemon_api.ts`.

See [SDK transports](../sdk/transports.md).

## Frontend store → API mapping

| Store | Daemon / Tauri | HTTP (when applicable) |
|-------|----------------|------------------------|
| `chat.svelte.ts` | `interactive_turn_send`, `interactive_stream_*`, `session_*` | `POST /v1/interactive/turn`, `GET …/stream` |
| `artifacts.svelte.ts` | `artifact_list_ui`, `artifact_fetch` | `POST /v1/runtime/artifact/list-ui`, `fetch` |
| `vault.svelte.ts` | `vault_*` | `/v1/vault/*` |
| `workspace.svelte.ts` | `workspace_stream_*`, `workspace_*_card` | `/v1/workspace/*` |
| `externalDesk.svelte.ts` | local FS + vault roots | `/v1/vault/roots`, active root |

## Tauri IPC command index (grouped)

### Daemon & connection

`daemon_url`, `set_daemon_url`, `daemon_health`, `daemon_start`, `daemon_restart`, `engine_diagnose`, `engine_clear_stale_lock`, `daemon_wait_healthy`, `workshop_ensure_engine`, `connection_load_prefs`, `connection_set_public_bind`, `connection_set_autostart`

### Pairing & workshops

`pairing_fetch_qr`, `pairing_rotate_invite`, `pairing_fetch_qr_image`, `pairing_fetch_status`, `pairing_revoke`, `pairing_wait_ready`, `pairing_complete_from_qr`, `pairing_load_credentials`, `pairing_send_heartbeat`, `bonjour_status`, `workshops_load`, `workshops_set_active`, `workshops_add_local`, `workshops_rename`, `workshops_remove`, `workshops_update_client_state`, `workshops_update_branding`

### Interactive chat & sessions

`interactive_turn_send`, `interactive_stream_start`, `interactive_stream_stop`, `interactive_stream_stop_turn`, `session_list`, `session_set_display_name`, `session_delete`, `session_get_history`, `session_get_active_turn`, `session_cancel_active_turn`, `turn_create`, `turn_list_session`

### Workspace & jobs

`workspace_stream_start`, `workspace_stream_stop`, `workspace_get_card`, `workspace_fetch_snapshot`, `workspace_cancel_card`, `workspace_archive_card`, `workspace_retry_card`, `job_get_result`, `job_enqueue_ask`, `job_complete_actions`, `job_archive_ask`

### Vault

`vault_list_notes`, `vault_list_tags`, `vault_list_roots`, `vault_set_active_root`, `vault_add_root`, `vault_get_note`, `vault_save_note`, `vault_create_note`, `vault_delete_note`, `vault_search`, `vault_backlinks`

### Artifacts

`artifact_command`, `artifact_fetch`, `artifact_list_ui`

### Runtime, budget, recurring

`runtime_get_stats`, `runtime_get_defaults`, `runtime_get_tui_defaults`, `runtime_put_tui_defaults`, `runtime_config_command`, `runtime_stage_route_command`, `runtime_get_delivery_status`, `runtime_get_continuation_status`, `turn_budget_approve`, `turn_budget_deny`, `turn_budget_list`, `recurring_list`, `recurring_register_prompt`, `recurring_update`, `recurring_delete`, `recurring_list_runs`, `recurring_get_delivery`

### Catalog, capabilities, identity, locus

`catalog_list_manuscripts`, `catalog_get_manuscript`, `catalog_update_manuscript`, `catalog_import_manuscripts`, `catalog_list_capabilities`, `catalog_get_capability`, `catalog_reindex_capabilities`, `identity_get_context`, `identity_list_profiles`, `identity_create_profile`, `identity_set_active_profile`, `identity_remember`, `identity_digest_preview`, `identity_export_markdown`, `locus_list_nodes`, `locus_list_tags`, `locus_get_node`

### Grapheme, media, MCP

`grapheme_*`, `media_upload`, `media_upload_path`, `mcp_gateway_*`, `capabilities_*`

### Messaging & paths

`messaging_load_product_config_summary`, `messaging_save_channel_config`, `messaging_secret_status`, `messaging_save_secret`, `messaging_clear_secret`, `medousa_config_paths`, `connection_runbook_path`, `load_tui_defaults`, `persist_tui_defaults`, …

Full list: [`src-tauri/src/lib.rs`](../../apps/medousa-home/src-tauri/src/lib.rs) `generate_handler!` block.

## Mobile development

iPhone on Mac: [`MOBILE-DEV.md`](../../apps/medousa-home/MOBILE-DEV.md). Operator guide: [mobile-and-lan cookbook](../cookbook/mobile-and-lan.md).
