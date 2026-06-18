# Home app — connection reliability

Forensic notes and sprint plan for SSE / workshop lifecycle bugs in `medousa-home`.

## Problem

Health checks can stay green while workspace and interactive SSE pipes are dead. Several code paths also call **global** stream stop when they mean **one turn** or **view teardown only**, leaving JS `streamOwners` out of sync with Rust slots.

Symptoms: frozen workspace board, chat stuck mid-stream, popout close killing main-window turns, resume from background with stale pipes.

---

## Sprint A — Pipes stay alive *(done)*

| # | Item | Status |
|---|------|--------|
| A1 | Rust SSE: emit error on unexpected EOF (not on cancel) | Done |
| A2 | JS: auto-reconnect workspace stream on error (backoff) | Done |
| A3 | JS: reattach interactive streams on SSE error | Done |
| A4 | Foreground `resumeWorkshop` restarts workspace + reattach | Done |
| A5 | `reconnectWorkshop` awaits stream teardown; sync owners | Done |
| A6 | Popout unmount: unlisten only (no global stream stop) | Done |
| A7 | `cancelActiveTurn`: scope to active turn, not scorched earth | Done |

### Key files

- `src-tauri/src/daemon/sse.rs` — EOF detection
- `src/lib/workshopConnection.ts` — reconnect + resume
- `src/lib/stores/chat.svelte.ts` — scoped cancel, `stopOwnedInteractiveStreams`
- `src/routes/popout/chat/+page.svelte` — popout lifecycle

---

## Sprint B — State matches UI *(done)*

| # | Item | Status |
|---|------|--------|
| B1 | `noteStreamFailure` evicts stale `streamOwners`; recoverable vs fatal | Done |
| B2 | `cancelActiveTurn` evicts/stops only session-owned streams | Done |
| B3 | Resume/reattach failures → `historyNotice` / `streamError` | Done |
| B4 | Workspace reconcile failures → `workspace.streamError` | Done |
| B5 | Budget: `budgetWorkCardId` + `workCardId` on pending approvals | Done |
| B6 | Skip duplicate budget push when chat already has pending approval | Done |
| B7 | Guard `JSON.parse` in SSE event listeners | Done |

### Key files

- `src/lib/stores/chat.svelte.ts` — `noteStreamFailure`, `evictStreamOwners`, `noteResumeFailure`
- `src/lib/workshopConnection.ts` — `recoverInteractiveStreams`, recoverable error detection
- `src/lib/notifications.ts` — `budgetWorkCardId`
- `src/lib/stores/workspace.svelte.ts` — reconcile error surfacing, deduped budget notify
- `src/lib/daemon.ts` — safe SSE payload parse

---

## Sprint C — Trust the first run *(done)*

| # | Item | Status |
|---|------|--------|
| C1 | Wizard skip/completion fail-closed when engine won't start | Done |
| C2 | `requireEngineReady()` helper for startup gates | Done |
| C3 | Autostart prefs saved only after install/remove succeeds | Done |
| C4 | Autostart toggle reloads prefs on failure (checkbox stays honest) | Done |
| C5 | Mobile turns pass provider/model after runtime hydrate | Done |
| C6 | Mobile composer routing hint + `DaemonPortalChip` | Done |

### Key files

- `src/lib/components/wizard/WizardWelcomeScreen.svelte` — skip gate
- `src/lib/components/wizard/WizardCompletionScreen.svelte` — completion gate
- `src/lib/utils/providersApi.ts` — `requireEngineReady`
- `src-tauri/src/connection_prefs.rs` — autostart order
- `src/lib/interactiveTurnOptions.ts` — mobile pass-through
- `src/lib/components/mobile/MobileChatComposer.svelte` — portal chip

---

## P2 backlog

Mutex poison panics, dual reconcile paths, health vs SSE timeout mismatch, mobile native listener races.
