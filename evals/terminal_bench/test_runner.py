import io
import json
from pathlib import Path
import tempfile
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import Mock

from evals.terminal_bench import runner


def envelope(seq, event, turn_id="turn-1"):
    return {"schema_version": 3, "seq": seq, "turn_id": turn_id, "event": event}


def stream(*events):
    return b"".join(b"data: " + json.dumps(event).encode() + b"\n\n" for event in events)


class StreamTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name)
        self.client = Mock()
        self.process = Mock()
        self.process.poll.return_value = None
        self.turn = {"turn_id": "turn-1", "stream_url": "/stream"}

    def follow(self, deadline=None):
        return runner.follow_turn(self.client, self.turn, self.output, self.process, deadline)

    def test_reconnect_replays_same_turn_and_deduplicates(self):
        first = envelope(1, {"type": "content_append", "segment_id": "a", "text": "x"})
        final = {"type": "turn_completed", "outcome": "completed", "aggregate_text": "done"}
        self.client.open.side_effect = [io.BytesIO(stream(first)), io.BytesIO(stream(first, envelope(2, final)))]
        self.assertEqual(self.follow(), final)
        self.assertEqual([call.args[0] for call in self.client.open.call_args_list],
                         ["/stream?since=0", "/stream?since=1"])
        self.assertEqual(len((self.output / "events.jsonl").read_text().splitlines()), 2)

    def test_fails_on_wrong_turn_instead_of_scoring_another_response(self):
        self.client.open.return_value = io.BytesIO(stream(envelope(1, {"type": "status"}, "other")))
        with self.assertRaisesRegex(ValueError, "identity"):
            self.follow()

    def test_eof_is_not_success(self):
        self.client.open.return_value = io.BytesIO(b"")
        with self.assertRaises(TimeoutError):
            self.follow(time.monotonic() + 0.1)

    def test_daemon_exit_is_not_success(self):
        self.process.poll.return_value = 1
        with self.assertRaisesRegex(RuntimeError, "exited"):
            self.follow()

    def test_operator_requests_do_not_approve_or_wait_forever(self):
        for kind in ("permission_request", "budget_approval_required", "secret_request"):
            with self.subTest(kind=kind):
                self.client.open.return_value = io.BytesIO(stream(envelope(1, {"type": kind})))
                self.assertEqual(self.follow()["outcome"], "needs_input")

    def test_failed_terminal_is_preserved(self):
        event = {"type": "turn_completed", "outcome": "failed", "aggregate_text": ""}
        self.client.open.return_value = io.BytesIO(stream(envelope(1, event)))
        self.assertEqual(self.follow()["outcome"], "failed")

    def test_sse_multiline_and_incomplete_frame(self):
        data = b': heartbeat\nevent: turn_stream_v3\ndata: {"seq":\ndata: 1}\n\ndata: {"seq": 2}'
        self.assertEqual(list(runner.sse_events(io.BytesIO(data))), [{"seq": 1}])


class IngressTests(unittest.TestCase):
    def test_normal_http_ingress_preserves_prompt_settings_and_workspace(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            workspace = root / "workspace"
            workspace.mkdir()
            received = []
            defaults = {"provider": "test", "model": "test-model", "response_depth_mode": "deep",
                        "reasoning_effort": "high", "stage_routing": {"routes": {}}}

            class Handler(BaseHTTPRequestHandler):
                def log_message(self, *_):
                    pass

                def do_GET(self):
                    self.handle_request()

                do_POST = do_PUT = do_GET

                def handle_request(self):
                    length = int(self.headers.get("Content-Length", "0"))
                    body = json.loads(self.rfile.read(length)) if length else None
                    received.append((self.command, self.path, body, dict(self.headers)))
                    responses = {
                        "/v1/coding-engine": {"available": True},
                        "/v1/shell-sessions": {"available": True},
                        "/v1/runtime/defaults": defaults,
                        "/v1/sessions": {"session_id": "ses_test"},
                        "/v1/forge/items/start": {"id": "work-1", "state": "ready",
                                                  "environment": {"worktree": str(workspace)}},
                        "/v1/interactive/turn": {"turn_id": "turn-1", "stream_url":
                            f"http://127.0.0.1:{self.server.server_port}/stream"},
                    }
                    if self.path.startswith("/stream"):
                        data = stream(envelope(1, {"type": "turn_completed", "outcome": "completed",
                                                   "aggregate_text": "final answer"}))
                    else:
                        data = json.dumps(responses.get(self.path, {})).encode()
                    self.send_response(200)
                    self.send_header("Content-Length", str(len(data)))
                    self.end_headers()
                    self.wfile.write(data)

            server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                client = runner.DaemonClient(f"http://127.0.0.1:{server.server_port}", "test-secret")
                process = Mock()
                process.poll.return_value = None
                instruction = "  Preserve this prompt.\nUnicode: λ; $(touch nope)\n"
                result = runner.run_coder(client, instruction, workspace, "main", root, process)
            finally:
                server.shutdown()
                server.server_close()
                thread.join()
            self.assertEqual(result["outcome"], "completed")
            self.assertEqual(json.loads((root / "sidecars.json").read_text()), {
                "medousa-code": {"available": True}, "medousa-session": {"available": True},
            })
            self.assertEqual((root / "final.txt").read_text(), "final answer")
            turns = [body for method, path, body, _ in received if path == "/v1/interactive/turn"]
            self.assertEqual(len(turns), 1)
            self.assertEqual(turns[0]["prompt"], instruction)
            self.assertEqual(turns[0]["agent_mode"], "coder")
            for key, value in defaults.items():
                self.assertEqual(turns[0][key], value)
            self.assertNotIn("scheduled_tool_allowlist", turns[0])
            self.assertNotIn("max_tool_rounds", turns[0])
            self.assertNotIn("worker_execution_target", turns[0])
            self.assertIn(("PUT", "/v1/sessions/ses_test/code-binding", {"work_id": "work-1"}),
                          [(m, p, b) for m, p, b, _ in received])
            self.assertEqual(received[-1][1], "/v1/sessions/ses_test/active-turn")
            self.assertTrue(all(headers["Authorization"] == "Bearer test-secret" for _, _, _, headers in received))
            self.assertEqual(next(headers["Accept"] for _, p, _, headers in received if p.startswith("/stream")),
                             "text/event-stream; medousa-version=3")

    def test_rejects_workspace_mismatch_before_sending_prompt(self):
        client = Mock()
        client.request.side_effect = [{"available": True}, {"available": True}, {}, {"session_id": "s"}, {
            "state": "ready", "environment": {"worktree": "/different"}
        }]
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(RuntimeError, "workspace differs"):
                runner.run_coder(client, "task", Path("/workspace"), "main", Path(directory), Mock())
        self.assertFalse(any(call.args[0] == "/v1/interactive/turn" for call in client.request.call_args_list))

    def test_unavailable_sidecar_stops_before_session_or_paid_turn(self):
        for name in ("medousa-code", "medousa-session"):
            with self.subTest(binary=name), tempfile.TemporaryDirectory() as directory:
                client = Mock()
                failure = {"available": False, "message": "binary failed to start"}
                client.request.side_effect = ([{"available": True}] if name == "medousa-session" else []) + [failure]
                output = Path(directory)
                with self.assertRaisesRegex(RuntimeError, f"{name} is unavailable: binary failed to start"):
                    runner.run_coder(client, "task", Path("/workspace"), "main", output, Mock())
                self.assertEqual(json.loads((output / "sidecars.json").read_text())[name], failure)
                self.assertTrue(all(call.args[0] in ("/v1/coding-engine", "/v1/shell-sessions")
                                    for call in client.request.call_args_list))

    def test_credentials_never_follow_a_different_stream_origin(self):
        client = runner.DaemonClient("http://127.0.0.1:1234", "secret")
        with self.assertRaisesRegex(ValueError, "another origin"):
            client.open("http://127.0.0.1:5678/stream")


class RepositoryTests(unittest.TestCase):
    def test_plain_task_directory_becomes_repository_without_moving_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "input.txt").write_text("original")
            branch = runner.prepare_repository(root)
            self.assertTrue(branch.startswith("codex/"))
            self.assertEqual((root / "input.txt").read_text(), "original")
            (root / "input.txt").write_text("dirty")
            self.assertEqual(runner.prepare_repository(root), branch)
            self.assertEqual((root / "input.txt").read_text(), "dirty")

    def test_does_not_attach_ancestor_repository(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runner.prepare_repository(root)
            child = root / "nested"
            child.mkdir()
            with self.assertRaisesRegex(ValueError, "repository root"):
                runner.prepare_repository(child)


class BinaryTests(unittest.TestCase):
    def test_missing_sidecars_are_rejected_without_searching_host(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            daemon = root / "medousa_daemon"
            daemon.touch()
            with self.assertRaisesRegex(ValueError, "Required medousa-code binary is missing"):
                runner.resolve_binaries(daemon)
            (root / "medousa-code").touch()
            with self.assertRaisesRegex(ValueError, "Required medousa-session binary is missing"):
                runner.resolve_binaries(daemon)


if __name__ == "__main__":
    unittest.main()
