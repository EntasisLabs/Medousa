# Artifacts

**Audience:** integrator

HTML UI artifacts are versioned documents the agent can present inline, in a side panel, or fullscreen. Integrators interact via **HTTP runtime routes** and/or **agent cognition tools** during turns.

---

## Dual API

| Layer | When to use |
|-------|-------------|
| **Agent tools** (`cognition_store_read` / `cognition_store_write`, `store=artifacts`) | Agent reads/writes HTML during a turn |
| **HTTP** (`/v1/runtime/artifact/*`) | Clients fetch bodies, list catalog, TUI slash commands |

### Agent tools

| Tool | Purpose |
|------|---------|
| `cognition_store_read` | `store=artifacts`, `op=list\|read\|search` (`search` needs artifact id in `path`) |
| `cognition_store_write` | `store=artifacts`, `op=write\|delete` |

Registered via `src/store_tools.rs` (backends in `src/artifact_tools.rs`). Requires `supports_ui_artifacts=true` on the turn surface for artifact ops; vault/code still work without it.

Vault uses the same primitives with `store=vault` — see [vault.md](vault.md).

### HTTP routes

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/v1/runtime/artifact/fetch` | Full HTML body (`ArtifactFetchRequest`) |
| POST | `/v1/runtime/artifact/write` | Create or update HTML revision (`ArtifactWriteRequest`) |
| POST | `/v1/runtime/artifact/delete` | Remove artifact from session store (`ArtifactDeleteRequest`) |
| POST | `/v1/runtime/artifact/list-ui` | Library catalog (`ArtifactListUiRequest`) |
| POST | `/v1/runtime/artifact/command` | TUI slash command DSL (`ArtifactCommandSpec`) |

`fetch` resolves to the **latest revision** in a lineage chain and returns the
body plus portable metadata. Daemon filesystem paths are never part of the
response; use `medousa:artifact/{session_id}/{artifact_id}` when a copyable
reference is needed.

SDK: `runtime().artifact_fetch`, `artifact_write`, `artifact_delete`, `artifact_list_ui`, `artifact_command` — [sdk/artifacts.md](../sdk/artifacts.md)

---

## Versioning

- `supersedes_artifact_id` links revisions
- `root_artifact_id` groups a lineage
- Stream event `artifact_updated` carries `previous_artifact_id` + `root_artifact_id`

Store: `src/artifact_store.rs`

---

## Presentation modes

| Mode | UI behavior |
|------|-------------|
| `inline` | Embedded in chat, height-capped |
| `panel` | Slide-over panel |
| `fullscreen` | Modal overlay (mobile: safe-area chrome) |

Stream field: `ui_artifact.presentation`

---

## Cookbook

[artifacts-and-presentations.md](../cookbook/artifacts-and-presentations.md)
