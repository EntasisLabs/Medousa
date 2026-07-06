# Home app — connection reliability

**Audience:** contributor, operator

How `medousa-home` keeps workshop pipes and interactive streams healthy after drops, backgrounding, and workshop switches.

Integrator contract: [interactive-streaming.md](../engine/interactive-streaming.md) · ADR: [adr-004-durable-turn-spine.md](../architecture/decisions/adr-004-durable-turn-spine.md)

---

## Current architecture

### Engine side (daemon)

- Every turn event is journaled to **`TurnEventLog`** on disk (`medousa-engine` crate).
- SSE is a projection of that journal — clients reconnect with `GET …/stream?since=<last_seq>`.
- Gateway pooling, bounded accept, and SSE idle timeout live in `src/iroh_transport/gateway.rs` and `src/comms/`.

### Home app side

| Layer | Role |
|-------|------|
| [`reconnect.ts`](../../apps/medousa-home/src/lib/stream/reconnect.ts) | Shared primitives: `streamPathWithSince`, `applyStreamSeq`, `ReconnectScheduler`, circuit breaker |
| [`workshopConnection.ts`](../../apps/medousa-home/src/lib/workshopConnection.ts) | Workshop lifecycle, workspace reconnect, `recoverInteractiveStreams` |
| [`chat.svelte.ts`](../../apps/medousa-home/src/lib/stores/chat.svelte.ts) | Per-session stream owners, seq tracking, scoped cancel |
| [`daemon/sse.rs`](../../apps/medousa-home/src-tauri/src/daemon/sse.rs) | Rust SSE bridge — EOF detection |

Transport: `daemon/sdk.rs` → `medousa-sdk-iroh` for JSON; SSE bytes may still use legacy workshop helpers for `interactive_stream_start`.

---

## Reconnect discipline

1. Track `seq` on every `InteractiveTurnStreamEvent` (generated type includes `seq`).
2. On SSE error before `terminal`, reattach with `?since=<last_seq>` on the same stream URL.
3. Use bounded exponential backoff + overlap guard (`ReconnectScheduler`) — do not spin tight reconnect loops.
4. Fallback: poll `session_get_active_turn` when `turn_id` is unknown.

This matches Rust `stream_turn_reconnecting` and Python `stream_turn_reconnecting` — see [SDK interactive streaming](../sdk/interactive-streaming.md).

---

## Shipped fixes (sprints A–C)

**Sprint A — pipes stay alive:** Rust EOF errors, workspace auto-reconnect, interactive reattach, foreground `resumeWorkshop`, popout unmount without global stream stop, scoped `cancelActiveTurn`.

**Sprint B — state matches UI:** `noteStreamFailure` / `evictStreamOwners`, scoped cancel, resume failure warnings, workspace reconcile errors, budget card dedup, safe SSE JSON parse.

**Sprint C — trust the first run:** wizard fail-closed gates, `requireEngineReady`, honest autostart prefs, mobile provider/model pass-through.

Key files listed in git history under `apps/medousa-home/` — see transport stack in [medousa-home.md](../apps/medousa-home.md).

---

## P2 backlog

Mutex poison panics, dual reconcile paths, health vs SSE timeout mismatch, mobile native listener races, full migration of `interactive_stream_start` to `medousa-sdk-iroh` byte streaming.
