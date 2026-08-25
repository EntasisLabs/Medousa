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

The legacy `stream` helper intentionally receives the v1 DTO. Opt into the
typed v2 envelope with `stream_v2`; it sends
`Accept: text/event-stream; medousa-version=2`:

```rust
use futures_util::StreamExt;
use medousa_types::TurnStreamEventV2;

let mut events = client.interactive().stream_v2(&stream_url);
while let Some(envelope) = events.next().await {
    let envelope = envelope?;
    match envelope.event {
        TurnStreamEventV2::ContentAppend { text } => print!("{text}"),
        TurnStreamEventV2::Final { text, .. } => {
            println!("{text}");
            break;
        }
        _ => {}
    }
}
```

`stream_v2` is one-shot. Use the typed reconnecting helpers below for durable
replay across connection drops.

### Reconnecting chronological v3 stream (recommended)

V3 returns the raw timeline facts in observation order while leaving filtering,
forking, aggregation, and presentation choices to the client:

```rust
use futures_util::StreamExt;
use medousa_types::{InteractiveTurnRequest, TurnStreamEventV3};

let mut events = client
    .interactive()
    .stream_turn_reconnecting_v3(&InteractiveTurnRequest {
        session_id: "my-session".into(),
        prompt: "Hello".into(),
        ..Default::default()
    })
    .await?;

while let Some(event) = events.next().await {
    let envelope = event?;
    match envelope.event {
        TurnStreamEventV3::ContentAppend { segment_id, text } => {
            consume_text(segment_id, text);
        }
        TurnStreamEventV3::TurnCompleted { .. } => break,
        _ => {}
    }
}
```

Use `stream_v3` for a one-shot connection or
`stream_reconnecting_v3_with_policy` for an existing stream URL and custom
reconnect policy.

### Reconnecting typed v2 compatibility stream

Tracks `event.seq`, reattaches with `?since=<last_seq>` after drops, and applies bounded exponential backoff + circuit breaker + overlap guard.

```rust
use futures_util::StreamExt;
use medousa_types::InteractiveTurnRequest;

let mut events = client
    .interactive()
    .stream_turn_reconnecting_v2(&InteractiveTurnRequest {
        session_id: "my-session".into(),
        prompt: "Hello".into(),
        ..Default::default()
    })
    .await?;

while let Some(event) = events.next().await {
    let envelope = event?;
    // `seq` is monotonic per turn; duplicates after replay are deduped client-side.
    if envelope.event.is_terminal() {
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
    .stream_reconnecting_v2_with_policy(&stream_url, policy);
```

Helper: `medousa_sdk::stream_path_with_since("/v1/interactive/turn/t1/stream", 42)` → `...?since=42`.
Absolute `stream_url` values retain their scheme and authority when the cursor
is replaced; `HttpTransport` accepts both absolute daemon responses and
relative paths.

The unsuffixed `stream_reconnecting*` and `stream_turn_reconnecting` methods
remain frozen v1 compatibility adapters during the support window.

### Python

One-shot typed v2:

```python
response = await client.interactive().start_turn(request)
async with client.interactive().stream_v2(response.stream_url) as events:
    async for event in events:
        handle(event)
```

Reconnecting (spine replay):

```python
async with client.interactive().stream_turn_reconnecting_v2(request) as events:
    async for event in events:
        handle(event)
```

Or open an existing URL:

```python
async for event in client.interactive().stream_reconnecting_v2(stream_url):
    ...
```

The v2 iterator stops on the typed terminal variants. Unsuffixed Python stream
helpers retain the frozen v1 projection for compatibility only.

### TypeScript

The dependency-free `@medousa/client` package uses the typed v2 protocol for
first-party browser, editor, and vault surfaces:

```ts
import {
  createTurnStreamProjectionState,
  MedousaClient,
  projectTurnStreamEvent,
} from "@medousa/client";

const client = new MedousaClient({ baseUrl, bearerToken });
const response = await client.startTurn(request);
const projection = createTurnStreamProjectionState();

for await (const envelope of client.streamTurnV2(response)) {
  for (const event of projectTurnStreamEvent(envelope, projection)) {
    render(event);
  }
}
```

`streamTurnV2` sends the v2 media type, reconnects with `?since=<last_seq>`,
drops replay overlap, and stops on typed terminal variants. Pass
`{ stopOnHandoff: true }` when the host should release its foreground composer
after a worker/workshop acknowledgment. `streamTurn` is the frozen v1
compatibility adapter and should not be used by new first-party code.

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

Legacy clients deserialize each SSE payload to `InteractiveTurnStreamEvent`.
V2 clients deserialize `TurnStreamEnvelopeV2` and switch exhaustively on
`envelope.event.type`; terminal state is represented by typed variants instead
of the legacy `terminal` boolean. Key legacy fields:

| Field | Meaning |
|-------|---------|
| `seq` | Monotonic per-turn sequence (use for reconnect cursor) |
| `content_delta` | Append to assistant bubble |
| `ui_artifact` | Show artifact embed |
| `terminal` | Turn finished — stop reading |

`worker_ack` and `workshop_ack` are non-terminal host handoff events. A surface
should release its composer at that boundary while the background lane
continues. Follow the stream for the later synthesis when possible, or reload
the session history after the workshop result is committed.

See [custom-chat-ui.md](../cookbook/custom-chat-ui.md).

---

## Tauri / workshop transport

`medousa-home` routes JSON + SSE through [`medousa-sdk-iroh`](../../crates/medousa-sdk-iroh/) (`WorkshopTransport`), which forwards the negotiated v2 media type over both LAN and Iroh routes. Reconnect discipline for the webview lives in [`apps/medousa-home/src/lib/stream/reconnect.ts`](../../apps/medousa-home/src/lib/stream/reconnect.ts) — bounded backoff, overlap guard, and `?since=<seq>` replay aligned with the Rust/Python SDK helpers. Multipart uploads still use the legacy `workshop_transport` helpers.

The first-party TUI and channel adapter ingest paths use the Rust SDK's typed
v2 reconnect stream directly. They do not maintain separate string-buffer SSE
parsers or project through `InteractiveTurnStreamEvent`; replay overlap and
terminal detection therefore follow the same SDK state machine as integrators.

---

## Local model download SSE

Both SDKs: `local_models().download_events(job_id)` streams `ModelDownloadProgress` events (separate from interactive turns).
