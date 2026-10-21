from __future__ import annotations

from medousa._decode import decode
from medousa._ops import op_path, op_path_query
from medousa.sync.client import MedousaClientSync
from medousa.types import (
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


class CreativeApiSync:
    """Workshop-owned creative documents; mutations require revision checks."""

    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def list(self, *, cursor: str | None = None, limit: int = 50) -> CreativeListResponse:
        query = [("limit", str(limit))]
        if cursor is not None:
            query.append(("cursor", cursor))
        value = self._client._transport.get_json(
            self._client.base_url, op_path_query("create.projects.get", query)
        )
        return decode(CreativeListResponse, value)

    def create(self, request: CreativeCreateRequest) -> CreativeDocument:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path("create.projects.post"),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeDocument, value)

    def get(self, id: str) -> CreativeMutationResponse:
        value = self._client._transport.get_json(
            self._client.base_url,
            op_path("create.projects.by_id.get", id=id),
        )
        return decode(CreativeMutationResponse, value)

    def context(self, id: str, request: CreativeContextRequest) -> CreativeSelectionContext:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.context.post", id=id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeSelectionContext, value)

    def transact(self, id: str, request: CreativeTransaction) -> CreativeMutationResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.transactions.post", id=id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeMutationResponse, value)

    def history(self, id: str, request: CreativeHistoryRequest) -> CreativeMutationResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.history.post", id=id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeMutationResponse, value)

    def delete(self, id: str, request: CreativeDeleteRequest) -> CreativeDeleteResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.delete.post", id=id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeDeleteResponse, value)

    def import_asset(self, id: str, request: CreativeAssetImportRequest) -> CreativeAsset:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.assets.post", id=id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeAsset, value)

    def asset(self, id: str, asset_id: str) -> CreativeAssetData:
        value = self._client._transport.get_json(
            self._client.base_url,
            op_path("create.projects.by_id.assets.by_asset_id.get", id=id, asset_id=asset_id),
        )
        return decode(CreativeAssetData, value)

    def promote_media(self, id: str, request: CreativePromoteMediaRequest) -> CreativeAsset:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.promote_media.post", id=id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeAsset, value)

    def proposals(self, id: str) -> CreativeProposalListResponse:
        value = self._client._transport.get_json(
            self._client.base_url,
            op_path("create.projects.by_id.proposals.get", id=id),
        )
        return decode(CreativeProposalListResponse, value)

    def propose(self, id: str, request: CreativeProposalRequest) -> CreativeProposal:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.proposals.post", id=id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeProposal, value)

    def decide(self, id: str, proposal_id: str, request: CreativeProposalDecision) -> CreativeMutationResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path("create.projects.by_id.proposals.by_proposal_id.decision.post", id=id, proposal_id=proposal_id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreativeMutationResponse, value)

