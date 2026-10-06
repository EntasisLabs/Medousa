# SDK transports

**Audience:** integrator

`MedousaClient` delegates all HTTP to a `Transport` trait — swap implementations for tests, LAN, or Iroh workshop routing.

---

## `HttpTransport` (default)

```rust
use std::sync::Arc;
use medousa_sdk::{HttpTransport, MedousaClient};

let http = reqwest::Client::builder()
    .default_headers(authenticated_headers_from_your_secret_store()?)
    .build()?;
let client = MedousaClient::with_transport(
    Arc::new(HttpTransport::with_client(http)),
    "http://127.0.0.1:7419",
);
```

Uses `reqwest` against `base_url` + path. `HttpTransport::new()` is suitable
only for public `/health` or tests; protected routes require a paired bearer
even when `base_url` is loopback. Mark authorization header values sensitive.

---

## `WorkshopTransport` (`medousa-sdk-iroh`)

Pooled LAN HTTP with optional Iroh fallback (mobile), TTL route cache, and bearer auth from pairing:

```rust
use std::sync::Arc;
use medousa_sdk::{MedousaClient, Transport};
use medousa_sdk_iroh::{WorkshopTransport, WorkshopTransportConfig};

let transport = WorkshopTransport::new(WorkshopTransportConfig::from_workshop_parts(
    "http://192.168.1.10:7419",
    Some("session-token".into()),
    None, // iroh ticket — set on paired mobile clients
));
let client = MedousaClient::with_transport(
    Arc::new(transport) as Arc<dyn Transport>,
    "http://192.168.1.10:7419",
);
```

---

## Tauri custom transport

`apps/medousa-home/src-tauri/src/daemon/sdk.rs` builds a `WorkshopTransport` from `medousa-sdk-iroh` (pooled clients + route cache). Mobile adds a `TauriIrohHook` when an Iroh ticket is present. Multipart / raw byte uploads still call legacy `workshop_transport` helpers.

Diagram: [medousa-client-transport.mmd](../../architecture/medousa-client-transport.mmd)

---

## CLI provider callbacks

`medousa-cli daemon-external-event --iroh-ticket <ticket>` uses the shared
`medousa-iroh-http` client directly. `MEDOUSA_BRIDGE_IROH_TICKET` or `--worker`
can also select this transport. It sends the paired bearer and conversation
callback key over `medousa-http/1`, with no direct HTTP probe or fallback.
This explicit routing differs from `WorkshopTransport`'s LAN-first policy.
Connectivity failures invalidate that transport's route cache. Only GET requests
may be replayed on the other route; a timed-out mutation must be reconciled with
its existing work handle rather than automatically sent again.
See the [provider conversation guide](../guides/provider-conversations.md)
for raw-ticket and saved-worker examples.

---

## Custom `Transport`

Implement `Transport` for mocks or corporate proxies:

```rust
use medousa_sdk::{MedousaClient, SdkError, Transport};
// get_json, post_json, put_json, patch_json, delete_json, post_empty_json
```

Helper: `medousa_sdk::transport::path_with_query`, `arc_transport`.

---

## Streaming transport

With the Rust SDK's `sse` feature, `Transport` also owns `stream_sse` and
`stream_sse_with_accept`. `HttpTransport` accepts relative paths and absolute
daemon `stream_url` responses. `WorkshopTransport` forwards the negotiated
media type over LAN and Iroh, converting an absolute URL to a route path only
for the Iroh hook.

Custom transports that support typed turn stream v2 or v3 must override
`stream_sse_with_accept`; the trait default intentionally rejects media types
other than plain `text/event-stream` instead of silently returning the v1
projection.

## Lightweight external agents

Instinct and Dots can use a scoped external-agent bearer over an ordinary HTTPS gateway.
Alternatively, the [Node 22 example](../../examples/instinct-urspace/client.mjs)
uses Urspace's WASM SDK and a saved private session identity to carry requests
over Iroh. This requires an Urspace host proxying the daemon; a Medousa Iroh
ticket cannot be passed to the Urspace SDK. See the [Instinct guide](../guides/instinct-agent.md),
[Dots guide](../guides/dots.md), and [credential API](../engine/external-conversations.md#instinct-and-dots-api-credentials).
The gateway must preserve Authorization and JSON request/response bodies.

## Iroh connection lifetime and deadlines

The shared `medousa-iroh-http` client retains an endpoint per relay set and a
QUIC connection per workshop peer. Concurrent requests share the connection
but use separate HTTP streams and authentication headers. Switching workshops
or receiving another relay set does not close an active stream. A closed
connection is redialed; HTTP requests are never replayed by this client after
request bytes have been sent.

Iroh health requests have a 10-second total deadline, ordinary GET/HEAD requests
30 seconds, and mutations 120 seconds, including response body reads. SSE has
20 seconds to open and a 75-second idle deadline between body chunks; active
streams can remain open indefinitely. Native Medousa also bounds its remote
health command to 10 seconds across LAN/Iroh route selection. Network-change
notifications preserve paired identity and existing connections. Native direct
connections do not require a healthy relay first.

`medousa_iroh_http::transport_diagnostics()` returns at most 128 recent,
content-free client events. Entries include phase, outcome, duration, a short
public peer identifier, selected direct/relay path, and RTT when available.
Connection closure is recorded when discovered during reuse; snapshots do not
actively probe peers. Tickets, URLs, headers, payloads, and peer-supplied close
reasons are never recorded. This is an in-memory diagnostic history, not a
durable work or delivery receipt.
