# Recurring schedule → channel delivery — Roadmap

> Created: 2026-05-30  
> Status: Phase 1 implemented (verify with `cargo test -p medousa recurring_delivery`)  
> Related: [outbox-channel-delivery-roadmap.md](outbox-channel-delivery-roadmap.md), [centralized-ingester-roadmap.md](centralized-ingester-roadmap.md)

## Problem

Interactive Telegram/ingest turns register `channel_deliveries[job_id]` and push final text via the internal outbox webhook. **Recurring / scheduled** Stasis jobs do not:

- Each materialized run gets a **new** `job_id`.
- Stasis sets `job.correlation_id` to the **recurring definition id**.
- The webhook handler only looked up `channel_deliveries[job_id]`, logged “missing delivery target”, and returned 200 without messaging the user.

Users also need to choose **where** to be notified (Telegram while in TUI, Slack channel, etc.) — not only “the channel they are typing in right now.”

## Target architecture

```
Agent / API registers recurring
        ↓
parse delivery spec (explicit | current_channel | product_default)
        ↓
recurring_delivery_store[recurring_id] → ChannelDeliveryTarget
        ↓
Stasis register_recurring(definition)
        ↓
[each due tick] materialize → new job_id (correlation_id = recurring_id)
        ↓
process_once → succeed → outbox job_succeeded
        ↓
POST /v1/deliver/outbox
        ↓
lookup: channel_deliveries[job_id] OR recurring_delivery_store[correlation_id]
        ↓
dispatch_channel_message (Telegram / Discord / Slack / WhatsApp)
```

## Delivery spec (tool + API input)

Optional `delivery` object on all recurring registration surfaces:

```json
{
  "cron_expr": "0 0 */4 * * * *",
  "timezone": "UTC",
  "delivery": {
    "channel": "telegram",
    "telegram_chat_id": "123456789"
  }
}
```

### Canonical stored shape

Same as ingest (`ChannelDeliveryTarget`):

| Field | Example |
|-------|---------|
| `channel` | `telegram`, `discord`, `slack`, `whatsapp`, `cli` |
| `channel_id` | `telegram:chat:123456789` |
| `user_id` | `telegram:user:…` (allowlist) |
| `session_id` | Medousa session for prompt context (defaults to `recurring-{recurring_id}`) |

### Convenience fields

| Input | Normalized |
|-------|------------|
| `telegram_chat_id` | `telegram:chat:{id}` |
| `discord_channel_id` | `discord:channel:{id}` |
| `slack_channel_id` | `slack:channel:{id}` |
| `whatsapp_chat_jid` | `whatsapp:chat:{jid}` |

### Modes (`delivery.mode`)

| Mode | Behavior |
|------|----------|
| *(omit)* / `explicit` | Require resolvable channel + destination id |
| `current_channel` | Use ambient `ChannelDeliveryTarget` from active agent turn scope (ingest / continuation) |
| `product_default` | First configured heartbeat chat for that channel in product config |

### Cron format

Stasis uses the Rust `cron` crate (**7 fields**):

```text
sec  min  hour  day-of-month  month  day-of-week  year
```

Example every 4 hours: `0 0 */4 * * * *`

Registration rejects schedules whose first two firings are **&lt; 60 seconds** apart (catches `0/1 * * * * * *` style mistakes).

## Security

- **Telegram:** chat must be in `telegram.heartbeat_chat_ids` or sender must match `telegram.allowed_user_ids` when `user_id` is set.
- **Discord / Slack:** destination id must appear in configured heartbeat channel lists (until per-user linking exists).
- **`cli`:** binding stored; push is a no-op (same as dispatch today).

## Phases

### Phase 1 — Explicit delivery + webhook resolve (this PR)

- [x] Architecture doc
- [x] `recurring_delivery_store` (in-memory + Surreal)
- [x] `parse_delivery_spec` + `persist_recurring_delivery_binding`
- [x] `resolve_delivery_target_for_job` in outbox webhook
- [x] Wire `cognition_runtime_recurring_register`, grapheme promote tools, workflow schedule
- [x] `POST /v1/recurring/prompt` optional `delivery`
- [x] Cron min-interval validation on register
- [x] Unit tests

**Exit:** Register with `delivery.telegram_chat_id`; each successful recurring run pushes output to that chat.

### Phase 2 — Ergonomics

- [ ] `turn_scope` on recurring register tools for `current_channel` from ingest-originated agent turns
- [ ] Doctor / list tools show `recurring_id`, cron, delivery binding
- [ ] Resolve Telegram chat from `channel_session_store` by identity (TUI user linked prior Telegram chat)

### Phase 3 — Richer jobs (later)

- [ ] Recurring **agent turns** (`run_agent_turn` per tick) vs prompt/grapheme-only output
- [ ] User-level default delivery profile in product config

## Code anchors

| Area | Path |
|------|------|
| Store + parse | `src/recurring_delivery.rs` |
| Outbox webhook | `src/bin/medousa_daemon.rs` (`deliver_outbox_webhook`) |
| Register tools | `src/runtime_tools.rs`, `src/tools.rs` |
| HTTP API | `src/daemon_api.rs`, `register_recurring_prompt` |
| Platform init | `src/runtime/platform.rs` |
| Surreal schema | `src/runtime/stasis_surreal_schema.rs` |

## Non-goals (Phase 1)

- Storing delivery inside Stasis `RecurringDefinition` metadata
- Per-materialized-job `channel_deliveries` entries
- Open-ended “message any chat” without allowlist checks
