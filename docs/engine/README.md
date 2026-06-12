# Medousa Engine — overview

Medousa is two products sharing one runtime:

- **Medousa** — the app people download and love.
- **Medousa Engine** — the headless service (`medousa_daemon`) that powers the app and every integration.

If your users already run Medousa at home, your enterprise product can offer **the same engine** — same durability, same memory model, same local inference story — without asking them to learn a new stack.

---

## Why embed the engine?

| Problem | Engine answer |
|---------|----------------|
| Chat wrappers lose work on restart | **Stasis** job lifecycle — enqueue, retry, finish, deliver |
| Assistants forget between sessions | **Locus** identity + session stores — structured memory, not scrollback |
| “Our AI” vs “their AI” at home | **One protocol** — normies use the app; your backend calls the same HTTP surface |
| Air-gapped / on-prem | **Local inference** — Gemma via `/v1/local/*`, loopback OpenAI on `:7421` |
| Tool sprawl | **MCP gateway** — bring your servers; Medousa executes with policy |

---

## Topology

```
┌─────────────────────────────────────────────────────────┐
│  Your UI (web, mobile, Slack bot, internal portal)      │
└───────────────────────────┬─────────────────────────────┘
                            │ HTTP
┌───────────────────────────▼─────────────────────────────┐
│  Medousa Engine (medousa_daemon :7419)                  │
│  · Agent runtime + tool loop                            │
│  · Durable jobs / recurring / delivery                  │
│  · Identity, vault, workspace APIs                      │
│  · Optional local Gemma engine (:7421)                  │
└───────────────────────────┬─────────────────────────────┘
                            │
         ┌──────────────────┼──────────────────┐
         ▼                  ▼                  ▼
    LLM providers      MCP gateway        SurrealKV / mem
    (cloud or local)   (:7420)            persistence
```

The **Medousa app** is just another client — Tauri shell + IPC that starts the engine and calls the same routes.

---

## Integration modes

1. **Drop-in daemon** — run `medousa_daemon` beside your stack; call `/v1/jobs/ask` or `/v1/interactive/turn`.
2. **Channel adapter** — point Discord/Telegram/Slack/WhatsApp adapters at your daemon URL; users keep their chat habits.
3. **Local brain only** — ship engine with `embedded-inference`; your UI never mentions models; download via `/v1/local/models/download`.
4. **MCP BYOB** — register Playwright, calendar, CRM servers in `mcp-gateway.toml`; engine invokes with policy tokens.

See [Integrate without the app](../cookbook/integrate-without-the-app.md) and [HTTP API reference](http-api.md).

---

## Trust & operations

- **Loopback by default** — `127.0.0.1:7419`; use `--public` + pairing for LAN phones.
- **Policy profiles** — interactive vs scheduled lanes; OpenShell sandbox for skill scripts.
- **Observability** — `/health`, `/v1/stats`, Stasis dashboard mount, `medousa doctor --local-engine`.

Deep architecture: [enterprise-architecture-and-flow-guide.md](../../architecture/enterprise-architecture-and-flow-guide.md), [component-daemon.md](../../architecture/component-daemon.md).

---

## Same engine, different surfaces

| Surface | Entry |
|---------|--------|
| Normie app | Download Medousa — [product README](../../README.md) |
| Power user | `medousa tui`, `medousa setup` — [CLI cookbook](../cookbook/cli-and-workspace.md) |
| Automation | `medousa-cli daemon-ask`, cron + `/v1/recurring` |
| Product team | Your HTTP client → `/v1/jobs/ask` |

**You’re not selling a chat skin. You’re selling a finished engine.**
