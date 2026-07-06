# SDK transports

**Audience:** integrator

`MedousaClient` delegates all HTTP to a `Transport` trait — swap implementations for tests, LAN, or Iroh workshop routing.

---

## `HttpTransport` (default)

```rust
use std::sync::Arc;
use medousa_sdk::{HttpTransport, MedousaClient};

let client = MedousaClient::new("http://127.0.0.1:7419");
// equivalent to with_transport(Arc::new(HttpTransport::new()), url)
```

Uses `reqwest` against `base_url` + path.

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

## Custom `Transport`

Implement `Transport` for mocks or corporate proxies:

```rust
use medousa_sdk::{MedousaClient, SdkError, Transport};
// get_json, post_json, put_json, patch_json, delete_json, post_empty_json
```

Helper: `medousa_sdk::transport::path_with_query`, `arc_transport`.

---

## Streaming limitation

`Transport` is **JSON-only**. SSE streams (interactive turn, workspace, ingest) require a separate HTTP stream client or app-specific bridge.
