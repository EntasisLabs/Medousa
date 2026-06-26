# Contributing to Medousa documentation

Audience tags used throughout `docs/`:

| Tag | Who | Examples |
|-----|-----|----------|
| **integrator** | Teams embedding Medousa Engine or building clients | HTTP API, SDK, streaming, artifacts |
| **operator** | Self-hosters and admins | Cookbook, config reference, runbooks |
| **contributor** | Engine and app developers | `architecture/`, turn runtime, component boundaries |

## Where new content goes

| Topic | Canonical location | Notes |
|-------|-------------------|--------|
| HTTP routes, request types | [`docs/engine/http-api.md`](engine/http-api.md) | Source of truth: [`src/daemon/router.rs`](../src/daemon/router.rs) |
| Subsystem behavior (artifacts, vault, streaming) | [`docs/engine/*.md`](engine/) | Link to `architecture/` for FSM internals |
| SDK methods and examples | [`docs/sdk/`](sdk/) | Source of truth: [`crates/medousa-sdk/src/`](../crates/medousa-sdk/src/) |
| Task-oriented how-tos | [`docs/cookbook/`](cookbook/) | Install, channels, mobile, custom UI |
| Env vars and ops | [`configuration-reference.md`](configuration-reference.md), [`docs/runbooks/`](runbooks/) | |
| Turn FSM, component design, epics | [`architecture/`](../architecture/) | Living plans; **not** duplicated in `docs/engine/` |
| Shipped milestone history | [`architecture/archive/`](../architecture/archive/) | Historical only |
| ADRs | [`docs/architecture/decisions/`](architecture/decisions/) | Durable decisions |

**Rule:** Code is source of truth. When docs and code disagree, fix the docs (or file a bug if code is wrong).

## Diagram policy

- **Source:** `.mmd` files live in [`architecture/`](../architecture/).
- **Render:** Embed mermaid inline in markdown where it helps integrators (`docs/engine/README.md`, `docs/sdk/README.md`).
- **Do not** commit rendered PNGs unless needed for the product README.

## Staleness banner (for archive / plan docs)

Add at the top of historical plans:

```markdown
> **Historical** — This document describes a shipped or superseded milestone. For current integrator docs see [docs/README.md](../docs/README.md). Active roadmap: [architecture/ROADMAP.md](../architecture/ROADMAP.md).
```

## Per-release checklist

When shipping a user-facing or integrator-facing feature:

1. Add or update route rows in [`docs/engine/http-api.md`](engine/http-api.md).
2. Add or update the subsystem guide under [`docs/engine/`](engine/) if the feature is non-trivial.
3. Add SDK rows in [`docs/sdk/api-reference.md`](sdk/api-reference.md) when `medousa-sdk` exposes a typed method.
4. Add a cookbook recipe if operators or integrators need a task-oriented walkthrough.
5. Update [`docs/README.md`](README.md) index if you add a new top-level guide.
6. Run [`scripts/verify-docs.sh`](../scripts/verify-docs.sh).

## Subsystem coverage checklist

| Subsystem | http-api | engine guide | SDK | cookbook | Status |
|-----------|----------|--------------|-----|----------|--------|
| Interactive streaming | yes | [interactive-streaming.md](engine/interactive-streaming.md) | yes | [custom-chat-ui.md](cookbook/custom-chat-ui.md) | documented |
| Artifacts | yes | [artifacts.md](engine/artifacts.md) | yes | [artifacts-and-presentations.md](cookbook/artifacts-and-presentations.md) | documented |
| Vault / Library | yes | [vault.md](engine/vault.md) | partial | [vault-and-library.md](cookbook/vault-and-library.md) | documented |
| Workspace | yes | [workspace.md](engine/workspace.md) | via `http()` | cli-and-workspace | documented |
| Agent tools | — | [agent-tools.md](engine/agent-tools.md) | — | — | documented |
| Runtime config | yes | [runtime-config.md](engine/runtime-config.md) | partial | configuration-reference | documented |
| Mobile / LAN | partial | — | transports | [mobile-and-lan.md](cookbook/mobile-and-lan.md) | documented |
| Grapheme, Locus, workflows, media, STT | yes | [extensions.md](engine/extensions.md) | via `http()` | configuration-reference | documented |
| medousa-home app | — | — | Tauri transport | [apps/medousa-home.md](apps/medousa-home.md) | documented |

Update this table when adding or changing coverage.
