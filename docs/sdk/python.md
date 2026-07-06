# Python SDK

**Audience:** integrator

Async-first HTTP client for **medousa_daemon**, mirroring the Rust [`medousa-sdk`](../../crates/medousa-sdk/) accessor layout.

Rust reference: [README.md](README.md) · [API reference](api-reference.md)

Contract: [`../../sdk-contract/manifest.yaml`](../../sdk-contract/manifest.yaml)

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

Same names as Rust `MedousaClient` (full table in [api-reference.md](api-reference.md)):

| Accessor | Notes |
|----------|-------|
| `health()`, `http()`, `ingest()` | Core |
| `local_models()` | Hardware, catalog, downloads, SSE progress |
| `jobs()`, `recurring()` | Headless jobs + cron |
| `sessions()`, `interactive()` | Chat sessions + streaming turns |
| `runtime()` | Artifacts, config, stage routing |
| `capabilities()`, `mcp_gateway()`, `budget()` | Catalog + gateway + budget |
| `vault()`, `workspace()` | Library + work board |

---

## Interactive streaming

Both Rust (`sse` feature, default) and Python ship built-in SSE clients:

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

Cancel: `await client.interactive().cancel("my-session")`

**Reconnecting** (durable spine replay via `?since=<seq>`):

```python
async with client.interactive().stream_turn_reconnecting(
    InteractiveTurnRequest(session_id="my-session", prompt="Hello"),
) as events:
    async for event in events:
        if event.content_delta:
            print(event.content_delta, end="", flush=True)
        if event.terminal:
            break
```

Or open an existing `stream_url`: `client.interactive().stream_reconnecting(url)`.

See [interactive-streaming.md](interactive-streaming.md).

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

---

## Sync client

Accessor-based blocking client (mirrors Rust `BlockingMedousaClient`):

```python
from medousa import MedousaClientSync

with MedousaClientSync("http://127.0.0.1:7419") as client:
    health = client.health().get()
    roots = client.vault().list_roots()
```

SSE is async-only.

---

## Types (codegen from Rust)

Python types are **generated** from `medousa-types` JSON Schema — not hand-maintained.

After changing Rust DTOs:

```bash
cargo run -p medousa-types-schema          # writes sdk-contract/medousa-types.schema.json
python scripts/gen-python-types.py         # writes python/medousa-sdk/src/medousa/types/_generated/
```

Import generated models:

```python
from medousa.types import HealthResponse, JobResultResponse, VaultRootsResponse
```

CI fails if `_generated/` is stale relative to the schema export.

---

## Tests & parity

```bash
cd python/medousa-sdk
ruff check .
pytest
```

`tests/test_parity_paths.py` loads `sdk-contract/manifest.yaml` and validates every method exists on async + sync clients.

Repository-wide: `bash scripts/check-sdk-contract.sh`

---

## Remaining gaps

Identity, grapheme, workflows — use `client.http()` until wrapped. See [api-reference.md](api-reference.md).
