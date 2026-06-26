from __future__ import annotations

from typing import Any, TypeVar

from pydantic import BaseModel

from medousa._decode import decode
from medousa.client import MedousaClient
from medousa.transport import path_with_query

T = TypeVar("T", bound=BaseModel)


class HttpApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def get(self, path: str, *, model: type[T]) -> T:
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(model, value)

    async def get_query(
        self,
        path: str,
        query: list[tuple[str, str]],
        *,
        model: type[T],
    ) -> T:
        return await self.get(path_with_query(path, query), model=model)

    async def post(self, path: str, body: Any, *, model: type[T]) -> T:
        value = await self._client.transport.post_json(
            self._client.base_url,
            path,
            _serialize(body),
        )
        return decode(model, value)

    async def post_empty(self, path: str, *, model: type[T]) -> T:
        value = await self._client.transport.post_empty_json(self._client.base_url, path)
        return decode(model, value)

    async def put(self, path: str, body: Any, *, model: type[T]) -> T:
        value = await self._client.transport.put_json(
            self._client.base_url,
            path,
            _serialize(body),
        )
        return decode(model, value)

    async def patch(self, path: str, body: Any, *, model: type[T]) -> T:
        value = await self._client.transport.patch_json(
            self._client.base_url,
            path,
            _serialize(body),
        )
        return decode(model, value)

    async def delete(self, path: str, *, model: type[T]) -> T:
        value = await self._client.transport.delete_json(self._client.base_url, path)
        return decode(model, value)

    async def post_raw(self, path: str, body: Any) -> Any:
        return await self._client.transport.post_json(
            self._client.base_url,
            path,
            _serialize(body),
        )

    async def delete_raw(self, path: str) -> Any:
        return await self._client.transport.delete_json(self._client.base_url, path)


def _serialize(body: Any) -> Any:
    if isinstance(body, BaseModel):
        return body.model_dump(mode="json", exclude_none=True)
    return body
