from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa.types import IngestRequest, IngestResponse

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class IngestApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def post(self, request: IngestRequest) -> IngestResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/ingest",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(IngestResponse, value)
