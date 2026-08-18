# Artifacts

**Audience:** integrator, operator

HTML artifacts created by the agent and browsed from the **Artifacts** rail door.

---

## During chat

The agent uses `cognition_store_write` (`store=artifacts`) to create/revise HTML. Stream emits `ui_artifact` or `artifact_updated` events.

Presentation modes: `inline`, `panel`, `fullscreen`.

Engine: [artifacts.md](../engine/artifacts.md)

---

## Artifacts door

Desktop: **Artifacts** rail door  
Mobile: You → Library → **Artifacts**

Lists artifacts via `POST /v1/runtime/artifact/list-ui`:

```rust
client.runtime().artifact_list_ui(&ArtifactListUiRequest {
    session_id: None,
    limit: 100,
    query: None,
}).await?;
```

Preview uses `artifact_fetch` for the HTML body. Copy/share actions use the
portable `medousa:artifact/{session_id}/{artifact_id}` reference rather than a
daemon-local filesystem path, so the same reference works with remote workshops.

---

## Versioning

Writes include `if_match_hash64` and `supersedes_artifact_id`. `fetch` always resolves latest revision in a lineage.

---

## Agent tools (turn-time)

| Tool | Use |
|------|-----|
| `cognition_store_read` | `store=artifacts` — list, read, or search HTML |
| `cognition_store_write` | `store=artifacts` — publish a revision or delete |

Requires `supports_ui_artifacts` on the turn surface.

---

## TUI slash commands

`POST /v1/runtime/artifact/command` — same DSL as TUI `/artifact` commands. SDK: `runtime().artifact_command()`.
