# Interactive streaming (SDK)

**Audience:** integrator

Engine contract: [../engine/interactive-streaming.md](../engine/interactive-streaming.md)

Both **Rust** (`sse` feature, default) and **Python** ship built-in SSE clients. Since Phase 1, the daemon journals every turn event to a **durable spine**; SSE reconnect with `?since=<seq>` replays missed events from disk (not an in-memory ring buffer).

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

### One-shot stream (no reconnect)

```rust
use futures_util::StreamExt;

let mut events = client.interactive().stream(&stream_url);
while let Some(event) = events.next().await {
    let event = event?;
    if event.terminal {
        break;
    }
}
```

### Reconnecting stream (recommended)

Tracks `event.seq`, reattaches with `?since=<last_seq>` after drops, and applies bounded exponential backoff + circuit breaker + overlap guard.

```rust
use futures_util::StreamExt;
use medousa_types::InteractiveTurnRequest;

let mut events = client
    .interactive()
    .stream_turn_reconnecting(&InteractiveTurnRequest {
        session_id: "my-session".into(),
        prompt: "Hello".into(),
        ..Default::default()
    })
    .await?;

while let Some(event) = events.next().await {
    let event = event?;
    // `seq` is monotonic per turn; duplicates after replay are deduped client-side.
    if event.terminal {
        break;
    }
}
```

Open an existing `stream_url` with reconnect policy:

```rust
use medousa_sdk::ReconnectPolicy;

let policy = ReconnectPolicy::default();
let mut events = client
    .interactive()
    .stream_reconnecting_with_policy(&stream_url, policy);
```

Helper: `medousa_sdk::stream_path_with_since("/v1/interactive/turn/t1/stream", 42)` → `...?since=42`.

### Python

One-shot:

```python
async with client.interactive().stream_turn(request) as events:
    async for event in events:
        if event.terminal:
            break
```

Reconnecting (spine replay):

```python
async with client.interactive().stream_turn_reconnecting(request) as events:
    async for event in events:
        if event.terminal:
            break
```

Or open an existing URL:

```python
async for event in client.interactive().stream_reconnecting(stream_url):
    ...
```

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

Deserialize each SSE payload to `InteractiveTurnStreamEvent`. Key fields:

| Field | Meaning |
|-------|---------|
| `seq` | Monotonic per-turn sequence (use for reconnect cursor) |
| `content_delta` | Append to assistant bubble |
| `ui_artifact` | Show artifact embed |
| `terminal` | Turn finished — stop reading |

See [custom-chat-ui.md](../cookbook/custom-chat-ui.md).

---

## Tauri / workshop transport

`medousa-home` routes JSON + SSE through [`medousa-sdk-iroh`](../../crates/medousa-sdk-iroh/) (`WorkshopTransport`). Reconnect discipline for the webview lives in [`apps/medousa-home/src/lib/stream/reconnect.ts`](../../apps/medousa-home/src/lib/stream/reconnect.ts) — bounded backoff, overlap guard, and `?since=<seq>` replay aligned with the Rust/Python SDK helpers. Multipart uploads still use the legacy `workshop_transport` helpers.

---

## Local model download SSE

Both SDKs: `local_models().download_events(job_id)` streams `ModelDownloadProgress` events (separate from interactive turns).
