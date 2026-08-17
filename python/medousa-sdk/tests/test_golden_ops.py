from __future__ import annotations

import json
from pathlib import Path

from medousa._generated.ops import by_id
from medousa._ops import ALIASES, op_path

GOLDEN = Path(__file__).resolve().parents[3] / "sdk-contract" / "golden" / "client-cases.json"


def test_generated_ops_match_shared_golden_cases() -> None:
    payload = json.loads(GOLDEN.read_text())
    for case in payload["cases"]:
        op = by_id(case["id"])
        assert op.method == case["method"]
        assert op.path == case["path"]
        assert op.streaming is case["streaming"]
        assert op.method != "SSE"
        expanded = op_path(case["id"], **case.get("params", {}))
        if "expanded" in case:
            assert expanded == case["expanded"]


def test_golden_mutation_rejects_wrong_verb_or_path() -> None:
    health = by_id("health.get")
    assert health.method != "POST"
    assert health.path != "/v1/healthz"
    stream = by_id("workspace.stream.get")
    assert stream.streaming is True
    assert health.streaming is False


def test_alias_map_is_bounded() -> None:
    assert ALIASES == {}
