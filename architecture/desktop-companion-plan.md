# Desktop Companion — pet, toolbelt, and daemon client

## Decision

Turn Medousa's existing floating desktop toolbar into one animated companion.
The companion replaces the toolbar rather than creating a second floating
utility. Its expanded **toolbelt** keeps the toolbar's Chat, Note, Web, custom
view, and main-window launchers.

The companion is another client of `medousa_daemon`. It does not own a second
product model or bypass the daemon's tool, permission, budget, vault, or
filesystem authority.

## Experience

The desktop companion has three presentations:

1. **Pet** — a small draggable, transparent, always-on-top Medousa mark.
2. **Bubble** — a short status, completion, failure, or approval message.
3. **Toolbelt** — a compact panel opened by clicking the pet.

The pet reacts to actual work. It does not steal focus, speak without a useful
event, passively capture the screen or clipboard, or roam across the desktop.
Context capture is explicit and previewed before it is sent.

## Architecture

```text
companion window
  ├─ companion runtime ── Tauri daemon gateway ── medousa_daemon
  ├─ shared action catalog
  │    ├─ daemon actions
  │    ├─ desktop actions
  │    └─ main-window handoffs
  └─ desktop intent router ── main / chat / note / web / custom views
```

The initial slice keeps the internal `desktop-toolbar` window label so existing
Tauri configuration, tray commands, close handling, and saved window state keep
working. Product copy calls it the **Companion**.

Each WebView still has its own Svelte stores. Tauri owns the daemon transports
and broadcasts stream events to every client window. The first slice lets the
companion own streams for turns it starts and independently observes raw events
so it can react to work started elsewhere.

The next transport milestone replaces the implicit main-window owner/observer
relationship with app-level, reference-counted client subscription leases. The
first active client starts shared workspace/environment streams; hiding one
client cannot stop streams required by another.

## State model

| Situation | Companion state |
|---|---|
| Connected and quiet | `float` |
| Toolbelt opened or approval required | `attention` |
| Prompt accepted | `launch` |
| Turn or job active | `loading` / `surge` |
| Work completed | `success`, then `float` |
| Failure or disconnected daemon | `error` |
| User interruption | `recoil` |

Animation communicates lifecycle state; it is not random decoration. Reduced
motion remains authoritative.

## Tool model

The toolbelt exposes three conceptual entries:

- **Ask** — compact prompt composer and current/recent conversation choice.
- **Do** — searchable daemon and desktop actions.
- **Open** — handoff to full Medousa surfaces when the task needs richer UI.

The shared command catalog must eventually describe execution scope explicitly:

```ts
type CompanionActionScope = "daemon" | "desktop" | "main-window";
```

Commands that currently mutate main-window navigation stores must not be run
inside the companion WebView. They emit a desktop intent or open the appropriate
pop-out instead.

## Delivery

### Slice 1 — useful pet

- Replace floating toolbar chrome with the animated companion.
- Collapse to a tight pet-sized window and expand to a toolbelt.
- Send a prompt into the selected Medousa conversation.
- Animate launch, working, completion, failure, and approval states.
- Surface and resolve pending tool-round budget approvals.
- Preserve Chat, Note, Web, custom Views, and Main launchers.

### Slice 2 — independent client subscriptions

- Add app-level subscription leases for shared daemon streams.
- Reconcile snapshots whenever a client wakes or expands.
- Ensure the companion works when the main WebView is suspended or unhealthy.

### Slice 3 — shared action catalog

- Split daemon, desktop, and main-window commands.
- Add action search, prompts, risk labels, and availability checks.
- Add session creation/switching and compact recent history.

### Slice 4 — attention hub

- Agent permissions, blocked work, retries, cancellations, and job progress.
- Actionable completion/error bubbles with direct handoff to detail.
- Per-event notification and quiet-hour preferences.

### Slice 5 — explicit desktop context

- Clipboard and selected-text actions.
- File drop.
- Manual screenshot or region capture.
- Browser-page context through the existing browser client.

Local context is always labeled as local. Remote workshop filesystem work still
goes through the daemon; the companion never turns vault access into an upload
pipeline.

## Slice 1 exit criteria

- The pet does not leave a large invisible click-blocking rectangle when closed.
- A prompt can be sent without opening the main window.
- The pet reflects raw daemon activity from any Medousa client.
- Successful, failed, and approval-paused turns produce distinct feedback.
- A pending budget request can be approved or denied from the toolbelt.
- Existing floating-toolbar destinations remain reachable.
- Hiding/reopening from the tray preserves normal desktop behavior.
- Svelte checks and focused companion state tests pass with zero warnings.
