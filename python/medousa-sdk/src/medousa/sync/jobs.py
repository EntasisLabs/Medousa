from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa.types import (
    ArchiveAskJobRequest,
    ArchiveAskJobResponse,
    AskJobCompleteActionsRequest,
    AskJobCompleteActionsResponse,
    EnqueueAskRequest,
    EnqueuePromptRequest,
    EnqueueReportRequest,
    EnqueueResponse,
    JobReportResponse,
    JobResultResponse,
)

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class JobsApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def enqueue_ask(self, request: EnqueueAskRequest) -> EnqueueResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/jobs/ask",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(EnqueueResponse, value)

    def result(self, job_id: str) -> JobResultResponse:
        value = self._client._transport.get_json(
            self._client.base_url,
            f"/v1/jobs/{job_id}/result",
        )
        return decode(JobResultResponse, value)

    def report(self, job_id: str) -> JobReportResponse:
        value = self._client._transport.get_json(
            self._client.base_url,
            f"/v1/jobs/{job_id}/report",
        )
        return decode(JobReportResponse, value)

    def enqueue_report(self, request: EnqueueReportRequest) -> EnqueueResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/jobs/report",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(EnqueueResponse, value)

    def enqueue_prompt(self, request: EnqueuePromptRequest) -> EnqueueResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/jobs/prompt",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(EnqueueResponse, value)

    def complete_actions(
        self,
        job_id: str,
        request: AskJobCompleteActionsRequest,
    ) -> AskJobCompleteActionsResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            f"/v1/jobs/{job_id}/complete-actions",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(AskJobCompleteActionsResponse, value)

    def archive(
        self,
        job_id: str,
        request: ArchiveAskJobRequest | None = None,
    ) -> ArchiveAskJobResponse:
        body = (request or ArchiveAskJobRequest()).model_dump(mode="json", exclude_none=True)
        value = self._client._transport.post_json(
            self._client.base_url,
            f"/v1/jobs/{job_id}/archive",
            body,
        )
        return decode(ArchiveAskJobResponse, value)
