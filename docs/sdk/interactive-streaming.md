# Interactive streaming (SDK)

**Audience:** integrator

Engine contract: [../engine/interactive-streaming.md](../engine/interactive-streaming.md)

---

## Step 1 — Start turn

```rust
use medousa_types::{InteractiveTurnRequest, InteractiveTurnResponse};

let response: InteractiveTurnResponse = client
    .interactive()
    .start_turn(&InteractiveTurnRequest {
        session_id: "my-session".into(),
        prompt: "Hello".into(),
        // surface, attachments, …
        ..Default::default()
    })
    .await?;

let stream_url = response.stream_url; // e.g. /v1/interactive/turn/{turn_id}/stream
```

---

## Step 2 — Open SSE

The SDK does not ship an SSE client yet. Options:

1. **HTTP client** — `GET {base_url}{stream_url}` with `Accept: text/event-stream`, parse `data:` lines as JSON → `InteractiveTurnStreamEvent`.
2. **Tauri app** — `interactive_stream_start` bridge (`apps/medousa-home`).
3. **`client.http()`** — for non-streaming poll of `GET /v1/sessions/{id}/active-turn`.

---

## Cancel

```rust
// HTTP until SDK wrapper exists:
client.http().post_empty::<serde_json::Value>(
    &format!("/v1/sessions/{session_id}/active-turn"),
).await?;
```

---

## Event handling

Deserialize each SSE payload to `InteractiveTurnStreamEvent`. Key cases:

- `content_delta` — append to assistant bubble
- `ui_artifact` — show artifact embed
- `previous_artifact_id` — refresh artifact revision (`artifact_updated`)
- `budget_request_id` — show approval UI
- `terminal: true` — close stream

Example loop (pseudo-Rust with `eventsource-stream` or your HTTP stack):

```rust
// while let Some(event) = sse.next().await {
//     let ev: InteractiveTurnStreamEvent = serde_json::from_str(&event.data)?;
//     if ev.terminal { break; }
// }
```

See [custom-chat-ui.md](../cookbook/custom-chat-ui.md).
