from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa._ops import op_path, op_path_query
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
        path = op_path_query("feeds.get", _profile_query(profile_id))
        value = self._client._transport.get_json(self._client.base_url, path)
        return decode(FeedListResponse, value)

    def tail(self, feed_id: str, query: FeedTailQuery) -> FeedTailResponse:
        path = op_path_query(
            "feeds.by_feed_id.tail.get", _tail_query(query), feed_id=feed_id.strip()
        )
        value = self._client._transport.get_json(self._client.base_url, path)
        return decode(FeedTailResponse, value)

    def mark_read(self, feed_id: str, request: FeedReadRequest) -> None:
        self._client._transport.post_json(
            self._client.base_url,
            op_path("feeds.by_feed_id.read.post", feed_id=feed_id.strip()),
            request.model_dump(mode="json", exclude_none=True),
        )
