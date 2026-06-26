# Custom chat UI

**Audience:** integrator

Build a chat client against Medousa Engine without the Medousa app UI.

---

## Sessions

```rust
let sessions = client.sessions().list(30).await?;
let history = client.sessions().history("my-session").await?;
```

Create implicit sessions by sending interactive turns with a stable `session_id`.

---

## Streaming turn

1. `client.interactive().start_turn(&InteractiveTurnRequest { ... })`
2. Open `response.stream_url` as SSE
3. Handle `InteractiveTurnStreamEvent` until `terminal`

Details: [interactive-streaming.md](../engine/interactive-streaming.md) · [SDK guide](../sdk/interactive-streaming.md)

---

## Artifact panel

On stream events:

| Event field | UI action |
|-------------|-----------|
| `ui_artifact` | Render new artifact (inline/panel/fullscreen per `presentation`) |
| `previous_artifact_id` | Replace revision in place (`artifact_updated`) |
| `root_artifact_id` | Track lineage for fetch |

Fetch body:

```rust
client.runtime().artifact_fetch(&ArtifactFetchRequest {
    session_id: session_id.clone(),
    artifact_id: artifact_id.clone(),
}).await?;
```

Embed HTML in sandboxed iframe (`sandbox="allow-scripts"`).

---

## Cancel & reconnect

- Cancel: `POST /v1/sessions/{session_id}/active-turn`
- Reconnect: `GET /v1/sessions/{session_id}/active-turn` then re-open stream if still running

[connection-reliability.md](../runbooks/connection-reliability.md)

---

## Reference implementation

`apps/medousa-home/src/lib/stores/chat.svelte.ts` — stream reducer, artifact strip, `artifact_updated` handling.

App doc: [medousa-home.md](../apps/medousa-home.md)
