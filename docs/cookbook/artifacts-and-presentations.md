# Artifacts & presentations

**Audience:** integrator, operator

HTML presentations created by the agent and browsed in Library.

---

## During chat

The agent uses `cognition_artifact_write` to create/revise HTML. Stream emits `ui_artifact` or `artifact_updated` events.

Presentation modes: `inline`, `panel`, `fullscreen`.

Engine: [artifacts.md](../engine/artifacts.md)

---

## Library (Presentations tab)

Desktop: Library → **Presentations**  
Mobile: You → Library → **Presentations**

Lists artifacts via `POST /v1/runtime/artifact/list-ui`:

```rust
client.runtime().artifact_list_ui(&ArtifactListUiRequest {
    session_id: None,
    limit: 100,
    query: None,
}).await?;
```

Preview uses `artifact_fetch` for HTML body.

---

## Versioning

Writes include `if_match_hash64` and `supersedes_artifact_id`. `fetch` always resolves latest revision in a lineage.

---

## Agent tools (turn-time)

| Tool | Use |
|------|-----|
| `cognition_artifact_list` | Discover artifacts in session |
| `cognition_artifact_read` | Read with optional line range |
| `cognition_artifact_grep` | Search HTML source |
| `cognition_artifact_write` | Publish new revision |

Requires `supports_ui_artifacts` on the turn surface.

---

## TUI slash commands

`POST /v1/runtime/artifact/command` — same DSL as TUI `/artifact` commands. SDK: `runtime().artifact_command()`.
