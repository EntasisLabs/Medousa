# Outbox → Channel Delivery — Roadmap

> Created: 2026-05-30  
> Status: Complete  
> Related: [centralized-ingester-roadmap.md](centralized-ingester-roadmap.md)

## Problem

Medousa ingester jobs can **succeed** while Stasis outbox events stay **pending** and users never receive the assistant reply on Telegram/Discord.

That is not a dashboard cosmetic issue. In Stasis, outbox publish is the **delivery obligation** — pending means the product did not finish the conversation loop.

We shipped a parallel SSE side-channel (ingest stream polling) without wiring Stasis endpoint routing or channel dispatch. The canonical path was skipped.

## Target Architecture

```
Adapter POST /v1/ingest
        ↓
Daemon enqueues job + registers delivery target (channel, user, chat, session)
        ↓
Scheduler process_once → job succeeds → outbox JobSucceeded (pending)
        ↓
Scheduler publish_pending_events → EndpointRoutingEventPublisher
        ↓
Internal HTTP webhook POST /v1/deliver/outbox (same daemon)
        ↓
Resolve job output + dispatch to channel (Telegram API, Discord, …)
        ↓
Outbox marked published — conversation turn complete
```

SSE ingest streams remain for **typing indicators and incremental deltas** during processing. **Final user-visible delivery** is authoritative via outbox → webhook → channel dispatch.

## Phases

### Phase 1 — Wire the delivery pipeline ✅

- [x] Document this roadmap
- [x] `ChannelDeliveryTarget` registry keyed by `job_id` at ingest enqueue time
- [x] `build_daemon_runtime`: enable `.with_endpoint_routing_delivery()`, seed internal outbox webhook endpoint
- [x] `POST /v1/deliver/outbox` handler (Stasis webhook payload shape)
- [x] Resolve job output text from runtime attempts on `job_succeeded`
- [x] Log when delivery target missing (debuggable)

**Exit criteria:** After a Telegram ask, outbox events move from `pending` → `published` in dashboard; daemon logs show deliver handler invoked.

### Phase 2 — Telegram push dispatch ✅

- [x] Daemon sends final message via Telegram Bot API (`sendMessage`) using stored bot token
- [x] Parse `telegram:chat:{id}` from `channel_id`
- [x] Truncate/format for Telegram limits
- [x] Handle `JobDeadLettered` / failed jobs with user-visible error message
- [x] Idempotency: ignore duplicate webhook delivery for same `event_id`

**Exit criteria:** User receives assistant reply on Telegram even without holding the ingest SSE stream open.

### Phase 3 — Adapter role adjustment ✅

- [x] Telegram adapter: ingest ack + typing only; treat outbox-driven push as source of truth for final text
- [x] Adapter long-poll `GET /v1/deliver/poll/{job_id}` fallback if push fails
- [x] Discord adapter parity
- [x] CLI: deliver via stdout or fire-and-forget based on `--no-wait` flag

**Exit criteria:** Thin adapters match roadmap principle — forward in, render out, no duplicate job polling logic.

### Phase 4 — Hardening & product config ✅

- [x] Internal webhook bearer token (`MEDOUSA_DELIVER_WEBHOOK_TOKEN` + product config)
- [x] Product config auto-generates deliver webhook token on setup
- [x] Doctor checks: endpoint seeded, auth configured, pending count, last deliver latency
- [x] Proactive / heartbeat messages use same dispatch path (daemon scheduler)

## Code Anchors

| Area | Path |
|------|------|
| Delivery registry + dispatch | `src/channel_delivery.rs` |
| Daemon runtime build | `src/lib.rs` (`build_daemon_runtime`) |
| Deliver webhook route | `src/bin/medousa_daemon.rs` |
| Ingest job registration | `src/session_mapping.rs`, `start_ingest_ask_stream` |
| Stasis outbox publish | scheduler tick → `RuntimeSdk::publish_pending_events` |

## Non-Goals (for now)

- Replacing Stasis outbox with a custom queue
- External customer webhooks (Phase 4+ could expose optional outbound endpoints)
- Removing ingest SSE entirely (keep for typing until centralized agent runtime lands)

## Successor track

Reply **generation** converges on [centralized-agent-runtime-roadmap.md](centralized-agent-runtime-roadmap.md). This delivery track remains the completion contract for all channels once the daemon hosts the gold-standard runtime.
