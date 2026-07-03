# Medousa — developer & integrator docs

The **[product README](../README.md)** is for humans downloading **Medousa** the app.

This folder is for **engineers, operators, and teams** who want the same runtime without the UI — or who want to embed **Medousa Engine** next to existing workflows.

Doc conventions: [CONTRIBUTING-DOCS.md](CONTRIBUTING-DOCS.md)

## The two-layer product

| Layer | Audience | What you get |
|-------|----------|--------------|
| **Medousa** (app) | Everyone | Welcome wizard, chat, vault, pairing — zero terminal |
| **Medousa Engine** (`medousa_daemon`) | Devs, corps, power users | Durable agent runtime, HTTP API, channels, MCP; offline brain via `medousa_local` |

**Same engine.** Your company doesn’t re-introduce a foreign stack — employees may already run Medousa at home; your product can speak the same protocol.

---

## Start here

### Cookbook — run it yourself

| Guide | You want to… |
|-------|----------------|
| [Install & self-host](cookbook/install-and-self-host.md) | `install.sh`, setup, doctor, data paths |
| [Configuration reference](configuration-reference.md) | **All env vars** — LLM, MCP, Locus, grapheme, channels |
| [CLI & workspace](cookbook/cli-and-workspace.md) | `medousa` commands, TUI, `start daemon --inference` |
| [Channels & chat commands](cookbook/channels-and-chat.md) | Discord, Telegram, Slack, WhatsApp, slash commands |
| [Skills & specialties](cookbook/skills-and-specialties.md) | Manuscripts, Hermes/Cursor/OpenClaw import |
| [Build from source](cookbook/build-from-source.md) | Cargo, Tauri dev, release builds, iPhone dev |
| [Integrate without the app](cookbook/integrate-without-the-app.md) | HTTP-only, jobs, ingest, MCP, corp patterns |
| [Mobile & LAN](cookbook/mobile-and-lan.md) | Phone pairing, iOS dev, workshop transport |
| [Custom chat UI](cookbook/custom-chat-ui.md) | Sessions, streaming, artifacts |
| [Artifacts & presentations](cookbook/artifacts-and-presentations.md) | HTML artifacts, Library tab, list-ui API |
| [Custom views & canvas](cookbook/custom-views-and-canvas.md) | Pinned dashboards Medousa builds for you |
| [Environment canvas (advanced)](cookbook/environment-canvas-advanced.md) | Operators: spec, presets, feeds, HTTP |
| [Vault & library](cookbook/vault-and-library.md) | Multi-root vault, wikilinks, external files |

### Engine — embed & scale

| Guide | You want to… |
|-------|----------------|
| [Engine overview](engine/README.md) | Why corps embed Medousa Engine, topology, trust model |
| [HTTP API reference](engine/http-api.md) | Full route tables |
| [Interactive streaming](engine/interactive-streaming.md) | Two-step turn + SSE events |
| [Artifacts](engine/artifacts.md) | Agent tools + HTTP commands |
| [Vault](engine/vault.md) | Notes API + cognition vault tools |
| [Workspace](engine/workspace.md) | Work board, feed, SSE |
| [Agent tools](engine/agent-tools.md) | Host/worker lanes, discover domains |
| [Runtime config](engine/runtime-config.md) | Inference profiles, stage routing |
| [Extensions](engine/extensions.md) | Grapheme, Locus, workflows, media, STT |
| [Architecture (deep)](../architecture/README.md) | Component boundaries; [daemon modules](architecture/daemon-modules.md), [turn runtime](../architecture/turn-runtime-and-lanes.md), [roadmap](../architecture/ROADMAP.md) |

### SDK — Rust & Python clients

| Guide | You want to… |
|-------|----------------|
| [SDK overview](sdk/README.md) | Crates, quick start, Tauri pattern |
| [Python SDK](sdk/python.md) | `pip install`, async client, SSE streaming |
| [API reference](sdk/api-reference.md) | Every `MedousaClient` method |
| [Interactive streaming (SDK)](sdk/interactive-streaming.md) | Client-side SSE flow |
| [Transports](sdk/transports.md) | HTTP, Workshop, custom `Transport` |
| [Artifacts (SDK)](sdk/artifacts.md) | `runtime().artifact_*` |

### Apps

| Guide | Topic |
|-------|--------|
| [medousa-home](apps/medousa-home.md) | Tauri IPC, transport, store mapping, mobile shell |

### Setup guides & runbooks

| Guide | Topic |
|-------|--------|
| [MCP gateway setup](mcp-gateway-setup.md) | Tool servers, `mcp-gateway.toml` |
| [OpenShell handoff](openshell-handoff-setup.md) | Sandboxed skill execution |
| [Connection reliability](runbooks/connection-reliability.md) | SSE/workshop lifecycle |
| [Doctor & health](runbooks/doctor-and-health.md) | `medousa doctor`, probes |
| [Upgrade & data dir](runbooks/upgrade-and-data-dir.md) | `MEDOUSA_DATA_DIR`, multi-workshop |

Full index: [runbooks/README.md](runbooks/README.md)

---

## Quick paths (power users & operators)

```bash
curl -fsSL https://raw.githubusercontent.com/EntasisLabs/Medousa/main/scripts/install.sh | bash
medousa start daemon --inference
medousa doctor
medousa doctor --local-engine
curl -s http://127.0.0.1:7419/health
```

App development: [Build from source](cookbook/build-from-source.md) · [medousa-home integrator doc](apps/medousa-home.md)

---

## Contributing

Implementation history: [`architecture/`](../architecture/). **Active work:** [`architecture/ROADMAP.md`](../architecture/ROADMAP.md). Shipped milestones: [`architecture/archive/`](../architecture/archive/). ADRs: [`docs/architecture/decisions/`](architecture/decisions/README.md).
