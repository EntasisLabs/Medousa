from __future__ import annotations

import httpx

from medousa.error import SdkError
from medousa.transport import normalize_path


class SyncTransport:
    def __init__(self, bearer_token: str | None = None) -> None:
        headers = {}
        if bearer_token:
            headers["Authorization"] = f"Bearer {bearer_token}"
        self._client = httpx.Client(timeout=httpx.Timeout(120.0, connect=10.0), headers=headers)

    def close(self) -> None:
        self._client.close()

    def _url(self, base_url: str, path: str) -> str:
        return f"{base_url.rstrip('/')}{normalize_path(path)}"

    def _raise(self, response: httpx.Response) -> None:
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

    def get_json(self, base_url: str, path: str):
        response = self._client.get(self._url(base_url, path))
        self._raise(response)
        return response.json()

    def post_json(self, base_url: str, path: str, body):
        response = self._client.post(self._url(base_url, path), json=body)
        self._raise(response)
        return response.json() if response.content else {}

    def put_json(self, base_url: str, path: str, body):
        response = self._client.put(self._url(base_url, path), json=body)
        self._raise(response)
        return response.json()

    def patch_json(self, base_url: str, path: str, body):
        response = self._client.patch(self._url(base_url, path), json=body)
        self._raise(response)
        return response.json()

    def delete_json(self, base_url: str, path: str):
        response = self._client.delete(self._url(base_url, path))
        self._raise(response)
        return response.json() if response.content else {}

    def post_empty_json(self, base_url: str, path: str):
        response = self._client.post(self._url(base_url, path))
        self._raise(response)
        return response.json() if response.content else {}
