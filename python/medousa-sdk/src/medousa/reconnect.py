"""Client-side reconnect discipline for interactive SSE streams."""

from __future__ import annotations

import asyncio
import random
from collections.abc import AsyncIterator, Callable
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Generic, TypeVar, cast
from urllib.parse import urlencode, urlparse

from pydantic import BaseModel

from medousa.types import InteractiveTurnStreamEvent, TurnStreamEnvelopeV2

if TYPE_CHECKING:
    from medousa.client import MedousaClient


TEvent = TypeVar("TEvent", bound=BaseModel)
TURN_STREAM_V2_MEDIA_TYPE = "text/event-stream; medousa-version=2"
TERMINAL_V2_EVENT_TYPES = frozenset(
    {"final", "needs_input", "checkpoint", "worker_synthesis", "error"}
)


@dataclass
class BackoffPolicy:
    base_ms: float = 500.0
    factor: float = 2.0
    max_ms: float = 30_000.0
    max_attempts: int | None = 10

    def delay(self, attempt: int) -> float:
        raw = min(self.base_ms * (self.factor**attempt), self.max_ms)
        return raw * random.uniform(0.0, 1.0)

    def may_retry(self, attempt: int) -> bool:
        if self.max_attempts is None:
            return True
        return attempt < self.max_attempts


@dataclass
class CircuitBreakerConfig:
    failure_threshold: int = 5


@dataclass
class CircuitBreaker:
    config: CircuitBreakerConfig = field(default_factory=CircuitBreakerConfig)
    consecutive_failures: int = 0
    open: bool = False

    def allow(self) -> bool:
        return not self.open

    def on_success(self) -> None:
        self.consecutive_failures = 0
        self.open = False

    def on_failure(self) -> None:
        self.consecutive_failures += 1
        if self.consecutive_failures >= self.config.failure_threshold:
            self.open = True


@dataclass
class ReconnectPolicy:
    backoff: BackoffPolicy = field(default_factory=BackoffPolicy)
    breaker: CircuitBreakerConfig = field(default_factory=CircuitBreakerConfig)


class OverlapGuard:
    def __init__(self) -> None:
        self._active = False

    def try_enter(self) -> bool:
        if self._active:
            return False
        self._active = True
        return True

    def release(self) -> None:
        self._active = False


def stream_path_with_since(path: str, since: int) -> str:
    """Append or replace `?since=` for spine replay."""
    base_path = path.split("?", 1)[0]
    if not base_path.startswith("/"):
        parsed = urlparse(path if "://" in path else f"http://local{path}")
        base_path = parsed.path or base_path
    if not base_path.startswith("/"):
        base_path = f"/{base_path}"
    if since <= 0:
        return base_path
    return f"{base_path}?{urlencode([('since', str(since))])}"


def apply_stream_seq(last_seq: int, event: InteractiveTurnStreamEvent) -> tuple[int, bool]:
    return apply_sequence(last_seq, int(event.seq or 0))


def apply_stream_seq_v2(last_seq: int, event: TurnStreamEnvelopeV2) -> tuple[int, bool]:
    return apply_sequence(last_seq, event.seq)


def apply_sequence(last_seq: int, seq: int) -> tuple[int, bool]:
    if seq and seq <= last_seq:
        return last_seq, False
    if seq:
        last_seq = max(last_seq, seq)
    return last_seq, True


class ReconnectingInteractiveStream(Generic[TEvent]):
    """Async iterator over an interactive turn with reconnect + `?since=` replay."""

    def __init__(
        self,
        client: MedousaClient,
        stream_path: str,
        *,
        policy: ReconnectPolicy | None = None,
        event_model: type[TEvent] | None = None,
        accept: str = "text/event-stream",
        terminal: Callable[[TEvent], bool] | None = None,
    ) -> None:
        self._client = client
        self._base_path = stream_path
        self._policy = policy or ReconnectPolicy()
        self._event_model = event_model or cast(type[TEvent], InteractiveTurnStreamEvent)
        self._accept = accept
        self._terminal = terminal or (lambda event: bool(event.terminal))
        self._breaker = CircuitBreaker(self._policy.breaker)
        self._overlap = OverlapGuard()
        self._last_seq = 0
        self._attempt = 0

    @property
    def last_seq(self) -> int:
        return self._last_seq

    def __aiter__(self) -> AsyncIterator[TEvent]:
        return self._iter_events()

    async def _iter_events(self) -> AsyncIterator[TEvent]:
        from medousa.interactive import InteractiveStream

        terminal_seen = False
        while not terminal_seen:
            path = stream_path_with_since(self._base_path, self._last_seq)
            try:
                stream = InteractiveStream(
                    self._client,
                    path,
                    event_model=self._event_model,
                    accept=self._accept,
                )
                async with stream:
                    async for event in stream:
                        self._last_seq, keep = apply_sequence(self._last_seq, int(event.seq))
                        if not keep:
                            continue
                        self._breaker.on_success()
                        self._attempt = 0
                        yield event
                        if self._terminal(event):
                            terminal_seen = True
                            return
            except Exception:
                await self._backoff_or_raise()
            else:
                await self._backoff_or_raise()

    async def _backoff_or_raise(self) -> None:
        self._breaker.on_failure()
        if not self._policy.backoff.may_retry(self._attempt):
            raise RuntimeError("interactive stream reconnect attempts exhausted") from None
        if not self._breaker.allow():
            raise RuntimeError("interactive stream reconnect circuit open") from None
        if not self._overlap.try_enter():
            raise RuntimeError("interactive stream reconnect already running") from None
        try:
            delay = self._policy.backoff.delay(self._attempt)
            self._attempt += 1
            await asyncio.sleep(delay)
        finally:
            self._overlap.release()


def is_turn_stream_terminal(event: TurnStreamEnvelopeV2) -> bool:
    event_type = event.event.root.type
    value = event_type.value if hasattr(event_type, "value") else str(event_type)
    return value in TERMINAL_V2_EVENT_TYPES


class ReconnectingInteractiveStreamV2(ReconnectingInteractiveStream[TurnStreamEnvelopeV2]):
    """Typed v2 reconnect stream using discriminated terminal events."""

    def __init__(
        self,
        client: MedousaClient,
        stream_path: str,
        *,
        policy: ReconnectPolicy | None = None,
    ) -> None:
        super().__init__(
            client,
            stream_path,
            policy=policy,
            event_model=TurnStreamEnvelopeV2,
            accept=TURN_STREAM_V2_MEDIA_TYPE,
            terminal=is_turn_stream_terminal,
        )
