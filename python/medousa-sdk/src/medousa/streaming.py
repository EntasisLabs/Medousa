from __future__ import annotations

from collections.abc import AsyncIterator

import httpx


async def iter_sse_events(response: httpx.Response) -> AsyncIterator[str]:
    """Yield SSE `data:` payloads from an open streaming response."""
    data_lines: list[str] = []
    async for line in response.aiter_lines():
        if line == "":
            if data_lines:
                yield "\n".join(data_lines)
                data_lines = []
            continue
        if line.startswith(":"):
            continue
        if line.startswith("data:"):
            data_lines.append(line[5:].lstrip())
    if data_lines:
        yield "\n".join(data_lines)
