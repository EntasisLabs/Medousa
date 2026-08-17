from __future__ import annotations

from medousa._decode import decode
from medousa._ops import op_path
from medousa.client import MedousaClient
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


class JobsApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def enqueue_ask(self, request: EnqueueAskRequest) -> EnqueueResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("jobs.ask.post"),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(EnqueueResponse, value)

    async def result(self, job_id: str) -> JobResultResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("jobs.by_job_id.result.get", job_id=job_id),
        )
        return decode(JobResultResponse, value)

    async def report(self, job_id: str) -> JobReportResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("jobs.by_job_id.report.get", job_id=job_id),
        )
        return decode(JobReportResponse, value)

    async def enqueue_report(self, request: EnqueueReportRequest) -> EnqueueResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("jobs.report.post"),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(EnqueueResponse, value)

    async def enqueue_prompt(self, request: EnqueuePromptRequest) -> EnqueueResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("jobs.prompt.post"),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(EnqueueResponse, value)

    async def complete_actions(
        self,
        job_id: str,
        request: AskJobCompleteActionsRequest,
    ) -> AskJobCompleteActionsResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("jobs.by_job_id.complete_actions.post", job_id=job_id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(AskJobCompleteActionsResponse, value)

    async def archive(
        self,
        job_id: str,
        request: ArchiveAskJobRequest | None = None,
    ) -> ArchiveAskJobResponse:
        body = (request or ArchiveAskJobRequest()).model_dump(mode="json", exclude_none=True)
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("jobs.by_job_id.archive.post", job_id=job_id),
            body,
        )
        return decode(ArchiveAskJobResponse, value)
