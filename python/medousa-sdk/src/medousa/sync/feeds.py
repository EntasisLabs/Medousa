from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa.transport import path_with_query
from medousa.types import FeedListResponse, FeedReadRequest, FeedTailQuery, FeedTailResponse

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


def _profile_query(profile_id: str | None) -> list[tuple[str, str]]:
    return [("profile_id", profile_id)] if profile_id is not None else []


def _tail_query(query: FeedTailQuery) -> list[tuple[str, str]]:
    params = _profile_query(query.profile_id)
    if query.limit is not None:
        params.append(("limit", str(query.limit)))
    return params


class FeedsApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def list(self, profile_id: str | None = None) -> FeedListResponse:
        path = path_with_query("/v1/feeds", _profile_query(profile_id))
        value = self._client._transport.get_json(self._client.base_url, path)
        return decode(FeedListResponse, value)

    def tail(self, feed_id: str, query: FeedTailQuery) -> FeedTailResponse:
        path = path_with_query(f"/v1/feeds/{feed_id.strip()}/tail", _tail_query(query))
        value = self._client._transport.get_json(self._client.base_url, path)
        return decode(FeedTailResponse, value)

    def mark_read(self, feed_id: str, request: FeedReadRequest) -> None:
        self._client._transport.post_json(
            self._client.base_url,
            f"/v1/feeds/{feed_id.strip()}/read",
            request.model_dump(mode="json", exclude_none=True),
        )
