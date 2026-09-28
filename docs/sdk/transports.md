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

Instinct can use a scoped external-agent bearer over an ordinary HTTPS gateway.
Alternatively, the [Node 22 example](../../examples/instinct-urspace/client.mjs)
uses Urspace's WASM SDK and a saved private session identity to carry requests
over Iroh. This requires an Urspace host proxying the daemon; a Medousa Iroh
ticket cannot be passed to the Urspace SDK. See the [Instinct guide](../guides/instinct-agent.md)
and [credential API](../engine/external-conversations.md#instinct-api-credentials).
The gateway must preserve Authorization and JSON request/response bodies.
