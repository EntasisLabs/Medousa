#!/usr/bin/env python3
"""Summarize tool-loop inference records from one session's turn ledger."""

import argparse
import collections
import json
from pathlib import Path


def summarize(records):
    turns = {}
    for record in records:
        usage = record.get("inference")
        if not usage:
            continue
        if type(usage.get("schema_version")) is not int or usage["schema_version"] not in (1, 2):
            raise ValueError("unsupported inference usage schema")
        turn_id = str(record["stream_turn_id"])
        turn = turns.setdefault(turn_id, {
            "requests": 0, "outcomes": collections.Counter(),
            "models": collections.Counter(), "tools": collections.Counter(),
            "tokens": {key: {"reported_sum": 0, "reported_requests": 0}
                       for key in ("input", "cache_read", "cache_write", "output", "reasoning")},
            "generated": collections.Counter(), "generated_coverage": collections.Counter(),
            "completed_tool_calls": 0, "completed_tool_requests": 0, "tools_changed": 0,
            "history_rewrites": 0, "elapsed_ms": 0,
        })
        turn["requests"] += 1
        turn["outcomes"][usage["outcome"]] += 1
        turn["models"][usage.get("provider_model") or usage["request"].get("model") or "unknown"] += 1
        turn["tools"].update(record.get("tools_invoked", []))
        turn["elapsed_ms"] += usage["elapsed_ms"]
        for key, counter in turn["tokens"].items():
            value = usage.get("tokens", {}).get(key)
            if value is not None:
                if not isinstance(value, int) or isinstance(value, bool) or value < 0:
                    raise ValueError(f"invalid {key} token count")
                counter["reported_sum"] += value
                counter["reported_requests"] += 1
        for key, value in (usage.get("generated", {}) if usage["outcome"] == "completed" else {}).items():
            if value is None:
                continue
            if not isinstance(value, int) or isinstance(value, bool) or value < 0:
                raise ValueError(f"invalid generated {key} count")
            turn["generated"][key] += value
            turn["generated_coverage"][key] += 1
        calls = usage.get("generated", {}).get("tool_calls")
        if usage["outcome"] == "completed" and calls is not None:
            turn["completed_tool_calls"] += calls
            turn["completed_tool_requests"] += 1
        change = usage.get("change")
        if change:
            turn["tools_changed"] += bool(change["tools_changed"])
            turn["history_rewrites"] += change["matching_messages"] < change["previous_message_count"]
    for turn in turns.values():
        for counter in turn["tokens"].values():
            counter["missing_requests"] = turn["requests"] - counter["reported_requests"]
            counter["total"] = counter["reported_sum"] if counter["missing_requests"] == 0 else None
        completed = turn["outcomes"].get("completed", 0)
        calls = turn.pop("completed_tool_calls")
        coverage = turn.pop("completed_tool_requests")
        turn["tool_calls_per_completed_request"] = calls / completed if completed and coverage == completed else None
        inputs = turn["tokens"]["input"]["total"]
        reads = turn["tokens"]["cache_read"]["total"]
        turn["cache_read_fraction"] = reads / inputs if inputs and reads is not None and reads <= inputs else None
    return turns


def read_records(path):
    with path.open(encoding="utf-8") as source:
        for number, line in enumerate(source, 1):
            if line.strip():
                try:
                    record = json.loads(line)
                except json.JSONDecodeError:
                    # Do not silently undercount a truncated active ledger.
                    raise ValueError(f"invalid JSON on line {number}; use a completed ledger snapshot") from None
                if not isinstance(record, dict):
                    raise ValueError(f"expected an object on line {number}")
                yield record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("ledger", type=Path, help="one session turn_ledger JSONL file")
    args = parser.parse_args()
    try:
        result = summarize(read_records(args.ledger))
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(1, f"usage report failed: {error}\n")
    print(json.dumps({
        "scope": "tool_loop_logical_requests",
        "notes": ["Unknown usage remains null; reported sums may be partial.",
                  "Output includes reasoning; do not add reasoning to output.",
                  "History rewrites are not proof of provider cache misses.",
                  "Generated sums are partial when coverage is below requests; batch operations are requested, not executed.",
                  "Adapter-internal retries and non-tool-loop inference are outside this ledger."],
        "turns": result,
    }, indent=2))


if __name__ == "__main__":
    main()
