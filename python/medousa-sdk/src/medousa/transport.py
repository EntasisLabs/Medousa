from __future__ import annotations

from typing import Any, Protocol, runtime_checkable
from urllib.parse import urlencode

import httpx

from medousa.error import SdkError


def normalize_path(path: str) -> str:
    return path if path.startswith("/") else f"/{path}"


def path_with_query(path: str, query: list[tuple[str, str]]) -> str:
    path = normalize_path(path)
    if not query:
        return path
    encoded = urlencode([(k, v) for k, v in query])
    return f"{path}?{encoded}"


@runtime_checkable
class Transport(Protocol):
    async def get_json(self, base_url: str, path: str) -> Any: ...

    async def post_json(self, base_url: str, path: str, body: Any) -> Any: ...

    async def put_json(self, base_url: str, path: str, body: Any) -> Any: ...

    async def patch_json(self, base_url: str, path: str, body: Any) -> Any: ...

    async def delete_json(self, base_url: str, path: str) -> Any: ...

    async def post_empty_json(self, base_url: str, path: str) -> Any: ...

    async def stream_sse(self, base_url: str, path: str) -> httpx.Response: ...


class HttpTransport:
    """Default httpx-based transport."""

    def __init__(self, client: httpx.AsyncClient | None = None) -> None:
        self._client = client
        self._owned = client is None

    async def _client_or_create(self) -> httpx.AsyncClient:
        if self._client is None:
            self._client = httpx.AsyncClient(timeout=httpx.Timeout(120.0, connect=10.0))
        return self._client

    async def aclose(self) -> None:
        if self._owned and self._client is not None:
            await self._client.aclose()
            self._client = None

    def _raise_for_status(self, response: httpx.Response) -> None:
        if response.is_success:
            return
        try:
            body = response.json()
        except Exception:
            body = response.text
        raise SdkError(
            f"HTTP request failed: {response.request.method} {response.request.url}",
            status_code=response.status_code,
            body=body,
        )

    async def get_json(self, base_url: str, path: str) -> Any:
        client = await self._client_or_create()
        response = await client.get(f"{base_url.rstrip('/')}{normalize_path(path)}")
        self._raise_for_status(response)
        return response.json()

    async def post_json(self, base_url: str, path: str, body: Any) -> Any:
        client = await self._client_or_create()
        response = await client.post(
            f"{base_url.rstrip('/')}{normalize_path(path)}",
            json=body,
        )
        self._raise_for_status(response)
        if response.status_code == 204:
            return {}
        return response.json()

    async def put_json(self, base_url: str, path: str, body: Any) -> Any:
        client = await self._client_or_create()
        response = await client.put(
            f"{base_url.rstrip('/')}{normalize_path(path)}",
            json=body,
        )
        self._raise_for_status(response)
        return response.json()

    async def patch_json(self, base_url: str, path: str, body: Any) -> Any:
        client = await self._client_or_create()
        response = await client.patch(
            f"{base_url.rstrip('/')}{normalize_path(path)}",
            json=body,
        )
        self._raise_for_status(response)
        return response.json()

    async def delete_json(self, base_url: str, path: str) -> Any:
        client = await self._client_or_create()
        response = await client.delete(f"{base_url.rstrip('/')}{normalize_path(path)}")
        self._raise_for_status(response)
        if response.status_code == 204 or not response.content:
            return {}
        return response.json()

    async def post_empty_json(self, base_url: str, path: str) -> Any:
        client = await self._client_or_create()
        response = await client.post(f"{base_url.rstrip('/')}{normalize_path(path)}")
        self._raise_for_status(response)
        if not response.content:
            return {}
        return response.json()

    async def stream_sse(self, base_url: str, path: str) -> httpx.Response:
        client = await self._client_or_create()
        request = client.build_request(
            "GET",
            f"{base_url.rstrip('/')}{normalize_path(path)}",
            headers={"Accept": "text/event-stream"},
        )
        response = await client.send(request, stream=True)
        self._raise_for_status(response)
        return response


class WorkshopTransport(HttpTransport):
    """LAN workshop transport with bearer token (mirrors medousa-sdk-iroh)."""

    def __init__(self, bearer_token: str, client: httpx.AsyncClient | None = None) -> None:
        super().__init__(client)
        self._headers = {"Authorization": f"Bearer {bearer_token}"}

    async def _request(self, method: str, url: str, **kwargs: Any) -> httpx.Response:
        client = await self._client_or_create()
        headers = {**self._headers, **kwargs.pop("headers", {})}
        response = await client.request(method, url, headers=headers, **kwargs)
        self._raise_for_status(response)
        return response

    async def get_json(self, base_url: str, path: str) -> Any:
        url = f"{base_url.rstrip('/')}{normalize_path(path)}"
        response = await self._request("GET", url)
        return response.json()

    async def post_json(self, base_url: str, path: str, body: Any) -> Any:
        url = f"{base_url.rstrip('/')}{normalize_path(path)}"
        response = await self._request("POST", url, json=body)
        return response.json() if response.content else {}

    async def put_json(self, base_url: str, path: str, body: Any) -> Any:
        url = f"{base_url.rstrip('/')}{normalize_path(path)}"
        response = await self._request("PUT", url, json=body)
        return response.json()

    async def patch_json(self, base_url: str, path: str, body: Any) -> Any:
        url = f"{base_url.rstrip('/')}{normalize_path(path)}"
        response = await self._request("PATCH", url, json=body)
        return response.json()

    async def delete_json(self, base_url: str, path: str) -> Any:
        url = f"{base_url.rstrip('/')}{normalize_path(path)}"
        response = await self._request("DELETE", url)
        return response.json() if response.content else {}

    async def post_empty_json(self, base_url: str, path: str) -> Any:
        url = f"{base_url.rstrip('/')}{normalize_path(path)}"
        response = await self._request("POST", url)
        return response.json() if response.content else {}

    async def stream_sse(self, base_url: str, path: str) -> httpx.Response:
        url = f"{base_url.rstrip('/')}{normalize_path(path)}"
        client = await self._client_or_create()
        response = await client.send(
            client.build_request(
                "GET",
                url,
                headers={**self._headers, "Accept": "text/event-stream"},
            ),
            stream=True,
        )
        self._raise_for_status(response)
        return response
