from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa._ops import op_path, op_path_query
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
        path = op_path_query(
            "components.by_component_id.store.get",
            _store_query(profile_id, key),
            component_id=component_id.strip(),
        )
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(ComponentStoreGetResponse, value)

    async def store_set(
        self,
        component_id: str,
        key: str,
        request: ComponentStoreSetRequest,
    ) -> ComponentStoreSetResponse:
        path = op_path_query(
            "components.by_component_id.store.put",
            _store_query(None, key),
            component_id=component_id.strip(),
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
        path = op_path_query(
            "components.by_component_id.store.keys.get",
            _profile_query(profile_id),
            component_id=component_id.strip(),
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
        path = op_path_query(
            "components.by_component_id.store.by_key.get",
            _profile_query(profile_id),
            component_id=component_id.strip(),
            key=key.strip(),
        )
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(ComponentStoreGetResponse, value)

    async def store_put_key(
        self,
        component_id: str,
        key: str,
        request: ComponentStoreSetRequest,
    ) -> ComponentStoreSetResponse:
        value = await self._client.transport.put_json(
            self._client.base_url,
            op_path(
                "components.by_component_id.store.by_key.put",
                component_id=component_id.strip(),
                key=key.strip(),
            ),
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
        path = op_path_query(
            "components.by_component_id.store.by_key.delete",
            _profile_query(profile_id),
            component_id=component_id.strip(),
            key=key.strip(),
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
        path = op_path_query(
            "components.by_component_id.runtime.events.get",
            query,
            component_id=component_id.strip(),
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
            op_path(
                "components.by_component_id.runtime.events.post", component_id=component_id.strip()
            ),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(ComponentRuntimeEventsResponse, value)

    async def runtime_complete_probe(
        self,
        component_id: str,
        probe_id: str,
        request: ComponentRuntimeProbeResult,
    ) -> dict:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path(
                "components.by_component_id.runtime.probe.by_probe_id.result.post",
                component_id=component_id.strip(),
                probe_id=probe_id.strip(),
            ),
            request.model_dump(mode="json", exclude_none=True),
        )
        return value
