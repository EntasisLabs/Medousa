from __future__ import annotations

from typing import TYPE_CHECKING
from urllib.parse import quote

from medousa._decode import decode
from medousa.transport import path_with_query
from medousa.types import (
    ComponentRuntimeEventsRequest,
    ComponentRuntimeEventsResponse,
    ComponentRuntimeEventsTailResponse,
    ComponentRuntimeProbeResult,
    ComponentStoreDeleteResponse,
    ComponentStoreGetResponse,
    ComponentStoreListResponse,
    ComponentStoreSetRequest,
    ComponentStoreSetResponse,
)

if TYPE_CHECKING:
    from medousa.client import MedousaClient


def _profile_query(profile_id: str | None) -> list[tuple[str, str]]:
    return [("profile_id", profile_id)] if profile_id is not None else []


def _store_query(
    profile_id: str | None,
    key: str | None,
) -> list[tuple[str, str]]:
    query = _profile_query(profile_id)
    if key is not None:
        query.append(("key", key))
    return query


class ComponentsApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def store_get(
        self,
        component_id: str,
        *,
        profile_id: str | None = None,
        key: str | None = None,
    ) -> ComponentStoreGetResponse:
        path = path_with_query(
            f"/v1/components/{component_id.strip()}/store",
            _store_query(profile_id, key),
        )
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(ComponentStoreGetResponse, value)

    async def store_set(
        self,
        component_id: str,
        key: str,
        request: ComponentStoreSetRequest,
    ) -> ComponentStoreSetResponse:
        path = path_with_query(
            f"/v1/components/{component_id.strip()}/store",
            _store_query(None, key),
        )
        value = await self._client.transport.put_json(
            self._client.base_url,
            path,
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(ComponentStoreSetResponse, value)

    async def store_list_keys(
        self,
        component_id: str,
        *,
        profile_id: str | None = None,
    ) -> ComponentStoreListResponse:
        path = path_with_query(
            f"/v1/components/{component_id.strip()}/store/keys",
            _profile_query(profile_id),
        )
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(ComponentStoreListResponse, value)

    async def store_get_key(
        self,
        component_id: str,
        key: str,
        *,
        profile_id: str | None = None,
    ) -> ComponentStoreGetResponse:
        encoded_key = quote(key.strip(), safe="")
        path = path_with_query(
            f"/v1/components/{component_id.strip()}/store/{encoded_key}",
            _profile_query(profile_id),
        )
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(ComponentStoreGetResponse, value)

    async def store_put_key(
        self,
        component_id: str,
        key: str,
        request: ComponentStoreSetRequest,
    ) -> ComponentStoreSetResponse:
        encoded_key = quote(key.strip(), safe="")
        value = await self._client.transport.put_json(
            self._client.base_url,
            f"/v1/components/{component_id.strip()}/store/{encoded_key}",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(ComponentStoreSetResponse, value)

    async def store_delete_key(
        self,
        component_id: str,
        key: str,
        *,
        profile_id: str | None = None,
    ) -> ComponentStoreDeleteResponse:
        encoded_key = quote(key.strip(), safe="")
        path = path_with_query(
            f"/v1/components/{component_id.strip()}/store/{encoded_key}",
            _profile_query(profile_id),
        )
        value = await self._client.transport.delete_json(self._client.base_url, path)
        return decode(ComponentStoreDeleteResponse, value)

    async def runtime_tail_events(
        self,
        component_id: str,
        *,
        profile_id: str | None = None,
        limit: int | None = None,
    ) -> ComponentRuntimeEventsTailResponse:
        query = _profile_query(profile_id)
        if limit is not None:
            query.append(("limit", str(limit)))
        path = path_with_query(
            f"/v1/components/{component_id.strip()}/runtime/events",
            query,
        )
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(ComponentRuntimeEventsTailResponse, value)

    async def runtime_append_events(
        self,
        component_id: str,
        request: ComponentRuntimeEventsRequest,
    ) -> ComponentRuntimeEventsResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            f"/v1/components/{component_id.strip()}/runtime/events",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(ComponentRuntimeEventsResponse, value)

    async def runtime_complete_probe(
        self,
        component_id: str,
        probe_id: str,
        request: ComponentRuntimeProbeResult,
    ) -> dict:
        encoded_probe = quote(probe_id.strip(), safe="")
        value = await self._client.transport.post_json(
            self._client.base_url,
            f"/v1/components/{component_id.strip()}/runtime/probe/{encoded_probe}/result",
            request.model_dump(mode="json", exclude_none=True),
        )
        return value
