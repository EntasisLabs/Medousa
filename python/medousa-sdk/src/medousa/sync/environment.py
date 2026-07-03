from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa.transport import path_with_query
from medousa.types import (
    EnvironmentPendingResponse,
    EnvironmentProposeResponse,
    EnvironmentSpecPutRequest,
    EnvironmentSpecResponse,
    EnvironmentStatusResponse,
    EnvironmentValidateRequest,
    EnvironmentValidateResponse,
)

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


def _profile_query(profile_id: str | None) -> list[tuple[str, str]]:
    return [("profile_id", profile_id)] if profile_id is not None else []


class EnvironmentApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def get_spec(self, profile_id: str | None = None) -> EnvironmentSpecResponse:
        path = path_with_query("/v1/environment/spec", _profile_query(profile_id))
        value = self._client._transport.get_json(self._client.base_url, path)
        return decode(EnvironmentSpecResponse, value)

    def put_spec(self, request: EnvironmentSpecPutRequest) -> EnvironmentSpecResponse:
        value = self._client._transport.put_json(
            self._client.base_url,
            "/v1/environment/spec",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(EnvironmentSpecResponse, value)

    def get_status(
        self,
        *,
        profile_id: str | None = None,
        surface_id: str | None = None,
        include_runtime: bool | None = None,
    ) -> EnvironmentStatusResponse:
        query = _profile_query(profile_id)
        if surface_id is not None:
            query.append(("surface_id", surface_id))
        if include_runtime is not None:
            query.append(("include_runtime", str(include_runtime).lower()))
        path = path_with_query("/v1/environment/status", query)
        value = self._client._transport.get_json(self._client.base_url, path)
        return decode(EnvironmentStatusResponse, value)

    def validate_spec(self, request: EnvironmentValidateRequest) -> EnvironmentValidateResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/environment/spec/validate",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(EnvironmentValidateResponse, value)

    def propose_spec(self, request: EnvironmentSpecPutRequest) -> EnvironmentProposeResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/environment/spec/propose",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(EnvironmentProposeResponse, value)

    def get_pending(self, profile_id: str | None = None) -> EnvironmentPendingResponse:
        path = path_with_query("/v1/environment/spec/pending", _profile_query(profile_id))
        value = self._client._transport.get_json(self._client.base_url, path)
        return decode(EnvironmentPendingResponse, value)

    def dismiss_pending(self, profile_id: str | None = None) -> None:
        path = path_with_query("/v1/environment/spec/pending", _profile_query(profile_id))
        self._client._transport.delete_json(self._client.base_url, path)

    def apply_pending(self, profile_id: str | None = None) -> EnvironmentSpecResponse:
        path = path_with_query("/v1/environment/spec/pending/apply", _profile_query(profile_id))
        value = self._client._transport.post_empty_json(self._client.base_url, path)
        return decode(EnvironmentSpecResponse, value)
