from __future__ import annotations

from medousa._decode import decode
from medousa._ops import op_path, op_path_query
from medousa.client import MedousaClient
from medousa.types import (
    CreativeJob,
    CreativeJobRequest,
    CreativeJobListResponse,
    CreativeAsset,
    CreativeAssetData,
    CreativeAssetImportRequest,
    CreativeContextRequest,
    CreativeCreateRequest,
    CreativeDeleteRequest,
    CreativeDeleteResponse,
    CreativeDocument,
    CreativeHistoryRequest,
    CreativeListResponse,
    CreativeMutationResponse,
    CreativePromoteMediaRequest,
    CreativeProposal,
    CreativeProposalDecision,
    CreativeProposalListResponse,
    CreativeProposalRequest,
    CreativeSelectionContext,
    CreativeTransaction,
)


class CreativeApi:
    """Workshop-owned creative documents; mutations require revision checks."""

    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def list(self, *, cursor: str | None = None, limit: int = 50) -> CreativeListResponse:
        query = [("limit", str(limit))]
        if cursor is not None:
            query.append(("cursor", cursor))
        value = await self._client.transport.get_json(
            self._client.base_url, op_path_query("create.projects.get", query)
        )
        return decode(CreativeListResponse, value)

    async def create(self, request: CreativeCreateRequest) -> CreativeDocument:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("create.projects.post"),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeDocument, value)

    async def get(self, id: str) -> CreativeMutationResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("create.projects.by_id.get", id=id),
        )
        return decode(CreativeMutationResponse, value)

    async def context(self, id: str, request: CreativeContextRequest) -> CreativeSelectionContext:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.context.post", id=id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeSelectionContext, value)

    async def transact(self, id: str, request: CreativeTransaction) -> CreativeMutationResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.transactions.post", id=id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeMutationResponse, value)

    async def history(self, id: str, request: CreativeHistoryRequest) -> CreativeMutationResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.history.post", id=id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeMutationResponse, value)

    async def delete(self, id: str, request: CreativeDeleteRequest) -> CreativeDeleteResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.delete.post", id=id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeDeleteResponse, value)

    async def import_asset(self, id: str, request: CreativeAssetImportRequest) -> CreativeAsset:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.assets.post", id=id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeAsset, value)

    async def asset(self, id: str, asset_id: str) -> CreativeAssetData:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("create.projects.by_id.assets.by_asset_id.get", id=id, asset_id=asset_id),
        )
        return decode(CreativeAssetData, value)

    async def promote_media(self, id: str, request: CreativePromoteMediaRequest) -> CreativeAsset:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.promote_media.post", id=id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeAsset, value)

    async def proposals(self, id: str) -> CreativeProposalListResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("create.projects.by_id.proposals.get", id=id),
        )
        return decode(CreativeProposalListResponse, value)

    async def propose(self, id: str, request: CreativeProposalRequest) -> CreativeProposal:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.proposals.post", id=id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeProposal, value)

    async def decide(self, id: str, proposal_id: str, request: CreativeProposalDecision) -> CreativeMutationResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.proposals.by_proposal_id.decision.post", id=id, proposal_id=proposal_id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeMutationResponse, value)


    async def create_job(self, id: str, request: CreativeJobRequest) -> CreativeJob:
        value = await self._client.transport.post_json(
            self._client.base_url, op_path("create.projects.by_id.jobs.post", id=id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeJob, value)

    async def jobs(self, id: str) -> CreativeJobListResponse:
        value = await self._client.transport.get_json(
            self._client.base_url, op_path("create.projects.by_id.jobs.get", id=id),
        )
        return decode(CreativeJobListResponse, value)

    async def job(self, id: str, job_id: str) -> CreativeJob:
        value = await self._client.transport.get_json(
            self._client.base_url, op_path("create.projects.by_id.jobs.by_job_id.get", id=id, job_id=job_id),
        )
        return decode(CreativeJob, value)

    async def decide_job(self, id: str, job_id: str, request: CreativeProposalDecision) -> CreativeJob:
        value = await self._client.transport.post_json(
            self._client.base_url, op_path("create.projects.by_id.jobs.by_job_id.decision.post", id=id, job_id=job_id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeJob, value)
