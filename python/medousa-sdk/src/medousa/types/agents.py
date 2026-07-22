"""Hand-maintained agent request/response models (fields for model_dump).

Fallback codegen only emits empty shells; these override via types/__init__ if needed.
Prefer regenerating from schema when datamodel-code-generator is available.
"""

from __future__ import annotations

from datetime import datetime
from typing import Any

from pydantic import BaseModel, ConfigDict, Field


class MedousaModel(BaseModel):
    model_config = ConfigDict(extra="allow", populate_by_name=True)


class AgentRuntimeInfo(MedousaModel):
    runtime: str
    available: bool = True
    command: str | None = None
    detail: str | None = None
    uses_native_turns: bool = False


class AgentRuntimeListResponse(MedousaModel):
    runtimes: list[AgentRuntimeInfo] = Field(default_factory=list)


class CreateAgentSessionRequest(MedousaModel):
    session_id: str
    runtime: str
    prompt: str | None = None
    cwd: str | None = None
    command: str | None = None
    args: list[str] | None = None
    surface: dict[str, Any] | None = None


class CreateAgentSessionResponse(MedousaModel):
    agent_session_id: str
    session_id: str
    runtime: str
    phase: str
    stream_url: str
    stream_ready: bool = True
    accepted_at_utc: datetime | None = None


class AgentSessionPromptRequest(MedousaModel):
    prompt: str


class AgentSessionPromptResponse(MedousaModel):
    accepted: bool
    agent_session_id: str


class CancelAgentSessionResponse(MedousaModel):
    cancelled: bool
    agent_session_id: str
    message: str = ""


class AgentPermissionRequestRecord(MedousaModel):
    request_id: str
    agent_session_id: str
    session_id: str
    runtime: str
    summary: str
    status: str
    created_at_utc: datetime | None = None
    updated_at_utc: datetime | None = None
    resolved_at_utc: datetime | None = None
    resolved_by: str | None = None


class AgentPermissionRequestListResponse(MedousaModel):
    requests: list[AgentPermissionRequestRecord] = Field(default_factory=list)


class AgentPermissionResolveRequest(MedousaModel):
    resolved_by: str | None = None


class AgentPermissionResolveResponse(MedousaModel):
    request: AgentPermissionRequestRecord
