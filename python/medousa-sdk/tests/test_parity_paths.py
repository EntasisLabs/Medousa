"""Route parity — validated against sdk-contract/manifest.yaml."""

from __future__ import annotations

from pathlib import Path

import pytest
import yaml

from medousa import MedousaClient, MedousaClientSync

MANIFEST_PATH = Path(__file__).resolve().parents[3] / "sdk-contract" / "manifest.yaml"


def load_manifest_methods() -> list[dict]:
    with MANIFEST_PATH.open(encoding="utf-8") as handle:
        data = yaml.safe_load(handle)
    return data["methods"]


MANIFEST_METHODS = load_manifest_methods()


def _accessor_method(client: MedousaClient | MedousaClientSync, accessor: str, method: str):
    api = getattr(client, accessor)()
    return getattr(api, method)


@pytest.mark.parametrize(
    "entry",
    MANIFEST_METHODS,
    ids=[f"{e['accessor']}.{e['method']}" for e in MANIFEST_METHODS],
)
def test_async_method_exists(entry: dict):
    if entry.get("status") == "planned":
        pytest.skip("planned method")
    client = MedousaClient("http://127.0.0.1:7419")
    fn = _accessor_method(client, entry["accessor"], entry["method"])
    assert callable(fn)


SYNC_MANIFEST_METHODS = [
    e for e in MANIFEST_METHODS if e.get("sync") and e.get("status") != "planned"
]


@pytest.mark.parametrize(
    "entry",
    SYNC_MANIFEST_METHODS,
    ids=[f"sync:{e['accessor']}.{e['method']}" for e in SYNC_MANIFEST_METHODS],
)
def test_sync_method_exists(entry: dict):
    client = MedousaClientSync("http://127.0.0.1:7419")
    fn = _accessor_method(client, entry["accessor"], entry["method"])
    assert callable(fn)


def test_manifest_minimum_accessors():
    accessors = {entry["accessor"] for entry in MANIFEST_METHODS}
    expected = {
        "health",
        "ingest",
        "local_models",
        "jobs",
        "recurring",
        "sessions",
        "interactive",
        "runtime",
        "capabilities",
        "mcp_gateway",
        "budget",
        "vault",
        "environment",
        "components",
        "feeds",
        "workspace",
    }
    assert expected <= accessors


def test_manifest_route_metadata():
    for item in MANIFEST_METHODS:
        assert item["accessor"]
        assert item["method"]
        assert item["http"] in {"GET", "POST", "PUT", "PATCH", "DELETE"}
        assert item["path"].startswith("/")
