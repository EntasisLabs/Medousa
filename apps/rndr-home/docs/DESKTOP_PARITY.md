# Desktop preview: reference destination parity matrix

Baseline: `a42c5236cd2239864d27aa9c151c3acd3e8191a0`.
This is a preview increment for independent review, not production replacement,
native-input qualification, permission expansion or integration approval.

## Inventory authority and status vocabulary

Inventory comes from production source at the baseline, not a new product map:

- `apps/medousa-home/src/lib/types/settings.ts`: all nine sections, labels,
  order, app/machine groups and mobile pager.
- `.../components/layout/SettingsPanel.svelte`, `.../settings/SettingsNav.svelte`
  and `Settings*Section.svelte`: actual settings destinations and host boundaries.
- `.../utils/navSurfaces.ts`, `lifeRailItems.ts`, `lmeExplorerModes.ts` and
  `.../components/layout/NavShell.svelte`: life rail/explorer doors and naming.
  Runtime and Messaging are settings-related, not new top-level product silos.
- `crates/medousa-sdk/src/{health,agents,runtime,mcp_gateway,local,recurring,reviews}.rs`,
  `crates/medousa-types/src/{daemon_api,local,mcp_gateway,review_provider}.rs`,
  `apps/medousa-home/src-tauri/src/daemon/{runtime,shared_mode}.rs`:
  existing service contracts, including the two read-only HTTP projections.

**Read-only wired** means a real existing endpoint is connected in code and tested
with loopback fixtures. It does **not** mean a real user workshop was contacted or
that live connectivity was demonstrated here. **Local preview** is window-local,
reversible, not production configuration. **Fixture** means explicitly synthetic
export evidence only. **Blocked** identifies a genuinely effectful or missing-host
boundary. **Deferred** is a port not implemented in this coherent pass; it is not
an IME blocker. Unsupported views explain their status instead of rendering fake lists.

## Shell and settings destinations

| Reference destination | Status in this increment | Working behavior / remaining limitation |
|---|---|---|
| Production Home | Preserved, unchanged | No replacement, build change or production mutation |
| Shared shell / rail / active destination | Local preview, partial visual parity | Stable shared rail, selected destination, top title/chrome, footer truth label, independent body scroll; chat stays mounted |
| Desktop settings master rail | Local preview | All sections, correct app/host groups, scrollable sidebar; narrow Previous/Next pager in reference order |
| Native titlebar/window controls, pane splits, shell tabs, notch, dynamic environment-driven life rail | Deferred | Existing rndr window/theme retained; no native chrome or complete pane-system parity claim |
| Preferences | Local preview + deferred | Working light/dark/high-contrast window settings; notifications, sounds, shortcuts, OS-following and persisted layout presets deferred |
| Accounts | Read-only wired + blocked/deferred | External account/runtime reports and hosted review provider discovery; provider credentials, OAuth, review-account linking/sign-out use existing app |
| Medousa Agent | Read-only wired + deferred | Cached model catalog, provider/model details; links to modes/local engine; no model selection/config writes, stance/charter/memory edits or provider probing |
| Runtime Controls | Read-only wired + blocked/deferred | Health/version/authority, local engine, runtime availability, worker capacity, schedules; shell/reach/SSH/computer/permission controls absent |
| Sharing (`network`) | Read-only wired + blocked/deferred | Shared-mode snapshot/details; phone/peer/seat/portal pairing and channel inventory not ported; no discovery or grants |
| External Agents (`connections`) | Read-only wired + blocked/deferred | Runtime list/filter/detail, availability/binary/account status; login/install/adopt/connect conversation operations absent |
| Packages | Blocked host service + deferred inventory | Usable host-boundary explanation with links to Sharing and Local engine; no pretend installed inventory or enabled installation control |
| MCP | Read-only wired | Gateway and server list/detail; disconnected stays disconnected; tools/enabling/credentials unavailable |
| Connection (`basement`) | Read-only wired + blocked/deferred | Responding workshop health/authority/version, existing explicit loopback/data-dir credential boundary; switching, linking vaults, start/stop and transport editing absent |

## Read-only administrative list/detail coverage

Every implemented dataset has explicit not-loaded/offline, loading, error,
permission-denied, empty, filtered/no-match, list, detail, Back and refresh states.
Filters are in-memory only. Reads are deduplicated while pending and generation-
fenced per resource; late Health data cannot populate a Models page. Refresh
hides earlier data until settlement. Snapshots are not continuous liveness.

| Destination / production model | Existing read | Safe projected fields / exclusions |
|---|---|---|
| Workshop / runtime descriptor | SDK health, `GET /v1/health` | Status, observed UTC, authority, version/build/contract/schema, deployment target, backend, tool count; no raw diagnostics |
| Models & providers | `GET /v1/models/catalog` | Provider and model ID; explicitly catalog-only, no credential/inference qualification; missing/malformed model fields error rather than fake empty success |
| External agent runtimes | SDK agents, `GET /v1/agents/runtimes` | Runtime, binary/availability/native-turn booleans, allowlisted signed_in/signed_out/unknown; no command, auth_detail or detail strings |
| Agent modes | SDK runtime, `GET /v1/agent-modes` | Mode ID, label, availability; not an agent roster or mode-changing control |
| MCP gateway & servers | SDK MCP, `GET /v1/mcp/gateway/status` | Reachability, invokes flag/count, server ID/title/enabled/connected/tools; no gateway URL, lanes, tokens, messages or invocation |
| Local engine | SDK local models, `GET /v1/local/engine/status` | Feature/loaded/phase/alias/backend; no model path, bind address, process log or start/stop |
| Schedules | SDK recurring, `GET /v1/recurring` | ID/name, enabled, cron, timezone, job type, queue, next/last status; no prompts, payloads, job execution or schedule writes |
| Shared workshop mode | Production existing `GET /v1/shared-mode` | Mode, enabled timestamp, profile IDs; no enable/disable or seat management |
| Worker capacity | Production existing `GET /v1/runtime/workers` | Validated integer capacities; no capacity writes or worker starts |
| Hosted review providers | SDK reviews, `GET /v1/reviews/providers` | Provider ID/label/protocol and problem count; raw diagnostics omitted; discovery is not account authentication |

Lists cap displayed rows at 500. Text is plain, control-filtered and bounded;
list titles abbreviate while details retain the sanitized value. Full response
bodies, credentials, commands and raw service errors are never placed in labels,
logs or public observation. Worker startup reuses `backend::client`; there is no
alternate credential reader. Auth errors are classified from the current SDK's
string-erased HTTP error (401/403); structured SDK status would be a future
improvement, not a backend rewrite in this increment.

## Other actual doors and explicit follow-on view matrix

These names are from the production life/explorer rail, not invented categories.
Each deferred rail destination opens a named port-status view with a Settings
return path; none simulates a connected service or offers fake effectful controls.
The rail is a static source-derived preview, not production environment discovery.

| Reference door / subview | Status | Follow-on scope / reason |
|---|---|---|
| Chat | Existing preview preserved; input-sensitive actions blocked | Send/Restore/New/Recent/Reload remain exactly gated; existing transcript, virtualization, recovery and Jump to latest tests retained |
| Agents | Deferred roster/detail | Do not misrepresent agent-mode registry as running agents; port existing workshop/bot roster and activity models |
| Activity | Deferred | Existing unified execution/activity feed and detail model |
| Scripts | Deferred | Existing LME script inventory/detail/editor; no arbitrary execution |
| Flows | Deferred | Existing workflow definitions/runs/details; keep execution separately authorized |
| Schedules | Read-only wired | Existing recurring list/detail implemented; runs/delivery details and management deferred |
| Code | Deferred | Project explorer/editing and governed work; no approvals/integration here |
| Reviews | Deferred review browser; provider inventory wired in Accounts | Existing prepared changes/repository-host review models; no approve/apply/merge |
| Notes | Deferred | Workshop-authoritative vault explorer/editor |
| Files | Deferred | Workshop-authoritative file inventory, typed preview/editing and locality rules |
| Artifacts | Deferred | Existing saved artifact list/presentation service |
| Create | Deferred | Existing creative project/document/asset service |
| Calendar | Deferred | Existing calendar event/list/detail service |
| Web | Deferred | Existing workshop/device browser authority and lifecycle |
| Context | Deferred | Existing memory/source graph and privacy model |
| Peers | Deferred read inventory; grants blocked | Existing paired/nearby models; pairing/trust/grants require existing app authorization |
| You | Deferred | Existing identity/profile models; no identity/config edits |
| Channels / Messaging (Sharing) | Deferred read inventory; writes blocked | Existing adapters/conversations, host package and secret boundaries; not a separate top-level Runtime/Messaging rail |
| Review accounts / provider keys / OAuth | Blocked effectful setup + deferred read account views | Reuse existing authorization/secure entry; never a fake secure input |
| Runtime SSH / shell reach / computer access | Deferred read targets + blocked changes | Existing target/reach models, grants and execution; not attempted here |
| Agent memory, custom voices, mode transition policy, retention/storage | Deferred | Existing charter/settings view models and save/authorization boundaries; avoid speculative persistence |
| Connection vault inventory / paired workshop selector | Deferred | Existing locality and workshop models; no local file picker for remote authority |
| Package inventory | Blocked missing Tauri host bridge in this port | Production package service is native-host inventory, not daemon model metadata; port read-only host adapter separately |

## Fixture-only and verification boundary

`--export-desktop DIRECTORY` cannot start a live worker: it forces offline mode
and asserts both chat/admin senders are absent. Its synthetic runtime/model/
schedule/MCP snapshots exist only in the exporter, never as a production fallback.
Every image has visible FIXTURE / SYNTHETIC GPU RENDER labeling. Default app state
has no such rows. Native observation remains Deny whenever any administrative
snapshot, non-chat route, read transport or filter input exists; no relaxation of
the existing chat fixture-only observation gate was made.

Physical IME, native window/input, accessibility, scaling and actual service
availability remain manual-verification gaps, not blanket navigation/settings
implementation blockers. See [implementation evidence](DESKTOP_IMPLEMENTATION.md).
