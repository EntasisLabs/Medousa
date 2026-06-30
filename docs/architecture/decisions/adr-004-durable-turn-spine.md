# ADR-004: Durable turn spine for interactive SSE

**Status:** Accepted  
**Date:** 2026-06

## Context

Interactive chat uses a two-step contract: `POST /v1/interactive/turn`, then `GET …/stream` as SSE. Clients on mobile, LAN, and long-running desktop sessions frequently drop SSE connections mid-turn.

Earlier implementations buffered stream events in memory. After a drop, clients had to poll `GET /v1/sessions/{id}/active-turn` and hope the in-memory buffer still held missed events — unreliable across daemon restarts and under load.

Engine hardening (Phase 1 + Phase 5 crate split) required a single durable source of truth for turn events that HTTP, ingest channels, and recovery could share.

## Decision

1. **Journal every `TurnEvent` to disk** via `TurnEventLog` (`medousa-engine` crate).
2. **SSE is a projection** of the journal plus a live tail — not the authority.
3. **Clients resume with `?since=<seq>`** on the same stream URL. The server replays events with `seq > since`, then continues live.
4. **Each SSE payload includes monotonic `seq`** per turn (`InteractiveTurnStreamEvent.seq`).
5. **Startup recovery** replays uncommitted turns from the spine (`recover_uncommitted`).

## Alternatives considered

| Option | Rejected because |
|--------|------------------|
| In-memory ring buffer per turn | Lost on restart; bounded size drops events |
| Poll active-turn only | Extra round-trip; no event replay without turn id |
| Client-side full turn restart | Duplicates LLM work; bad UX mid-stream |

## Consequences

**Positive**

- Reconnect is deterministic: same `turn_id`, same `stream_url`, cursor via `seq`.
- Daemon restart can recover in-flight turns from disk.
- Rust/Python SDKs ship `stream_reconnecting` / `stream_turn_reconnecting` helpers.
- `medousa-home` aligns via `reconnect.ts` (`streamPathWithSince`, seq dedup).

**Tradeoffs**

- Disk I/O per turn event (acceptable for chat-scale throughput).
- Clients must track `seq` and dedupe after replay.
- Integrators must not assume SSE POST on the turn endpoint.

**Migration**

- No API version bump: `since` is optional (default `0` = full replay).
- `seq` field added to `InteractiveTurnStreamEvent` — clients should ignore unknown fields per usual JSON contract.

## Code anchors

| Area | Path |
|------|------|
| Engine crate | `crates/medousa-engine/src/turn_event_log.rs` |
| Turn lifecycle | `crates/medousa-engine/src/engine.rs` (`run_turn`) |
| SSE projection | `src/sse_turn_projection.rs` |
| Interactive handler | `src/daemon/interactive.rs` |
| Ingest stream | `src/daemon/ingest.rs` (`StreamSinceQuery`) |
| Recovery | `src/engine_recovery.rs` |
| Rust SDK reconnect | `crates/medousa-sdk/src/reconnecting_stream.rs` |
| Python SDK reconnect | `python/medousa-sdk/src/medousa/reconnect.py` |
| Home TS reconnect | `apps/medousa-home/src/lib/stream/reconnect.ts` |

## References

- Integrator guide: [docs/engine/interactive-streaming.md](../../engine/interactive-streaming.md)
- Component doc: [component-engine.md](../../../architecture/component-engine.md)
