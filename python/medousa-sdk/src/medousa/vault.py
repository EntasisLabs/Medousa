from __future__ import annotations

from medousa._decode import decode
from medousa._ops import op_path, op_path_query
from medousa.client import MedousaClient
from medousa.types import (
    VaultAddRootRequest,
    VaultBacklinksResponse,
    VaultDeleteResponse,
    VaultNoteContentResponse,
    VaultNotesListResponse,
    VaultRootsResponse,
    VaultSearchResponse,
    VaultSetActiveRootRequest,
    VaultTagsListResponse,
    VaultWriteRequest,
    VaultWriteResponse,
)


class VaultApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def list_roots(self) -> VaultRootsResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("vault.roots.get"),
        )
        return decode(VaultRootsResponse, value)

    async def add_root(self, request: VaultAddRootRequest) -> VaultRootsResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("vault.roots.post"),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(VaultRootsResponse, value)

    async def set_active_root(self, request: VaultSetActiveRootRequest) -> VaultRootsResponse:
        value = await self._client.transport.put_json(
            self._client.base_url,
            op_path("vault.active.put"),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(VaultRootsResponse, value)

    async def list_notes(
        self,
        *,
        prefix: str | None = None,
        limit: int | None = None,
        tags: str | None = None,
        tag_prefix: str | None = None,
    ) -> VaultNotesListResponse:
        query: list[tuple[str, str]] = []
        if prefix is not None:
            query.append(("prefix", prefix))
        if limit is not None:
            query.append(("limit", str(limit)))
        if tags is not None:
            query.append(("tags", tags))
        if tag_prefix is not None:
            query.append(("tag_prefix", tag_prefix))
        path = op_path_query("vault.notes.get", query)
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(VaultNotesListResponse, value)

    async def create_note(self, request: VaultWriteRequest) -> VaultWriteResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("vault.notes.post"),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(VaultWriteResponse, value)

    async def get_note(self, note_path: str) -> VaultNoteContentResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("vault.notes.by_note_path.get", note_path=note_path.lstrip("/")),
        )
        return decode(VaultNoteContentResponse, value)

    async def update_note(self, note_path: str, request: VaultWriteRequest) -> VaultWriteResponse:
        value = await self._client.transport.put_json(
            self._client.base_url,
            op_path("vault.notes.by_note_path.put", note_path=note_path.lstrip("/")),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(VaultWriteResponse, value)

    async def delete_note(self, note_path: str) -> VaultDeleteResponse:
        value = await self._client.transport.delete_json(
            self._client.base_url,
            op_path("vault.notes.by_note_path.delete", note_path=note_path.lstrip("/")),
        )
        return decode(VaultDeleteResponse, value)

    async def list_tags(
        self,
        *,
        prefix: str | None = None,
        limit: int | None = None,
    ) -> VaultTagsListResponse:
        query: list[tuple[str, str]] = []
        if prefix is not None:
            query.append(("prefix", prefix))
        if limit is not None:
            query.append(("limit", str(limit)))
        path = op_path_query("vault.tags.get", query)
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(VaultTagsListResponse, value)

    async def search(
        self,
        *,
        q: str | None = None,
        limit: int | None = None,
        tags: str | None = None,
    ) -> VaultSearchResponse:
        query: list[tuple[str, str]] = []
        if q is not None:
            query.append(("q", q))
        if limit is not None:
            query.append(("limit", str(limit)))
        if tags is not None:
            query.append(("tags", tags))
        path = op_path_query("vault.search.get", query)
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(VaultSearchResponse, value)

    async def backlinks(self, *, path: str | None = None) -> VaultBacklinksResponse:
        query: list[tuple[str, str]] = []
        if path is not None:
            query.append(("path", path))
        api_path = op_path_query("vault.backlinks.get", query)
        value = await self._client.transport.get_json(self._client.base_url, api_path)
        return decode(VaultBacklinksResponse, value)
