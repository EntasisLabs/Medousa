# HTTP API reference (Medousa Engine)

Base URL default: `http://127.0.0.1:7419`  
Override: `MEDOUSA_DAEMON_URL`

Full component notes: [component-daemon.md](../../architecture/component-daemon.md)

---

## Health & ops

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/health` | Liveness, agent runtime version, tool count, last turn latency |
| GET | `/v1/stats` | Job counters (enqueued, running, succeeded, failed) |
| GET | `/v1/delivery/status` | Outbox / webhook delivery health |
| GET | `/v1/continuations/status` | Turn continuation / DLQ posture |

---

## Interactive chat

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/v1/interactive/turn` | TUI / app streaming turns (SSE) |
| POST | `/v1/ingest` | Channel adapters — enqueue ask from Discord, Telegram, CLI, etc. |

---

## Jobs (headless ask / report)

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/v1/jobs/ask` | Agent turn; poll result |
| GET | `/v1/jobs/{id}/result` | Fetch completed job output |
| POST | `/v1/jobs/report` | Structured report flow with citations |
| POST | `/v1/jobs/prompt` | Legacy Stasis prompt job |
| POST | `/v1/recurring/prompt` | Register cron-style recurring work |

CLI wrappers: `medousa-cli daemon-ask`, `daemon-report`, `daemon-watch-add`.

---

## Workspace & vault (app parity)

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/v1/workspace/...` | Work board, feed, cards — see workspace handlers |
| GET/POST | `/v1/vault/...` | Notes, search, backlinks |

Used by the Medousa app; available to any client with daemon access.

---

## Local inference (private brain)

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/v1/local/hardware` | RAM tier, recommended Gemma SKU |
| GET | `/v1/local/catalog` | Models allowed for tier |
| GET | `/v1/local/models` | Installed models + active downloads |
| POST | `/v1/local/models/download` | Start HF download job |
| GET | `/v1/local/models/download/{job_id}` | Progress |
| DELETE | `/v1/local/models/{model_id}` | Remove installed model |
| GET | `/v1/local/engine/status` | Loopback server on `:7421` |
| POST | `/v1/local/engine/load` | Load Gemma into engine |

Provider id: `medousa-local` → base URL `http://127.0.0.1:7421/v1`

CLI parity: `medousa models probe|download|engine-load`, `medousa start daemon --inference`

Plan: [embedded-local-inference-plan.md](../../architecture/embedded-local-inference-plan.md)

---

## Pairing (LAN phone)

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/qr` | Pairing QR payload |
| GET | `/pair/status` | Paired devices |

Protocol: [normie-onboarding-and-lan-pairing-plan.md](../../architecture/normie-onboarding-and-lan-pairing-plan.md)

---

## Identity

Identity propose/commit/history routes under `/v1/identity/...` — see `medousa-cli daemon-identity-*` commands.

---

## MCP

Medousa Engine delegates tool calls to **MCP gateway** (default `http://127.0.0.1:7420`).  
Setup: [mcp-gateway-setup.md](../mcp-gateway-setup.md)

---

## Suggested integration patterns

**Sync ask (script):**

```bash
medousa-cli daemon-ask "Summarize open risks" --daemon-url http://127.0.0.1:7419
```

**Async job (your service):**

1. `POST /v1/jobs/ask` with JSON body (channel, user_id, text)
2. Poll `GET /v1/jobs/{id}/result` until complete
3. Deliver via your UI or `/v1/delivery` webhook

**Replace your chat UI entirely** — keep engine; swap only presentation.

More recipes: [integrate-without-the-app.md](../cookbook/integrate-without-the-app.md)
