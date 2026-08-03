# Browser Companion — first-class Medousa surface

## Decision

Build the Chromium browser companion next, targeting Chrome and Edge through a
Manifest V3 side panel. Treat it as a full integration like VS Code and
Obsidian, not as a channel adapter.

The product model is:

> The current tab is context. The side panel is the conversation. Medousa
> proposes or performs the next useful step with the user's explicit intent.

Firefox and Safari should reuse the host-neutral TypeScript surface after the
Chromium slice settles. Firefox has a native WebExtension sidebar variant;
Safari needs separate Web Extension packaging and signing.

## Scope of the first slice

### Included

- side-panel chat with the shared daemon session and SSE contract;
- page title, URL, selection, and bounded readable text context;
- toolbar, keyboard shortcut, and selection/page context-menu entry points;
- restore, search, switch, rename, delete, and new-conversation flows;
- copy, stop, retry, budget approval, permission approval, and workshop handoff
  recovery;
- local daemon connection plus explicit remote workshop configuration;
- privacy-first page capture: no persistent content script and no default
  `all_urls` permission.

### Deliberately deferred

- client-executed browser actions (`click`, typing, navigation, and DOM
  mutation) and their approval UI;
- automatic page indexing or passive browsing history capture;
- inline DOM mutation, form filling, or submission;
- Firefox and Safari packaging;
- vault save/clip actions until the browser context and permission model have
  been exercised in real browsing workflows.

## Runtime boundaries

```mermaid
flowchart LR
  Tab[Current browser tab] -->|explicit capture| Panel[Medousa side panel]
  Menu[Toolbar / shortcut / context menu] --> Worker[MV3 service worker]
  Worker -->|pending context + open panel| Panel
  Panel --> Client[@medousa/client]
  Client --> Daemon[medousa_daemon]
  Panel -->|stream + history reconciliation| Client
  Daemon -->|queued client-tool request| Client
  Client -->|snapshot result| Daemon
```

- The side panel owns the foreground SSE stream because MV3 service workers
  are event-driven and may be unloaded.
- The service worker owns only short-lived browser events, context-menu setup,
  and pending context handoff.
- The daemon remains authoritative for sessions, history, identity, and
  workshop work.
- The first slice sends `supports_browser_host=false` and registers one
  read-only `browser_page_snapshot` client tool. The daemon exposes it only to
  `channel_surface="browser"` turns, then routes each invocation through a
  pull-based request/result queue so the extension never needs an inbound
  listener.
- Browser actions stay outside the first slice. They will reuse this bridge but
  add explicit effect classification, approval, and replay-safe UI affordances.

## Permission posture

- `activeTab` + `scripting` for user-invoked page capture;
- `storage` for endpoint, token, active session, and one-shot context handoff;
- `sidePanel` and `contextMenus` for native entry points;
- localhost daemon origins in the base manifest;
- one remote workshop origin requested at connection-save time.

## Follow-on sequence

1. Dogfood the Chrome/Edge unpacked build and polish capture, handoff, and
   connection recovery.
2. Add approval-aware browser actions over the client-tool bridge.
3. Add Firefox sidebar packaging.
4. Add Safari Web Extension packaging.
5. Decide whether browser-host tools should be exposed through a separate,
   explicit client-executed capability.
