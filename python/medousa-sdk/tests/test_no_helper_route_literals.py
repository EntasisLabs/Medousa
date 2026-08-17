from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "src" / "medousa"


def test_helpers_do_not_embed_daemon_route_literals() -> None:
    hits: list[str] = []
    for path in ROOT.rglob("*.py"):
        if "_generated" in path.parts:
            continue
        for index, line in enumerate(path.read_text().splitlines(), start=1):
            if '"/v1/' in line or "'/v1/" in line:
                hits.append(f"{path}:{index}: {line}")
    assert hits == [], "helpers still embed /v1 literals:\n" + "\n".join(hits)
