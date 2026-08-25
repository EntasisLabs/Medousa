from __future__ import annotations

from typing import TYPE_CHECKING, Any

from medousa._decode import decode
from medousa._generated.ops import by_id
from medousa.error import CompatibilityError, SdkError
from medousa.types import HealthResponse

if TYPE_CHECKING:
    from medousa.client import MedousaClient

EXPECTED_DAEMON_CONTRACT_REVISION = 1


def decode_health_response(value: Any, path: str) -> HealthResponse:
    runtime = value.get("runtime") if isinstance(value, dict) else None
    if not isinstance(runtime, dict):
        raise CompatibilityError(
            f"GET {path} responder omitted the required runtime descriptor; "
            f"client expects daemon contract revision {EXPECTED_DAEMON_CONTRACT_REVISION}"
        )
    try:
        response = decode(HealthResponse, value)
    except SdkError as error:
        raise CompatibilityError(
            f"GET {path} responder returned an invalid health contract: {error}"
        ) from error
    descriptor = response.runtime
    if descriptor.contract_revision != EXPECTED_DAEMON_CONTRACT_REVISION:
        raise CompatibilityError(
            f"GET {path} responder authority {descriptor.authority_id.root} build "
            f"{descriptor.build_revision} ({descriptor.deployment_target}) uses daemon contract "
            f"revision {descriptor.contract_revision}; client expects "
            f"{EXPECTED_DAEMON_CONTRACT_REVISION}"
        )
    if descriptor.base_schema_revision == 0:
        raise CompatibilityError(
            f"GET {path} responder authority {descriptor.authority_id.root} build "
            f"{descriptor.build_revision} reported invalid base schema revision 0"
        )
    return response


class HealthApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def get(self) -> HealthResponse:
        path = by_id("health.get").path
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode_health_response(value, path)
