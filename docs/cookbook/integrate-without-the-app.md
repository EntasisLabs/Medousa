# Integrate without the Medousa app

Use **Medousa Engine** as infrastructure. Your UI, your portal, your Slack bot — same runtime non-devs get in the app.

Overview: [engine/README.md](../engine/README.md)  
API index: [engine/http-api.md](../engine/http-api.md)

---

## Pattern 1 — HTTP ask (simplest)

Run engine:

```bash
medousa start daemon
```

Ask from anything that speaks HTTP:

```bash
medousa-cli daemon-ask "What changed in ops this week?" \
  --daemon-url http://127.0.0.1:7419
```

Or `POST /v1/jobs/ask` + poll `/v1/jobs/{id}/result` from your service.

**Corp angle:** Internal dashboard → your backend → Medousa Engine on VPC. Employees never see Medousa UI; they see *your* product powered by the same brain.

---

## Pattern 2 — Streaming interactive turns

Build a custom chat UI:

- `POST /v1/interactive/turn` with SSE
- Same route the TUI and app use
- Session + identity headers per your policy

Reference client: `medousa_tui`, `apps/medousa-home` frontend.

---

## Pattern 3 — Ingest / channels

Reuse adapters instead of rebuilding bots:

```bash
medousa start daemon
medousa telegram   # or discord, slack, whatsapp
```

`POST /v1/ingest` for custom channel names — wire your product’s event bus into the engine.

---

## Pattern 4 — Local inference API only

Headless Gemma for air-gapped or edge:

```bash
medousa start daemon --inference
```

| Step | API |
|------|-----|
| Tier probe | `GET /v1/local/hardware` |
| Download | `POST /v1/local/models/download` |
| Load | `POST /v1/local/engine/load` |
| Chat | OpenAI-compatible `http://127.0.0.1:7421/v1` with provider `medousa-local` |

Your product UI handles progress bars; engine handles bytes and SHA256.

---

## Pattern 5 — MCP tools (BYOB)

Register servers in `~/.config/medousa/mcp-gateway.toml`:

```bash
medousa start mcp-gateway
```

Engine invokes `cognition.mcp.*` tools with policy tokens.  
Setup: [mcp-gateway-setup.md](../mcp-gateway-setup.md)

**Corp angle:** Calendar, CRM, internal APIs as MCP — Medousa executes with audit trail; you keep gateway inside VPN.

---

## Pattern 6 — Recurring & delivery

Schedule work without a human in chat:

```bash
medousa-cli daemon-watch-add "0 7 * * *" "Morning ops brief" --tz America/New_York
```

Or `POST /v1/recurring/prompt`. Outbox delivers to Telegram, webhook, or explicit target.

---

## Pattern 7 — Identity & memory API

`medousa-cli daemon-identity-*` wraps propose/commit/history.

Embed **Locus memory** in your app: same user_id across your UI and Medousa Engine → one identity graph.

---

## Pattern 8 — Pairing & edge devices

LAN QR protocol (`GET /qr`, `/pair/*`) for phones or kiosks talking to a edge-hosted engine.

Run engine with `--public` on factory floor Mac mini; phones scan QR — no Medousa app required on kiosk.

---

## Enterprise checklist

| Requirement | Medousa answer |
|-------------|----------------|
| Same stack home + office | One engine binary, one HTTP contract |
| Air-gap | Local inference + SurrealKV on disk |
| Audit | Stasis job history, verification store, artifact chunks |
| Sandboxed automation | OpenShell + MCP policy |
| No vendor UI lock-in | Engine headless; your UX |

Deep dive: [enterprise-architecture-and-flow-guide.md](../../architecture/enterprise-architecture-and-flow-guide.md)

---

## What not to do

- Don’t ask employees to `curl | bash` if they’re non-devs — give them **Medousa** the app.
- Don’t fork the turn loop — call the engine; that’s the Ferrari.
