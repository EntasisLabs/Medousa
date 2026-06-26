# Doctor & health

**Audience:** operator

---

## Basic health

```bash
medousa doctor
curl -s http://127.0.0.1:7419/health | jq .
```

`GET /health` returns agent runtime version, tool count, last turn latency (`HealthResponse`).

SDK: `client.health().get().await?`

---

## Local engine probe

```bash
medousa doctor --local-engine
```

Checks `medousa_local` on `:7421`, HF token hints, installed models.

Daemon route: `GET /v1/local/engine/status` — **probe only**; load via `medousa models engine-load`.

---

## Delivery & continuations

| Route | Purpose |
|-------|---------|
| `GET /v1/delivery/status` | Outbox / webhook health |
| `GET /v1/continuations/status` | DLQ / continuation posture |
| `GET /v1/stats` | Job counters |

---

## MCP gateway

```bash
curl -s http://127.0.0.1:7420/health   # if gateway running
```

Daemon: `GET /v1/mcp/gateway/status` — SDK `mcp_gateway().status()`.

[mcp-gateway-setup.md](../mcp-gateway-setup.md)

---

## When things look stuck

See [connection-reliability.md](connection-reliability.md) for SSE/workshop lifecycle.
