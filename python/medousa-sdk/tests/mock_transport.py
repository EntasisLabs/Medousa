from __future__ import annotations

from collections.abc import Callable
from typing import Any

import httpx


class MockTransport:
    """In-memory transport for unit tests."""

    def __init__(
        self,
        handlers: dict[tuple[str, str], Callable[..., Any]] | None = None,
    ) -> None:
        self.handlers = handlers or {}
        self.calls: list[tuple[str, str, Any]] = []

    async def get_json(self, base_url: str, path: str) -> Any:
        self.calls.append(("GET", path, None))
        key = ("GET", path)
        if key not in self.handlers:
            raise KeyError(f"No handler for GET {path}")
        return self.handlers[key](base_url, path)

    async def post_json(self, base_url: str, path: str, body: Any) -> Any:
        self.calls.append(("POST", path, body))
        key = ("POST", path)
        if key not in self.handlers:
            raise KeyError(f"No handler for POST {path}")
        return self.handlers[key](base_url, path, body)

    async def put_json(self, base_url: str, path: str, body: Any) -> Any:
        self.calls.append(("PUT", path, body))
        key = ("PUT", path)
        if key not in self.handlers:
            raise KeyError(f"No handler for PUT {path}")
        return self.handlers[key](base_url, path, body)

    async def patch_json(self, base_url: str, path: str, body: Any) -> Any:
        self.calls.append(("PATCH", path, body))
        key = ("PATCH", path)
        if key not in self.handlers:
            raise KeyError(f"No handler for PATCH {path}")
        return self.handlers[key](base_url, path, body)

    async def delete_json(self, base_url: str, path: str) -> Any:
        self.calls.append(("DELETE", path, None))
        key = ("DELETE", path)
        if key not in self.handlers:
            raise KeyError(f"No handler for DELETE {path}")
        return self.handlers[key](base_url, path)

    async def post_empty_json(self, base_url: str, path: str) -> Any:
        self.calls.append(("POST", path, None))
        key = ("POST", path)
        if key not in self.handlers:
            raise KeyError(f"No handler for POST {path}")
        return self.handlers[key](base_url, path, None)

    async def stream_sse(self, base_url: str, path: str) -> httpx.Response:
        self.calls.append(("SSE", path, None))
        key = ("SSE", path)
        if key not in self.handlers:
            raise KeyError(f"No handler for SSE {path}")
        return self.handlers[key](base_url, path)
