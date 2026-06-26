from __future__ import annotations

from datetime import datetime

from medousa.types.daemon_api import MedousaModel


class ConversationTurn(MedousaModel):
    role: str
    content: str
    timestamp: datetime
    tool_names: list[str] = []
    answer_state: str | None = None


class SessionHistorySummary(MedousaModel):
    session_id: str
    display_name: str | None = None
    turns: int
    verification_runs: int = 0
    last_timestamp: datetime | None = None
    preview: str = ""


class SessionHistoryListResponse(MedousaModel):
    sessions: list[SessionHistorySummary]
    next_cursor: str | None = None


class SessionHistoryResponse(MedousaModel):
    session_id: str
    turns: list[ConversationTurn]


class SessionAppendTurnRequest(MedousaModel):
    turn: ConversationTurn


class SessionAppendTurnResponse(MedousaModel):
    session_id: str
    stored: bool


class SessionSetDisplayNameRequest(MedousaModel):
    display_name: str


class SessionSetDisplayNameResponse(MedousaModel):
    session_id: str
    display_name: str
