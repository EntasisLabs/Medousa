from __future__ import annotations

from medousa._decode import decode
from medousa.client import MedousaClient
from medousa.transport import path_with_query
from medousa.types import (
    CalendarDeleteResponse,
    CalendarExportResponse,
    CalendarImportRequest,
    CalendarImportResponse,
    CalendarListResponse,
    CalendarWriteRequest,
    CalendarWriteResponse,
)


class CalendarApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def list_events(
        self,
        *,
        from_: str | None = None,
        to: str | None = None,
        path: str | None = None,
    ) -> CalendarListResponse:
        query: list[tuple[str, str]] = []
        if from_ is not None:
            query.append(("from", from_))
        if to is not None:
            query.append(("to", to))
        if path is not None:
            query.append(("path", path))
        route = path_with_query("/v1/calendar/events", query)
        value = await self._client.transport.get_json(self._client.base_url, route)
        return decode(CalendarListResponse, value)

    async def create_event(self, request: CalendarWriteRequest) -> CalendarWriteResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/calendar/events",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CalendarWriteResponse, value)

    async def update_event(
        self, uid: str, request: CalendarWriteRequest
    ) -> CalendarWriteResponse:
        value = await self._client.transport.put_json(
            self._client.base_url,
            f"/v1/calendar/events/{uid.strip()}",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CalendarWriteResponse, value)

    async def delete_event(
        self, uid: str, *, path: str | None = None
    ) -> CalendarDeleteResponse:
        query: list[tuple[str, str]] = []
        if path is not None:
            query.append(("path", path))
        route = path_with_query(f"/v1/calendar/events/{uid.strip()}", query)
        value = await self._client.transport.delete_json(self._client.base_url, route)
        return decode(CalendarDeleteResponse, value)

    async def import_ics(self, request: CalendarImportRequest) -> CalendarImportResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/calendar/import",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CalendarImportResponse, value)

    async def export(self, *, path: str | None = None) -> CalendarExportResponse:
        query: list[tuple[str, str]] = []
        if path is not None:
            query.append(("path", path))
        route = path_with_query("/v1/calendar/export", query)
        value = await self._client.transport.get_json(self._client.base_url, route)
        return decode(CalendarExportResponse, value)
