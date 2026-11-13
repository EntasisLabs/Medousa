# Desktop follow-on: shell, Settings and Activity coverage

Baseline: `9d51646838cba3571eaec994d5919ef7b2bf6be2`, clean at admission.
This matrix supersedes the **current-status claims** in `DESKTOP_PARITY.md` for
this follow-on only. Prior reports, commits and evidence remain unchanged.
This is an app-local preview, not production completion or integration approval.

## Status meanings and reference authority

- **Functional read-only (wired)**: real existing production GET contract in code,
  exercised only against ephemeral loopback fixture servers here. No real daemon,
  provider, credential or configuration operation was performed.
- **Local preview**: reversible window-local navigation, appearance and state;
  no production configuration changes or persistence.
- **Deferred**: no functional port; a named explanation is not a working route.
  A deferred route is not automatically an IME blocker.
- **Fixture**: synthetic offline exporter data only, never a live success fallback.

Reference sources at the admitted baseline:
`apps/medousa-home/src/lib/types/settings.ts`,
`components/settings/SettingsNav.svelte`, `components/layout/NavShell.svelte`,
`utils/lifeRailSections.ts`, `utils/lifeRailItems.ts`,
`spotlight/activityApi.ts`, `spotlight/activityData.svelte.ts` (all under its `src/lib`),
plus `crates/medousa-sdk/src/workspace.rs`,
`crates/medousa-types/src/daemon_api.rs` and `src/workspace/{card,service}.rs`.
The production Activity source combines workspace cards with other data. This
increment deliberately implements **one bounded workspace work-card snapshot**,
not a fabricated universal jobs endpoint or complete durable queue browser.

## Shell and Settings

| Surface | Classification | Implemented / remaining difference |
|---|---|---|
| Primary rail + fixed dock | Local preview, partial reference grouping | Primary doors scroll separately; Focus and Automations group headings; Activity/Settings/You dock remains reachable at 650px height. Selected destination has an explicit semantic name and visual state. Static text shortcuts remain, not the production dynamic icon rail |
| Native panes, tabs, titlebar, notch, workspace switching | Deferred | No framework/window-system changes or claim of exact native chrome |
| Automation grouping | Local preview, partial | Scripts/Flows/Schedules retain separate preview shortcuts under Automations; production folds them behind its explorer. Scripts and Flows remain deferred |
| Settings chooser | Local preview | All nine sections directly available at both widths; Medousa/This host groups, current-view names, last section remembered in this window. Stable anchor survives responsive rebuilds. Escape/outside dismissal and focus return use the existing popup scope |
| Chooser keyboard order | Local preview, tested app input | Tab/Shift-Tab cycles close + nine sections; arrows and Home/End navigate sections; Enter/Space use existing button activation. Ordinary single navigation keys are mapped in section order independent of recycled entity IDs; mixed/synthetic/modified/composition batches are not intercepted. Physical keyboard and native accessibility bridge remain human checks |
| Settings sidebar/pager | Local preview | Existing grouped wide sidebar and narrow Previous/Next retained; section labels announce current view and hints |
| Preferences | Local preview + deferred | Reversible light/dark/high-contrast window controls. Notifications, persistence, OS-following and layout presets remain deferred |
| Accounts | Functional read-only subset + deferred | Runtime/account availability and review-provider discovery. Not complete account panels; sign-in/out, review accounts, OAuth/keys remain in production app |
| Medousa Agent | Functional read-only subset + deferred | Cached model catalog, modes/local-engine links. No charter, memory, model-selection, voice or provider writes |
| Runtime Controls | Functional read-only subset + deferred | Health/version/authority, engine/runtime/capacity/schedule views. Shell/reach/SSH/computer access not ported |
| Sharing | Functional read-only subset + deferred | Shared-mode snapshot. Seats, phone/peer/portal pairing, channel inventory and permissions deferred |
| External Agents | Functional read-only subset + deferred | Runtime list/filter/details with safe account status. No login/install/adopt/conversation connect |
| Packages | Deferred host-boundary page | Missing native inventory adapter; no fake inventory or installs |
| MCP | Functional read-only subset | Gateway/server status. No invoke, enable, credentials or server configuration |
| Connection | Functional read-only subset + deferred | Health/authority/version. No daemon start/stop, switching, vault linking or credential editing |

These are nine **read-only/local-preview destinations**, not nine editable or
production-complete Settings panels. Current-view accessible names and existing
button/focus semantics are implementation evidence, **not screen-reader certification**.

## Functional read-only datasets

| Dataset | Existing contract | Safe coverage / exclusions |
|---|---|---|
| **Activity · jobs snapshot (new)** | SDK workspace `GET /v1/workspace/cards?limit=100&include_terminal=true` | Latest up to 100 work cards, including terminal cards; locally numbered generic title, controlled board-state enum, card ID, created/updated UTC, snapshot revision. Detail reuses exactly this snapshot without a second network read. Prompt-derived title, raw status prose, outputs, prompts, diagnostics, payloads, worker transcript, URLs and associations are not projected. Board state is not execution-success proof |
| Workshop status | `GET /v1/health` | Existing sanitized authority/version/observed metadata; no diagnostics |
| Models/providers | `GET /v1/models/catalog` | Cached catalog, not inference/credential proof |
| External runtimes | `GET /v1/agents/runtimes` | Availability and allowlisted account status, no command/auth detail |
| Modes | `GET /v1/agent-modes` | Registry, not running-agent roster |
| MCP | `GET /v1/mcp/gateway/status` | Gateway/server status only, not tool invocation |
| Local engine | `GET /v1/local/engine/status` | State metadata, no paths/bind/log/process controls |
| Schedules | `GET /v1/recurring` | Existing sanitized recurring metadata, not execution or editing |
| Shared mode | `GET /v1/shared-mode` | State metadata only |
| Worker capacity | `GET /v1/runtime/workers` | Validated capacities only |
| Review providers | `GET /v1/reviews/providers` | Discovery, not authenticated accounts |

All eleven models reuse the closed resource-enum GET-only worker and existing
client/authorization boundary. Activity explicitly caps both its query and local
projection at 100; other models retain their 500-row display cap. No generic write,
job enqueue/cancel/retry/approve/archive or administrative command channel was added.

Every dataset keeps not-loaded/offline, loading, permission, incompatible,
unavailable, disconnected, empty, no-match, list and metadata-detail states distinct.
Fixture/live-read/retained-offline snapshot labels do not imply continuous liveness.
Refresh deduplicates, serial-fences stale replies and clears index-based detail
selection before new data. Explicit filter/navigation starts at the top; Back
restores the results viewport and row focus (canonical focus visibility can reveal
the row). Same-view resize/refresh and unrelated updates retain state as before.
No runtime fixture fallback, raw error rendering or secret/config editor exists.

## Actual shell doors: no landing-page inflation

There are still **18 preview doors**. Chat, Settings, Schedules and now Activity
have working views: **14/18 remain deferred**. Activity here is only the bounded
read-only subset described above, not full global Activity parity.

| Door / view | Classification | Remaining scope |
|---|---|---|
| Chat | Existing preview preserved | Send/Restore/New/Recent/Reload remain denied; draft/session/scroll/virtualization/Jump to latest preserved |
| Settings | Local preview + functional read-only subsets | Complete editable settings and host services not claimed |
| Schedules | Functional read-only | Runs/delivery/management deferred |
| Activity | Functional read-only bounded subset | Unified feed, assignment/agent activity, durable runtime job ledger, richer details and live subscriptions deferred |
| Code, Reviews | Deferred | Project/review browsing/editing and governed actions not implemented |
| Notes, Files, Artifacts, Create | Deferred | Authoritative inventory/editing/presentation ports not implemented |
| Calendar, Web, Context, Peers | Deferred | Existing services and locality/grant boundaries still required |
| Agents | Deferred | Actual roster/detail is not the mode registry |
| Scripts, Flows | Deferred | Definition/run/editor views not implemented |
| You | Deferred | Identity/profile views and persistence not implemented |
| Channels/Messaging, accounts/pairing, package inventory, connection selector | Deferred nested views | Existing host/permission/locality/secure-entry contracts remain necessary |

## Evidence and final human checklist

See [exact verification and inspected-pixel ledger](shell-activity-evidence/REPORT.md).
Tests cover navigation, chooser keyboard/focus, selection/Back/scroll/filter,
responsive anchor stability, stale/unrelated updates and existing chat isolation.
GPU exports are real rendered pixels with synthetic data; theme/width combinations
actually inspected are listed explicitly, not inferred from export success.

Final human verification (deferred, **not blanket blockers** on further safe work):

1. On the intended devices, traverse chooser/sidebar/docked rail with the physical
   keyboard, test focus visibility and dismissal, screen-reader announcements,
   scaling, narrow/minimum-height windows, pointer/scroll and high contrast.
2. In an explicitly authorized live environment, verify responding workshop identity,
   offline/error/permission/empty behavior, real work-card ordering and timestamp
   meaning, refreshing while reading details, and locality/privacy. This worker did
   not connect, discover credentials, start/repair/configure daemons or probe providers.
3. Keep the five input-sensitive actions denied. Physical IME/native-emptiness
   qualification and any rndr repair belong to their separately authorized track.
   No framework/pin/root dependency change or production-ready assertion follows.

Independent review, any corrections and native integration remain the sender's
responsibility. This worker neither accepts its own work nor integrates it.
