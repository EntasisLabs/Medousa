"""Generated operations are the Python SDK contract."""

from medousa._generated.ops import OPERATIONS, by_id


def test_generated_ops_are_real_http_methods() -> None:
    assert OPERATIONS
    seen: set[tuple[str, str]] = set()
    for operation in OPERATIONS.values():
        assert operation.method != "SSE"
        assert operation.method in {"GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"}
        assert operation.path.startswith("/")
        assert "?" not in operation.path
        key = (operation.method, operation.path)
        assert key not in seen
        seen.add(key)


def test_health_paths() -> None:
    assert by_id("health.get").path == "/v1/health"
    assert by_id("liveness.get").path == "/health"
