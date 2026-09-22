"""Opt-in integration check with a real daemon and a scripted local model endpoint.

MEDOUSA_BENCH_TEST_DAEMON=/path/to/medousa_daemon python3 -m unittest \
    evals.terminal_bench.test_native_daemon -v
"""

from argparse import Namespace
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import shutil
import tempfile
import threading
import unittest
from unittest.mock import patch

from evals.terminal_bench.runner import resolve_binaries, run


@unittest.skipUnless(os.environ.get("MEDOUSA_BENCH_TEST_DAEMON"), "set MEDOUSA_BENCH_TEST_DAEMON")
class NativeDaemonTests(unittest.TestCase):
    def test_native_coder_executes_tool_in_scored_workspace(self):
        tool_requests = []

        class Provider(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_GET(self):
                self.respond(json.dumps({"object": "list", "data": [{"id": "gpt-4o", "object": "model"}]}).encode())

            def respond(self, data, streaming=False):
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream" if streaming else "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                tools = [tool.get("function", {}).get("name") for tool in body.get("tools", [])]
                message = {"role": "assistant", "content": "benchmark-smoke-ok"}
                finish = "stop"
                if tools and not tool_requests:
                    tool_requests.append(tools)
                    message = {"role": "assistant", "content": None, "tool_calls": [{
                        "id": "smoke-tool", "type": "function", "function": {
                            "name": "cognition_coder_shell_run",
                            "arguments": json.dumps({
                                "intent": "Write the requested smoke-test file in this undertaking",
                                "command": "printf 'native-coder-ok' > result.txt",
                            }),
                        },
                    }]}
                    finish = "tool_calls"
                elif tools:
                    # Normal ActiveWork ends through the ordinary turn-control tool.
                    message["content"] = None
                    message["tool_calls"] = [{"id": "smoke-finish", "type": "function", "function": {
                        "name": "cognition_turn", "arguments": json.dumps({
                            "action": "turn.finish", "needs_synthesis": False,
                            "message": "benchmark-smoke-ok",
                            "intent": "Finish the completed smoke-test task",
                        }),
                    }}]
                    finish = "tool_calls"
                header = {"id": "smoke", "created": 1, "model": body["model"]}
                usage = {"prompt_tokens": 10, "completion_tokens": 4, "total_tokens": 14}
                if body.get("stream"):
                    delta = dict(message)
                    if "tool_calls" in delta:
                        delta["tool_calls"] = [{"index": 0, **delta["tool_calls"][0]}]
                    chunks = [
                        {**header, "object": "chat.completion.chunk", "choices": [
                            {"index": 0, "delta": delta, "finish_reason": None}]},
                        {**header, "object": "chat.completion.chunk", "choices": [
                            {"index": 0, "delta": {}, "finish_reason": finish}], "usage": usage},
                    ]
                    data = "".join("data: " + json.dumps(chunk) + "\n\n" for chunk in chunks).encode()
                    self.respond(data + b"data: [DONE]\n\n", streaming=True)
                else:
                    self.respond(json.dumps({**header, "object": "chat.completion", "choices": [
                        {"index": 0, "message": message, "finish_reason": finish}], "usage": usage}).encode())

        server = ThreadingHTTPServer(("127.0.0.1", 0), Provider)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {
                "OPENAI_API_KEY": "smoke-placeholder", "STASIS_LLM_API_KEY": "smoke-placeholder",
                "MEDOUSA_TEST_HERMETIC": "1", "RUST_LOG": "warn",
                "MEDOUSA_CODE_BIN": "/missing/ambient-medousa-code",
                "MEDOUSA_SESSION_BIN": "/missing/ambient-medousa-session",
            }):
                root = Path(directory).resolve()
                bundle = root / "bundle"
                bundle.mkdir()
                binaries = resolve_binaries(os.environ["MEDOUSA_BENCH_TEST_DAEMON"],
                                            os.environ.get("MEDOUSA_BENCH_TEST_CODE"),
                                            os.environ.get("MEDOUSA_BENCH_TEST_SESSION"))
                for name, path in binaries.items():
                    shutil.copy2(path, bundle / name)
                workspace = root / "workspace"
                workspace.mkdir()
                instruction = root / "instruction.txt"
                instruction.write_text("Write native-coder-ok to result.txt, then reply benchmark-smoke-ok.")
                result = run(Namespace(
                    workspace=workspace, daemon=bundle / "medousa_daemon", code_bin=None, session_bin=None,
                    instruction=instruction, output=root / "output", state=root / "state",
                    defaults=None, provider="openai", model="gpt-4o", reasoning_effort=None,
                    base_url=f"http://127.0.0.1:{server.server_port}/v1/", timeout=45,
                ))
                self.assertEqual(result["outcome"], "completed", {
                    "result": result,
                    "events": (root / "output" / "events.jsonl").read_text()[-6000:],
                })
                self.assertEqual((workspace / "result.txt").read_text(), "native-coder-ok")
                self.assertIn("cognition_coder_memory_recall", tool_requests[0])
                self.assertIn("cognition_coder_shell_run", result["tool_names"])
                self.assertEqual((root / "output" / "final.txt").read_text(), "benchmark-smoke-ok")
                self.assertFalse((root / "state" / "daemon.pid").exists())
                sidecars = json.loads((root / "output" / "sidecars.json").read_text())
                self.assertTrue(sidecars["medousa-code"]["available"])
                self.assertTrue(sidecars["medousa-session"]["available"])
                manifest = json.loads((root / "output" / "manifest.json").read_text())
                self.assertEqual(set(manifest["binary_sha256"]), set(binaries))
        finally:
            server.shutdown()
            server.server_close()
            thread.join()


if __name__ == "__main__":
    unittest.main()
