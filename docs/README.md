# Medousa — developer & integrator docs

The **[product README](../README.md)** is for humans downloading **Medousa** the app.

This folder is for **engineers, operators, and teams** who want the same runtime without the UI — or who want to embed **Medousa Engine** next to existing workflows.

## The two-layer product

| Layer | Audience | What you get |
|-------|----------|--------------|
| **Medousa** (app) | Everyone | Welcome wizard, chat, vault, pairing — zero terminal |
| **Medousa Engine** (`medousa_daemon`) | Devs, corps, power users | Durable agent runtime, HTTP API, channels, local inference, MCP |

**Same engine.** Your company doesn’t re-introduce a foreign stack — employees may already run Medousa at home; your product can speak the same protocol.

---

## Start here

### Cookbook — run it yourself

| Guide | You want to… |
|-------|----------------|
| [Install & self-host](cookbook/install-and-self-host.md) | `install.sh`, setup, doctor, data paths, env vars |
| [CLI & workspace](cookbook/cli-and-workspace.md) | `medousa` commands, TUI, `start daemon --inference` |
| [Channels & chat commands](cookbook/channels-and-chat.md) | Discord, Telegram, Slack, WhatsApp, slash commands |
| [Skills & specialties](cookbook/skills-and-specialties.md) | Manuscripts, Hermes/Cursor/OpenClaw import |
| [Build from source](cookbook/build-from-source.md) | Cargo, Tauri dev, release builds, iPhone dev |
| [Integrate without the app](cookbook/integrate-without-the-app.md) | HTTP-only, jobs, ingest, MCP, corp patterns |

### Engine — embed & scale

| Guide | You want to… |
|-------|----------------|
| [Engine overview](engine/README.md) | Why corps embed Medousa Engine, topology, trust model |
| [HTTP API reference](engine/http-api.md) | Routes, contracts, interactive turns, local inference |
| [Architecture (deep)](../architecture/README.md) | Component boundaries, turn worker, identity, plans |

### Existing setup guides

| Guide | Topic |
|-------|--------|
| [MCP gateway setup](mcp-gateway-setup.md) | Tool servers, `mcp-gateway.toml` |
| [OpenShell handoff](openshell-handoff-setup.md) | Sandboxed skill execution |

---

## Quick paths (power users & operators)

Need the terminal workspace or headless engine without the app? Start here:

```bash
# Self-host engine (power user / server)
curl -fsSL https://raw.githubusercontent.com/EntasisLabs/Medousa/main/scripts/install.sh | bash
medousa start daemon --inference

# Health
medousa doctor --local-engine

# Ask via HTTP (no UI)
curl -s http://127.0.0.1:7419/health
```

App development lives under `apps/medousa-home/` — see [Build from source](cookbook/build-from-source.md).

---

## Contributing

Implementation plans and ADRs live in [`architecture/`](../architecture/). Product onboarding spec: [`architecture/normie-onboarding-and-lan-pairing-plan.md`](../architecture/normie-onboarding-and-lan-pairing-plan.md).
