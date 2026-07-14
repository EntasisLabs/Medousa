from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
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

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class CalendarApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def list_events(
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
        value = self._client._transport.get_json(self._client.base_url, route)
        return decode(CalendarListResponse, value)

    def create_event(self, request: CalendarWriteRequest) -> CalendarWriteResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/calendar/events",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CalendarWriteResponse, value)

    def update_event(self, uid: str, request: CalendarWriteRequest) -> CalendarWriteResponse:
        value = self._client._transport.put_json(
            self._client.base_url,
            f"/v1/calendar/events/{uid.strip()}",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CalendarWriteResponse, value)

    def delete_event(self, uid: str, *, path: str | None = None) -> CalendarDeleteResponse:
        query: list[tuple[str, str]] = []
        if path is not None:
            query.append(("path", path))
        route = path_with_query(f"/v1/calendar/events/{uid.strip()}", query)
        value = self._client._transport.delete_json(self._client.base_url, route)
        return decode(CalendarDeleteResponse, value)

    def import_ics(self, request: CalendarImportRequest) -> CalendarImportResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/calendar/import",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CalendarImportResponse, value)

    def export(self, *, path: str | None = None) -> CalendarExportResponse:
        query: list[tuple[str, str]] = []
        if path is not None:
            query.append(("path", path))
        route = path_with_query("/v1/calendar/export", query)
        value = self._client._transport.get_json(self._client.base_url, route)
        return decode(CalendarExportResponse, value)
