# medousa-app-core

Renderer-agnostic **application** core for Medousa Home.

## Ownership

| Layer | Owns |
|-------|------|
| Workshop daemon + `medousa-types` | Durable records, execution, permissions, filesystem authority |
| **`medousa-app-core`** | Chat lifecycle, live↔persisted reconciliation, stable identities, tool/agent relationships, recovery |
| Tauri `app_chat` host | Process-local `ChatCore`, IPC commands |
| Svelte / future renderers | Layout, a11y, scrolling, input widgets, paint |

Daemon API and application API stay separate. This crate does not become a competing workshop authority.

## Slices

Canonical chat timeline:

1. Load persisted history (`ApplyHistory` / `ProjectHistory`)
2. Incorporate live stream events (`ApplyStreamEvent`)
3. Reconcile live and persisted representations (`merge_transcript`)
4. Reconnect / session switch with revision guards
5. Stable turn/item identities and ordering
6. Tool and agent/A2A relationships on the snapshot
7. Agent/task turn placement (`LinkWorker`, `ApplyProposals`, `ApplyVisibleItems`,
   `ProjectAgentPlacement`) — anchors, proposal threading, mixed spawn order;
   snapshot `agent_groups` are thin ids the renderer joins to full rows
8. Worker-lane bubble lifecycle (`EnsureWorkerFollowUp`, `FinalizeWorkerHandoff`,
   `MarkWorkerSynthesisDelivered`, `DeliverWorkerSynthesis`) plus production
   `BeginTurn` / `SettleTurn` dual-write; snapshot `workers` carries synthesis links
9. Ask-lane hydrate / promote (`HydrateAskThreads`, `PromoteAskToChat`) merges
   ask ledger history into the focused session and flips lane identity on promote
10. Painted visibility (`ProjectVisibleItems`) — empty handoff shells and exact
    linked worker raw returns; beat grouping stays in the renderer

## Regenerate client types

```bash
cargo run -p medousa-app-core --features json-schema --bin medousa-app-core-schema
python3 scripts/gen-app-api-types.py
bash scripts/check-app-api.sh
```

## Headless tests

```bash
cargo test -p medousa-app-core --tests
```

## Host integration

Tauri commands: `app_chat_dispatch`, `app_chat_open_session`, `app_chat_snapshot`.

Vitest installs a Node stdio backend (`apps/medousa-home/vitest/`) that drives
the same `ChatCore` via `app-chat-dispatch`. That harness stays outside `$lib`
so the production Vite client never bundles Node builtins.
