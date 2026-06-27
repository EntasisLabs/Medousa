# Interactive streaming (SDK)

**Audience:** integrator

Engine contract: [../engine/interactive-streaming.md](../engine/interactive-streaming.md)

Both **Rust** (`sse` feature, default) and **Python** ship built-in SSE clients.

---

## Step 1 — Start turn

```rust
use medousa_types::{InteractiveTurnRequest, InteractiveTurnResponse};

let response: InteractiveTurnResponse = client
    .interactive()
    .start_turn(&InteractiveTurnRequest {
        session_id: "my-session".into(),
        prompt: "Hello".into(),
        ..Default::default()
    })
    .await?;

let stream_url = response.stream_url;
```

---

## Step 2 — Open SSE

### Rust (`medousa-sdk` feature `sse`)

Combined helper:

```rust
use futures_util::StreamExt;
use medousa_types::InteractiveTurnRequest;

let mut events = client
    .interactive()
    .stream_turn(&InteractiveTurnRequest {
        session_id: "my-session".into(),
        prompt: "Hello".into(),
        ..Default::default()
    })
    .await?;

while let Some(event) = events.next().await {
    let event = event?;
    if event.terminal {
        break;
    }
}
```

Or open an existing `stream_url`:

```rust
let mut events = client.interactive().stream(&stream_url).await?;
```

### Python

```python
async with client.interactive().stream_turn(request) as events:
    async for event in events:
        if event.terminal:
            break
```

See [python.md](python.md).

### Tauri desktop

When using `TauriWorkshopTransport`, SSE may require the Tauri bridge (`interactive_stream_start`) instead of the SDK stream helper.

---

## Cancel

```rust
client.interactive().cancel("my-session").await?;
```

```python
await client.interactive().cancel("my-session")
```

---

## Event handling

Deserialize each SSE payload to `InteractiveTurnStreamEvent`. Key cases:

- `content_delta` — append to assistant bubble
- `ui_artifact` — show artifact embed
- `previous_artifact_id` — refresh artifact revision
- `budget_request_id` — show approval UI
- `terminal: true` — close stream

See [custom-chat-ui.md](../cookbook/custom-chat-ui.md).

---

## Local model download SSE

Both SDKs: `local_models().download_events(job_id)` streams `ModelDownloadProgress` events.
