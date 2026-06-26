from __future__ import annotations

from datetime import datetime
from typing import Any

from pydantic import BaseModel, ConfigDict, Field


class MedousaModel(BaseModel):
    model_config = ConfigDict(extra="ignore", populate_by_name=True)


# ── Health & jobs ─────────────────────────────────────────────────────────────

class HealthResponse(MedousaModel):
    status: str
    backend: str
    worker_id: str
    now_utc: datetime
    agent_runtime_version: str = "centralized-v1"
    tool_registry_count: int = 0
    last_agent_turn_latency_ms: int | None = None
    last_agent_turn_at_utc: datetime | None = None
    active_profile_id: str = ""
    active_profile_display_name: str = ""


class EnqueueAskRequest(MedousaModel):
    prompt: str
    policy_profile: str | None = None
    model_hint: str | None = None
    max_turns: int | None = None
    identity_user_id: str | None = None
    identity_persona_id: str | None = None
    identity_channel_id: str | None = None
    manuscript_id: str | None = None
    additional_manuscript_ids: list[str] | None = None
    suggested_capability_ids: list[str] | None = None


class EnqueueResponse(MedousaModel):
    job_id: str
    queue: str
    accepted_at_utc: datetime


class RegisterRecurringPromptRequest(MedousaModel):
    id: str | None = None
    queue: str | None = None
    prompt: str
    system_prompt: str | None = None
    cron_expr: str
    timezone: str | None = None
    jitter_seconds: int | None = None
    enabled: bool | None = None
    max_attempts: int | None = None
    policy_profile: str | None = None
    model_hint: str | None = None
    delivery: dict[str, Any] | None = None
    session_id: str | None = None
    execution_mode: str | None = None
    manuscript_id: str | None = None
    display_name: str | None = None


class RegisterRecurringResponse(MedousaModel):
    recurring_id: str
    queue: str
    next_run_at_utc: datetime
    cron_expr: str
    timezone: str


# ── Ingest ────────────────────────────────────────────────────────────────────

class IngestAttachment(MedousaModel):
    kind: str = ""
    content: str = ""


class IngestRequest(MedousaModel):
    channel: str
    user_id: str
    channel_id: str
    text: str
    attachments: list[IngestAttachment] = Field(default_factory=list)


class IngestResponse(MedousaModel):
    session_id: str
    job_id: str | None = None
    reply: str
    is_new_session: bool
    stream_id: str | None = None
    stream_url: str | None = None
    stream_ready: bool = False


# ── Interactive ─────────────────────────────────────────────────────────────

class MediaRef(MedousaModel):
    media_id: str
    kind: str
    mime: str
    label: str | None = None


class StageRoute(MedousaModel):
    role: str
    provider: str
    model: str
    policy_profile: str
    fallback_chain: list[str] = Field(default_factory=list)


class StageRoutingMatrix(MedousaModel):
    orchestrator: StageRoute
    chunker: StageRoute
    extractor: StageRoute
    summarizer: StageRoute
    verifier: StageRoute
    packer: StageRoute
    final_response: StageRoute


class TurnSurfaceContext(MedousaModel):
    channel: str = ""
    supports_ui_artifacts: bool = False


class InteractiveTurnRequest(MedousaModel):
    session_id: str
    prompt: str
    persist_user_turn: bool = True
    response_depth_mode: str = "balanced"
    reasoning_effort: str = ""
    provider: str = ""
    model: str = ""
    stage_routing: StageRoutingMatrix | dict[str, Any] = Field(default_factory=dict)
    surface: TurnSurfaceContext | None = None
    max_tool_rounds: int | None = None
    retry_runtime_max_rounds: int | None = None
    manuscript_id: str | None = None
    additional_manuscript_ids: list[str] | None = None
    suggested_capability_ids: list[str] | None = None
    voice_preset_id: str | None = None
    voice_appendix: str | None = None
    scheduled_tool_allowlist: list[str] | None = None
    media_refs: list[MediaRef] = Field(default_factory=list)
    identity_user_id: str | None = None


class InteractiveTurnResponse(MedousaModel):
    turn_id: str
    accepted_at_utc: datetime
    stream_url: str
    stream_ready: bool
    fallback_to_local: bool = False
    fallback_reason: str | None = None
    daemon_notice: str | None = None


class StreamUiArtifact(MedousaModel):
    artifact_id: str
    mime: str
    label: str
    presentation: str
    byte_size: int | None = None
    height_px: int | None = None


class InteractiveTurnStreamEvent(MedousaModel):
    turn_id: str
    event_type: str
    phase: str
    message: str
    content_delta: str | None = None
    reasoning_delta: str | None = None
    final_text: str | None = None
    tool_names: list[str] | None = None
    terminal: bool
    emitted_at_utc: datetime
    budget_request_id: str | None = None
    requested_rounds: int | None = None
    work_id: str | None = None
    tool_run_id: str | None = None
    tool_name: str | None = None
    tool_status: str | None = None
    tool_input_summary: str | None = None
    tool_output_summary: str | None = None
    tool_round: int | None = None
    ui_artifact: StreamUiArtifact | None = None
    previous_artifact_id: str | None = None
    root_artifact_id: str | None = None
    operator_message: str | None = None
    debug_message: str | None = None


# ── Artifacts ─────────────────────────────────────────────────────────────────

class ArtifactFetchRequest(MedousaModel):
    session_id: str
    artifact_id: str


class ArtifactFetchResponse(MedousaModel):
    artifact_id: str
    mime: str
    label: str
    body: str
    byte_size: int
    presentation: str | None = None
    height_px: int | None = None


class ArtifactListUiRequest(MedousaModel):
    session_id: str | None = None
    limit: int = 50
    query: str | None = None


class ArtifactSummary(MedousaModel):
    artifact_id: str
    session_id: str
    label: str
    presentation: str | None = None
    byte_size: int
    stored_at_utc: datetime
    root_artifact_id: str | None = None
    supersedes_artifact_id: str | None = None


class ArtifactListUiResponse(MedousaModel):
    artifacts: list[ArtifactSummary]


class ArtifactCommandRequest(MedousaModel):
    session_id: str
    selected_context_pack_query: str | None = None
    command: dict[str, Any]
    verification_policy: dict[str, Any] | None = None
    verifier_route_label: str | None = None


class ArtifactCommandResponse(MedousaModel):
    selected_context_pack_query: str | None = None
    rendered_output: str


class RuntimeConfigCommandRequest(MedousaModel):
    current_provider: str
    current_model: str
    response_depth_mode: str
    reasoning_effort: str
    command: dict[str, Any]


class RuntimeConfigCommandResponse(MedousaModel):
    rendered_output: str
    response_depth_mode: str | None = None
    reasoning_effort: str | None = None


class StageRouteCommandRequest(MedousaModel):
    stage_routing: StageRoutingMatrix | dict[str, Any]
    provider: str
    model: str
    command: dict[str, Any]


class StageRouteCommandResponse(MedousaModel):
    stage_routing: StageRoutingMatrix | dict[str, Any]
    rendered_output: str


# ── Budget ────────────────────────────────────────────────────────────────────

class TurnBudgetApproveRequest(MedousaModel):
    extra_rounds: int | None = None
    resolved_by: str | None = None


class TurnBudgetDenyRequest(MedousaModel):
    resolved_by: str | None = None


class TurnBudgetRequestRecord(MedousaModel):
    request_id: str
    turn_correlation_id: str | None = None
    stream_turn_id: int
    session_id: str
    channel: str | None = None
    rounds_executed: int
    max_tool_rounds: int
    requested_rounds: int
    granted_rounds: int | None = None
    reason: str
    progress_summary: str | None = None
    status: str
    resolved_by: str | None = None
    created_at_utc: datetime
    updated_at_utc: datetime
    resolved_at_utc: datetime | None = None


class TurnBudgetRequestListResponse(MedousaModel):
    requests: list[TurnBudgetRequestRecord]


class TurnBudgetRequestResponse(MedousaModel):
    request: TurnBudgetRequestRecord
    message: str
