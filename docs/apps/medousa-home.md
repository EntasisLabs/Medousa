# medousa-home — app integrator reference

**Audience:** integrator, contributor

Native desktop and mobile shell (Tauri v2 + SvelteKit). Product README: [../../README.md](../../README.md). Dev quickstart: [../cookbook/build-from-source.md](../cookbook/build-from-source.md).

## Interactive stream boundary

Home explicitly negotiates the typed turn-stream v2 media type. The native
bridge emits the decoded `TurnStreamEnvelopeV2` value, and the webview performs
sequence dedupe plus frame-batched coalescing on that generated union before
updating reactive chat state. Content and reasoning are coalesced only while
adjacent; switching lanes or receiving a tool, approval, reset, or terminal
variant flushes the previous batch first.

During the v1 support window, an older workshop may still return the frozen v1
DTO. Home converts that payload once at the ingress compatibility seam. Current
v2 traffic does not allocate that conversion. The mature chat reducer still
uses the generated v1 projection after batching; removing that final reducer
projection is tracked by H03 migration and does not change the wire contract.

## Surfaces

| Viewport | Shell | Primary surfaces |
|----------|-------|------------------|
| Desktop (>768px) | WorkshopShell | Chat, Work, Notes, Files, Artifacts, Workshop, Settings |
| Mobile (≤768px) | MobileShell | Pulse, Work, Chat, You (Library, Skills, Settings, …) |

Library on mobile includes **Notes** and **Artifacts** tabs; artifacts open fullscreen with safe-area chrome.

Active roadmap: [architecture/ROADMAP.md](../../architecture/ROADMAP.md).

## Runtime boundaries

Startup contains boot, connection, platform selection, layout primitives, and
eager chat. Desktop versus mobile composition is chosen before that shell graph
imports. Vault, code/work, browser, settings, spotlight, wizard, exporters, and
rich renderers load on real user or restored-state intent through
[`src/lib/runtime/features/`](../../apps/medousa-home/src/lib/runtime/features/)
and [`viewLoaders.ts`](../../apps/medousa-home/src/lib/runtime/viewLoaders.ts).

Allowed dependency direction ([ADR-020](../architecture/decisions/adr-020-feature-boundaries-and-lazy-runtime.md)):

- Catalog/descriptors import no stores or `.svelte` implementations.
- Feature stores do not import sibling feature stores.
- Dynamic `import()` is a load boundary, not a way to hide a cycle.
- Chat stays eager; overlays listed in `APP_SHELL_LAZY_OVERLAYS` stay out of the
  static closure.
- Feature CSS loads with its entry. `app.postcss` declares cascade layers and
  does not `@import` feature sheets. Theme tokens are one stored
  `/themes/<name>.css` sheet, not a compiled 50-palette Tailwind tree.

Enforcement (home CI): `npm run check` (includes `check:runtime-graph`),
`npm run test:h09`, `npm run build`, `npm run check:bundle-budget`.

H09 code trains emptied the ARCH-001/ARCH-002 unit/CI ledgers: `crossStoreEdges`
is `[]` and no source owner is above 2,000 lines. ARCH-001 and ARCH-002 are
Mitigated on unit/CI exit tests. FRONT-001 stays Proposed: root CSS is still
above 600 KiB. Validated/Shipped still need P08 packaged multi-OS evidence.

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
| `environment.svelte.ts` | `environment_get_spec`, `environment_stream_*`, `environment_get_status` | `/v1/environment/*` |

## Environment / Canvas

Custom views are agent-built surfaces pinned in nav. The **environment store** (`environment.svelte.ts`) holds:

- `spec` — surfaces, layout presets, components, `theme`
- `pendingProposal` — operator approval queue
- `feedStateByComponentId` — live feed patches for presentation widgets
- `canvasStatus` — doctor-shaped status for Settings → Canvas

**UI entry points:**

| Location | Behavior |
|----------|----------|
| Left master rail (`NavShell.svelte`) | Visible destinations or view lists; fully hideable from content headers |
| Settings → Canvas (`SettingsCanvasSection.svelte`) | Preset switcher, pending apply/dismiss, per-surface status |
| Mobile More → My views (`MoreHub.svelte`) | Custom surfaces from active preset |
| `EnvironmentRenderer.svelte` | Renders custom vs builtin; `PresentationFrame` for HTML widgets |

**Theming:** `spec.theme` is environment-first; widgets receive `--medousa-host-*` CSS vars via `artifactPrepareHtml.ts`. Workshop Room theme is fallback.

Cookbooks: [Custom views](../cookbook/custom-views-and-canvas.md) · [Advanced](../cookbook/environment-canvas-advanced.md)

## Tauri IPC command index (grouped)

### Daemon & connection

`daemon_url`, `set_daemon_url`, `daemon_health`, `daemon_start`, `daemon_restart`, `engine_diagnose`, `engine_clear_stale_lock`, `daemon_wait_healthy`, `workshop_ensure_engine`, `connection_load_prefs`, `connection_set_public_bind`, `connection_set_autostart`

### Pairing & workshops

`pairing_fetch_qr`, `pairing_rotate_invite`, `pairing_fetch_qr_image`, `pairing_fetch_status`, `pairing_revoke`, `pairing_update_policy`, `pairing_wait_ready`, `pairing_complete_from_qr`, `pairing_load_credentials`, `pairing_send_heartbeat`, `bonjour_status`, `workshops_load`, `workshops_set_active`, `workshops_add_local`, `workshops_rename`, `workshops_remove`, `workshops_update_client_state`, `workshops_update_branding`

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

### Native browser request ownership

Desktop browser content is a hostile-origin webview class. Its capability
matches only `browser-content-embed-*` and `browser-content-popout`, grants one
`browser-bridge:allow-report` permission, and does not grant application, core,
event, window, webview, path, resource, menu, tray, opener, or general plugin
commands. Lifecycle state—URL, title, navigation generation, popups, and
downloads—comes from native webview hooks rather than page-supplied routing.

Snapshot, action, navigation-state, and find calls are correlated by a native
request ID, the actual injected webview label, embed/pop-out surface, response
kind, and navigation generation. Overlapping calls may complete in any order.
Evaluation failure, timeout, caller drop, navigation, tab replacement, control
takeover, close, and shutdown revoke the matching pending authority; late or
mismatched callbacks cannot consume a sibling request.

Snapshots serialize at most 128 KiB of inert DOM-shaped text without first
materializing `outerHTML`; callback transport is capped at 512 KiB and exposes a
`truncated` bit. Agent actions are bounded and allow only ordinary
click/type/press/scroll/select/wait operations under the current agent-control
lease. Credential/file/payment inputs, sensitive forms, downloads, active or
external schemes, popups, and permission-affecting behavior fail closed.

`human_browser_request_diagnostics` exposes payload-free broker counts for
pending/high-water, matched, late-or-unsolicited, wrong-kind, wrong-surface,
stale-navigation, cancelled, oversize, and capacity-rejected outcomes.

### Trusted shell CSP and local resources

The trusted shell ships with an explicit CSP: scripts are bundled/self-only,
with the narrower `wasm-unsafe-eval` permission reserved for the bundled HEIC
decoder; general JavaScript `unsafe-eval` remains denied. Objects and embedding
are denied, and connection/image/frame/worker sources are listed by feature.
CSP diagnostics record only the effective directive and a source class; they
never log the blocked URL, query, local path, or payload.

The Tauri asset protocol is disabled. Co-located vault image previews use
`authorized_resource_admit` followed by the one-use
`authorized_resource_read`. Admission accepts only a strict vault-relative path
that the daemon opens under vault authority, safe raster MIME types, and at most
8 MiB. The opaque ID is bound to the requesting trusted webview, expires after
two minutes, and is consumed by the read. SVG, HTML, PDF, arbitrary absolute
paths, and non-vault files are not inline preview resources. Remote workshops
continue to fetch vault bytes from their authenticated daemon.

The reviewed inventory is
[`browser-authority-inventory.json`](../../apps/medousa-home/src-tauri/security/browser-authority-inventory.json).
`npm run check:browser-capabilities` freezes all application commands, the
report-only plugin ACL, concrete label classes, CSP, asset-protocol state, and
locked Tauri/Wry versions. CI runs it on Linux, macOS, and Windows; release jobs
run it again immediately before packaging.

## Mobile development

iPhone on Mac: [`MOBILE-DEV.md`](../../apps/medousa-home/MOBILE-DEV.md). Operator guide: [mobile-and-lan cookbook](../cookbook/mobile-and-lan.md).
