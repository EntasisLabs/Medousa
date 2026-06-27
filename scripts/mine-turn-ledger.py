#!/usr/bin/env python3
"""Mine turn_ledger JSONL files for runtime friction investigation.

Read-only: walks MEDOUSA_DATA_DIR/turn_ledger (or --data-dir) and emits
architecture/investigations/runtime-friction/daemon-corpus.json.

Usage:
  python3 scripts/mine-turn-ledger.py
  python3 scripts/mine-turn-ledger.py --data-dir ~/Library/Application\\ Support/medousa
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from collections import Counter
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

# Tool outcome pattern in round_digests: "tool:ok" or "tool:fail (reason)"
DIGEST_TOOL_RE = re.compile(
    r"(\w+(?:\.\w+)*):(?:(ok|fail)(?: \(([^)]+)\))?)",
    re.IGNORECASE,
)
WS_FAIL_REASON_RE = re.compile(
    r"cognition_web_search:fail(?: \(([^)]+)\))?", re.IGNORECASE
)


def default_data_dir() -> Path:
    if env := os.environ.get("MEDOUSA_DATA_DIR"):
        return Path(env).expanduser()
    home = Path.home()
    if sys.platform == "darwin":
        return home / "Library" / "Application Support" / "medousa"
    xdg = os.environ.get("XDG_DATA_HOME")
    if xdg:
        return Path(xdg).expanduser() / "medousa"
    return home / ".local" / "share" / "medousa"


def channel_hint(filename: str) -> str:
    name = filename.lower()
    if name.startswith("medousa-home-"):
        return "home"
    if "daemon-ask" in name or name.startswith("medousa-ask"):
        return "ask"
    if name.startswith("medousa-tui") or "tui" in name:
        return "tui"
    return "other"


def normalize_fail_reason(reason: str) -> str:
    r = reason.strip()
    if not r:
        return "unknown"
    if "web.tavily unavailable" in r:
        return "binding_tavily_unavailable"
    if "web.brave unavailable" in r:
        return "binding_brave_unavailable"
    if "web.xaviv unavailable" in r:
        return "binding_xaviv_unavailable"
    if "web.websearch unavailable" in r or "web.websearch.search unavailable" in r:
        return "binding_websearch_unavailable"
    if "no available bindings" in r:
        return "no_available_bindings"
    if "unknown capability" in r:
        return "unknown_capability"
    if "max rounds" in r:
        return "max_rounds_exhausted"
    if "port failure" in r:
        return "port_failure_other"
    return r[:80]


@dataclass
class SessionRecord:
    session_id: str
    channel: str
    goal: str = ""
    first_timestamp: str | None = None
    last_timestamp: str | None = None
    tool_counts: dict[str, int] = field(default_factory=dict)
    event_kinds: dict[str, int] = field(default_factory=dict)
    ws_ok: int = 0
    ws_fail: int = 0
    ws_fail_reasons: dict[str, int] = field(default_factory=dict)
    cap_search: int = 0
    cap_invoke: int = 0
    cap_invoke_fail: int = 0
    spawn_worker: int = 0
    work_delegated: int = 0
    work_failed: int = 0
    work_completed: int = 0
    vault_tools: int = 0
    memory_tools: int = 0
    identity_tools: int = 0
    discover_tools: int = 0
    friction_score: int = 0
    tags: list[str] = field(default_factory=list)
    timeline: list[dict[str, Any]] = field(default_factory=list)
    notes: str = ""


VAULT_TOOLS = {
    "cognition_vault_search",
    "cognition_vault_read",
    "cognition_vault_write",
    "cognition_vault_list",
}
MEMORY_TOOLS = {
    "cognition_memory_store",
    "cognition_memory_recall",
    "cognition_memory_list",
    "cognition_memory_context",
    "cognition_memory_calibrate",
    "cognition_memory_schema",
    "cognition_memory_moods",
}
IDENTITY_TOOLS = {
    "cognition_identity_remember",
    "cognition_identity_recall",
    "cognition_identity_context",
    "cognition_identity_propose",
}
DISCOVER_TOOLS = {
    "cognition_tools_discover",
    "cognition_capability_search",
    "cognition_capability_list",
    "cognition_capability_resolve",
    "cognition_tool_history_summary",
    "cognition_tool_history_detail",
}


def parse_digests(digests: list[str], record: SessionRecord) -> None:
    for digest in digests:
        for m in DIGEST_TOOL_RE.finditer(digest):
            tool, status, reason = m.group(1), m.group(2).lower(), m.group(3) or ""
            if tool == "cognition_web_search":
                if status == "ok":
                    record.ws_ok += 1
                else:
                    record.ws_fail += 1
                    key = normalize_fail_reason(reason)
                    record.ws_fail_reasons[key] = record.ws_fail_reasons.get(key, 0) + 1
            if tool == "cognition_capability_invoke" and status == "fail":
                record.cap_invoke_fail += 1


def compute_friction_score(record: SessionRecord) -> int:
    """0=never succeeded, 1=after retries/spawn, 2=first-call ok, 3=clean."""
    if record.ws_ok == 0 and record.ws_fail > 0:
        return 0
    if record.ws_ok == 0 and record.ws_fail == 0:
        return -1  # no web search
    # Has web search success
    if record.ws_fail == 0 and record.cap_search <= 1:
        return 3
    if record.ws_fail == 0:
        return 2
    if record.ws_ok > 0:
        return 1
    return 0


def proactive_hint(goal: str) -> bool:
    gl = goal.lower()
    hints = (
        "remember",
        "vault",
        "note",
        "preference",
        "identity",
        "who am i",
        "my name",
        "recall",
        "stored",
        "wrote down",
    )
    return any(h in gl for h in hints)


def score_proactive(record: SessionRecord) -> str | None:
    """overtook / balanced / too_passive / None if not applicable."""
    discover_heavy = record.discover_tools >= 8 or record.cap_search >= 6
    proactive_used = (
        record.vault_tools + record.memory_tools + record.identity_tools
    ) > 0
    web_heavy = record.ws_ok + record.ws_fail >= 3

    if discover_heavy and record.ws_fail > 0:
        return "overtook"
    if proactive_hint(record.goal) and not proactive_used:
        return "too_passive"
    if proactive_used and (web_heavy or record.goal):
        return "balanced"
    if discover_heavy and not web_heavy:
        return "overtook"
    if record.goal and not proactive_used and len(record.goal) > 40:
        # research/decision goals without memory/vault
        research_words = ("research", "look into", "find out", "best way", "how to")
        if any(w in record.goal.lower() for w in research_words):
            if record.vault_tools == 0 and record.memory_tools == 0:
                return "too_passive"
    return "balanced" if proactive_used else None


def mine_ledger_file(path: Path) -> SessionRecord | None:
    session_id = path.stem
    record = SessionRecord(session_id=session_id, channel=channel_hint(path.name))
    first_ws_round: int | None = None
    first_ws_ok = False
    web_fail_before_ok = False
    delegated_after_web_fail = False
    saw_web_fail = False

    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue

        ts = obj.get("timestamp")
        if ts:
            if record.first_timestamp is None:
                record.first_timestamp = ts
            record.last_timestamp = ts

        kind = obj.get("kind", "unknown")
        record.event_kinds[kind] = record.event_kinds.get(kind, 0) + 1

        scratch = obj.get("scratch") or {}
        if not record.goal and scratch.get("goal"):
            record.goal = str(scratch["goal"])[:500]

        tools = obj.get("tools_invoked") or []
        for t in tools:
            record.tool_counts[t] = record.tool_counts.get(t, 0) + 1
            if t == "cognition_capability_search":
                record.cap_search += 1
            elif t == "cognition_capability_invoke":
                record.cap_invoke += 1
            elif t == "cognition_spawn_turn_worker":
                record.spawn_worker += 1
            elif t in VAULT_TOOLS:
                record.vault_tools += 1
            elif t in MEMORY_TOOLS:
                record.memory_tools += 1
            elif t in IDENTITY_TOOLS:
                record.identity_tools += 1
            elif t in DISCOVER_TOOLS:
                record.discover_tools += 1

        if kind == "work_delegated":
            record.work_delegated += 1
            if saw_web_fail:
                delegated_after_web_fail = True
        elif kind == "work_failed":
            record.work_failed += 1
        elif kind == "work_completed":
            record.work_completed += 1

        digests = scratch.get("round_digests") or []
        prev_ws_ok = record.ws_ok
        parse_digests(digests, record)

        # Timeline: compact entries for significant events
        if tools or kind in (
            "work_delegated",
            "work_failed",
            "work_completed",
            "finalized",
            "receipt_missing",
            "stuck",
        ):
            digest_tail = digests[-1] if digests else ""
            record.timeline.append(
                {
                    "ts": ts,
                    "kind": kind,
                    "round": obj.get("rounds_executed", 0),
                    "tools": tools,
                    "detail": (obj.get("detail") or "")[:120],
                    "digest_tail": digest_tail[:200],
                }
            )

        if "cognition_web_search" in tools or "cognition_web_search" in str(digests):
            round_n = obj.get("rounds_executed", 0)
            if first_ws_round is None:
                first_ws_round = round_n
            for d in digests:
                if "cognition_web_search:ok" in d:
                    if record.ws_ok > prev_ws_ok:
                        first_ws_ok = True
                if "cognition_web_search:fail" in d:
                    saw_web_fail = True
                    if not first_ws_ok:
                        web_fail_before_ok = True

    record.friction_score = compute_friction_score(record)

    if record.friction_score < 0:
        return record if any(
            record.tool_counts.get(t, 0) > 0
            for t in (
                "cognition_web_search",
                "cognition_capability_search",
                "cognition_capability_invoke",
                "cognition_vault_search",
            )
        ) else None

    if record.ws_fail > 0:
        record.tags.append("web_search_pain")
    if record.friction_score >= 3:
        record.tags.append("web_search_clean")
    if delegated_after_web_fail:
        record.tags.append("worker_escalation")
    if web_fail_before_ok and record.cap_search >= 3:
        record.tags.append("discovery_spiral")
    if proactive_hint(record.goal) and record.vault_tools + record.identity_tools + record.memory_tools == 0:
        record.tags.append("proactive_miss_candidate")

    proactive_score = score_proactive(record)
    if proactive_score:
        record.tags.append(f"proactive_{proactive_score}")

    return record


def select_corpus(sessions: list[SessionRecord]) -> dict[str, list[str]]:
    web_pain = sorted(
        [s for s in sessions if s.ws_fail > 0],
        key=lambda s: (-s.ws_fail, -s.cap_search),
    )
    web_clean = sorted(
        [s for s in sessions if s.friction_score >= 3],
        key=lambda s: -s.ws_ok,
    )
    proactive_miss = [
        s for s in sessions if "proactive_miss_candidate" in s.tags
    ]
    worker_esc = [s for s in sessions if "worker_escalation" in s.tags]

    return {
        "web_search_pain": [s.session_id for s in web_pain[:10]],
        "web_search_clean": [s.session_id for s in web_clean[:4]],
        "proactive_miss": [s.session_id for s in proactive_miss[:5]],
        "worker_escalation": [s.session_id for s in worker_esc[:3]],
    }


def aggregate_stats(sessions: list[SessionRecord]) -> dict[str, Any]:
    ws_fail_reasons: Counter[str] = Counter()
    tool_totals: Counter[str] = Counter()
    for s in sessions:
        for k, v in s.ws_fail_reasons.items():
            ws_fail_reasons[k] += v
        for k, v in s.tool_counts.items():
            tool_totals[k] += v

    ws_sessions = [s for s in sessions if s.ws_ok + s.ws_fail > 0]
    score_dist = Counter(s.friction_score for s in ws_sessions)

    return {
        "total_ledger_files": len(sessions),
        "sessions_with_web_search": len(ws_sessions),
        "web_search_friction_scores": dict(sorted(score_dist.items())),
        "ws_fail_reason_totals": dict(ws_fail_reasons.most_common(20)),
        "top_tool_invocations": dict(tool_totals.most_common(25)),
        "total_ws_ok_digest_hits": sum(s.ws_ok for s in sessions),
        "total_ws_fail_digest_hits": sum(s.ws_fail for s in sessions),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Mine medousa turn_ledger for friction")
    parser.add_argument(
        "--data-dir",
        type=Path,
        default=None,
        help="Medousa data directory (default: platform default or MEDOUSA_DATA_DIR)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Output JSON path (default: architecture/investigations/runtime-friction/daemon-corpus.json)",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[1]
    data_dir = args.data_dir or default_data_dir()
    ledger_dir = data_dir / "turn_ledger"
    output = args.output or (
        repo_root / "architecture" / "investigations" / "runtime-friction" / "daemon-corpus.json"
    )

    if not ledger_dir.is_dir():
        print(f"ERROR: turn_ledger not found at {ledger_dir}", file=sys.stderr)
        return 1

    sessions: list[SessionRecord] = []
    for path in sorted(ledger_dir.glob("*.jsonl")):
        rec = mine_ledger_file(path)
        if rec is not None:
            sessions.append(rec)

    selection = select_corpus(sessions)
    stats = aggregate_stats(sessions)

    # Trim timelines for non-selected sessions to save space
    selected_ids = set()
    for ids in selection.values():
        selected_ids.update(ids)

    output_sessions = []
    for s in sessions:
        d = asdict(s)
        if s.session_id not in selected_ids:
            d["timeline"] = d["timeline"][-3:]  # keep last 3 events only
        output_sessions.append(d)

    payload = {
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "data_dir": str(data_dir),
        "ledger_dir": str(ledger_dir),
        "aggregate_stats": stats,
        "selection": selection,
        "sessions": output_sessions,
    }

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    print(f"Wrote {len(sessions)} sessions to {output}")
    print(f"Selection: {json.dumps(selection, indent=2)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
