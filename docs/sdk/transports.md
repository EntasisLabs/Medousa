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

Authenticated LAN workshop base URL (bearer token from pairing):

```rust
use medousa_sdk_iroh::WorkshopTransport;

let transport = WorkshopTransport::from_lan_base("http://192.168.1.10:7419");
let client = MedousaClient::with_transport(Arc::new(transport), "http://192.168.1.10:7419");
```

---

## Tauri custom transport

`apps/medousa-home/src-tauri/src/daemon/sdk.rs` implements `Transport` by calling `workshop_transport` (LAN with Iroh failover). All typed `runtime().artifact_*` calls go through this stack.

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
