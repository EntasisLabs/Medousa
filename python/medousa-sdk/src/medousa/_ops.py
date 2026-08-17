"""Expand generated operations into request paths."""

from __future__ import annotations

from medousa._generated.ops import by_id
from medousa._paths import expand_path
from medousa.transport import path_with_query

# Bounded public-name aliases. Slice 3 did not rename accessors.
ALIASES: dict[str, str] = {}


def op_path(operation_id: str, **params: str) -> str:
    operation_id = ALIASES.get(operation_id, operation_id)
    return expand_path(by_id(operation_id).path, params)


def op_path_query(
    operation_id: str,
    query: list[tuple[str, str]] | None = None,
    **params: str,
) -> str:
    return path_with_query(op_path(operation_id, **params), query or [])
