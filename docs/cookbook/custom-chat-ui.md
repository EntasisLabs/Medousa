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
3. Handle `InteractiveTurnStreamEvent` until `terminal` (or a background
   handoff boundary; see below)

Details: [interactive-streaming.md](../engine/interactive-streaming.md) · [SDK guide](../sdk/interactive-streaming.md)

**Python** (built-in SSE):

```python
async with client.interactive().stream_turn(
    InteractiveTurnRequest(session_id="my-session", prompt="Hello"),
) as events:
    async for event in events:
        if event.content_delta:
            print(event.content_delta, end="")
        if event.terminal:
            break
```

See [python.md](../sdk/python.md).

### Background workshop handoff

`worker_ack` and `workshop_ack` are deliberately non-terminal events. They mean
the host turn has handed the work to a background lane, so a chat composer can
be released immediately even though the workshop is still running. Continue
observing the turn stream for the later `worker_synthesis`/terminal event when
the surface can keep that listener in the background, or reconcile
`GET /v1/sessions/{session_id}/history` when the synthesis is ready.

The shared TypeScript client exposes this host boundary explicitly:

```ts
for await (const event of client.streamTurn(accepted, { stopOnHandoff: true })) {
  // Render the handoff, release the composer, and follow history separately.
}
```

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

**Cancel:** `POST /v1/sessions/{session_id}/active-turn` (SDK: `client.interactive().cancel(session_id)`).

**Reconnect after SSE drop** (before `terminal`):

1. Track `event.seq` on every `InteractiveTurnStreamEvent`.
2. Re-open the same `stream_url` with `?since=<last_seq>` — the daemon replays from its durable turn journal, then tails live.
3. If you lost `turn_id`, poll `GET /v1/sessions/{session_id}/active-turn` first.

**Rust** (recommended):

```rust
let mut events = client
    .interactive()
    .stream_turn_reconnecting(&InteractiveTurnRequest {
        session_id: "my-session".into(),
        prompt: "Hello".into(),
        ..Default::default()
    })
    .await?;
```

**Python:**

```python
async with client.interactive().stream_turn_reconnecting(request) as events:
    async for event in events:
        if event.terminal:
            break
```

**Raw HTTP:** append `?since=42` to the stream path; parse SSE until `terminal`.

[connection-reliability.md](../runbooks/connection-reliability.md) · [SDK interactive streaming](../sdk/interactive-streaming.md)

---

## Reference implementation

`apps/medousa-home/src/lib/stores/chat.svelte.ts` — stream reducer, artifact strip, `artifact_updated` handling.

App doc: [medousa-home.md](../apps/medousa-home.md)
