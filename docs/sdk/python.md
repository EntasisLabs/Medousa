# Python SDK

**Audience:** integrator

Async-first HTTP client for **medousa_daemon**, mirroring the Rust [`medousa-sdk`](../../crates/medousa-sdk/) accessor layout.

Rust reference: [README.md](README.md) · [API reference](api-reference.md)

---

## Install

From the monorepo (development):

```bash
pip install -e "python/medousa-sdk[dev]"
```

PyPI package name: `medousa-sdk` — import as `medousa`.

---

## Quick start

```python
import asyncio
from medousa import MedousaClient

async def main():
    client = MedousaClient("http://127.0.0.1:7419")
    health = await client.health().get()
    print(health.status, health.backend)
    sessions = await client.sessions().list(20)
    print(len(sessions.sessions))
    await client.aclose()

asyncio.run(main())
```

---

## Accessors

Same names as Rust `MedousaClient`:

| Accessor | Module | Notes |
|----------|--------|-------|
| `health()` | `health.py` | `get()` |
| `http()` | `http.py` | Generic JSON GET/POST/PUT/PATCH/DELETE |
| `ingest()` | `ingest.py` | Channel ingest |
| `local_models()` | `local_models.py` | Hardware, catalog, downloads |
| `jobs()` | `jobs.py` | `enqueue_ask` |
| `recurring()` | `recurring.py` | `register_prompt` |
| `sessions()` | `sessions.py` | List, history, display name, append turn |
| `interactive()` | `interactive.py` | `start_turn`, `stream_turn`, `cancel` |
| `runtime()` | `runtime.py` | Artifacts, config, stage-route commands |
| `capabilities()` | `capabilities.py` | List, get, reindex |
| `mcp_gateway()` | `mcp_gateway.py` | Gateway status |
| `budget()` | `budget.py` | List, approve, deny |

Full method table: [api-reference.md](api-reference.md) (Rust-oriented; paths are identical).

---

## Interactive streaming

Python includes a first-class SSE client (Rust SDK still documents manual SSE).

```python
from medousa import MedousaClient
from medousa.types import InteractiveTurnRequest

async def chat(client: MedousaClient):
    async with client.interactive().stream_turn(
        InteractiveTurnRequest(session_id="my-session", prompt="Hello"),
    ) as events:
        async for event in events:
            if event.content_delta:
                print(event.content_delta, end="", flush=True)
            if event.terminal:
                break
```

Cancel an active turn:

```python
await client.interactive().cancel("my-session")
```

See [interactive-streaming.md](interactive-streaming.md) and [../engine/interactive-streaming.md](../engine/interactive-streaming.md).

---

## Workshop / LAN auth

```python
from medousa import MedousaClient
from medousa.transport import WorkshopTransport

client = MedousaClient(
    "http://192.168.1.10:7419",
    transport=WorkshopTransport(bearer_token="..."),
)
```

No Iroh wire protocol in Python v1 — use a known HTTP base URL on LAN or tailnet.

---

## Sync client

For scripts and notebooks:

```python
from medousa import MedousaClientSync

with MedousaClientSync("http://127.0.0.1:7419") as client:
    health = client.health_get()
    progress = client.local_models_download_status("job-id")
```

---

## Types

Pydantic v2 models in `medousa.types`, aligned with `medousa-types` Rust DTOs. Local/MCP JSON uses `camelCase` where the daemon does.

---

## HTTP-only routes

Vault, workspace, identity, grapheme, and other routes without typed wrappers: use `client.http().get/post(...)` with paths from [../engine/http-api.md](../engine/http-api.md).

---

## Tests & parity

```bash
cd python/medousa-sdk
ruff check .
pytest
```

`tests/test_parity_paths.py` guards route drift against [api-reference.md](api-reference.md).
