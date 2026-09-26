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
import shlex
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

from evals.terminal_bench.runner import DaemonClient, resolve_binaries, run, stop_daemon


@unittest.skipUnless(os.environ.get("MEDOUSA_BENCH_TEST_DAEMON"), "set MEDOUSA_BENCH_TEST_DAEMON")
class NativeDaemonTests(unittest.TestCase):
    def run_scenario(self, actions, verify, *, delayed_startup=False, cancel_turn=False):
        tool_requests = []
        observations = {}
        calls = []
        provider_errors = []

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
                if tools:
                    tool_requests.append(tools)
                    for previous in body.get("messages", []):
                        call_id = previous.get("tool_call_id", "")
                        if previous.get("role") == "tool" and call_id.startswith("smoke-tool-"):
                            observations.setdefault(int(call_id.removeprefix("smoke-tool-")), json.loads(previous["content"]))
                    index = len(calls)
                    if index < len(actions):
                        try:
                            name, arguments = actions[index](observations)
                        except Exception as error:
                            provider_errors.append(str(error))
                            name, arguments = "cognition_turn", {"action": "turn.finish", "message": "fixture failed"}
                        arguments = {"intent": "Exercise the native shell lifecycle", **arguments}
                        call_id = f"smoke-tool-{index}"
                        calls.append((name, arguments))
                    else:
                        name, arguments = "cognition_turn", {
                            "action": "turn.finish", "needs_synthesis": False,
                            "message": "benchmark-smoke-ok", "intent": "Finish the completed smoke-test task",
                        }
                        call_id = "smoke-finish"
                    message = {"role": "assistant", "content": None, "tool_calls": [{
                        "id": call_id, "type": "function", "function": {
                            "name": name, "arguments": json.dumps(arguments),
                        },
                    }]}
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
                "MEDOUSA_TEST_HERMETIC": "1", "RUST_LOG": "warn", "SHELL": "/bin/sh",
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
                if delayed_startup:
                    for name in ("medousa-code", "medousa-session"):
                        binary = bundle / name
                        real = binary.with_suffix(".real")
                        binary.rename(real)
                        binary.write_text("#!/bin/sh\nsleep 6\nexec " + shlex.quote(str(real)) + ' "$@"\n')
                        binary.chmod(0o755)
                    shell = root / "bash"
                    shell.write_text('#!/bin/sh\nsleep 6\nexec /bin/sh -c "$2"\n')
                    shell.chmod(0o755)
                    os.environ["SHELL"] = str(shell)
                workspace = root / "workspace"
                workspace.mkdir()
                instruction = root / "instruction.txt"
                instruction.write_text("Write native-coder-ok to result.txt, then reply benchmark-smoke-ok.")
                cancellation = []
                stop_canceller = threading.Event()

                def cancel_running_command():
                    while not stop_canceller.wait(0.05):
                        if not (workspace / "running.pid").exists():
                            continue
                        try:
                            control = json.loads((root / "state" / "control.json").read_text())
                            secret = json.loads((root / "state" / "data" / "credentials" / "medousa-cli.secret").read_text())
                            client = DaemonClient(control["base_url"], secret["token"])
                            cancellation.append(client.request(
                                f"/v1/sessions/{control['session_id']}/active-turn", "POST"))
                        except Exception as error:
                            provider_errors.append(str(error))
                        return

                def check_cleanup_then_stop(state, wait=False):
                    try:
                        if cancel_turn:
                            pid = int((workspace / "running.pid").read_text())
                            deadline = time.monotonic() + 5
                            while True:
                                try:
                                    os.kill(pid, 0)
                                except ProcessLookupError:
                                    break
                                self.assertLess(time.monotonic(), deadline,
                                                "cancelled command survived turn cleanup")
                                time.sleep(0.05)
                    finally:
                        stop_daemon(state, wait)

                canceller = threading.Thread(target=cancel_running_command, daemon=True)
                if cancel_turn:
                    canceller.start()
                try:
                    with patch("evals.terminal_bench.runner.stop_daemon", check_cleanup_then_stop):
                        result = run(Namespace(
                            workspace=workspace, daemon=bundle / "medousa_daemon", code_bin=None, session_bin=None,
                            instruction=instruction, output=root / "output", state=root / "state",
                            defaults=None, provider="openai", model="gpt-4o", reasoning_effort=None,
                            base_url=f"http://127.0.0.1:{server.server_port}/v1/", timeout=90,
                        ))
                except Exception as error:
                    self.fail(f"{error}\n" + (root / "output" / "daemon.log").read_text()[-6000:])
                finally:
                    stop_canceller.set()
                    if cancel_turn:
                        canceller.join()
                self.assertEqual(result["outcome"], "cancelled" if cancel_turn else "completed", {
                    "result": result,
                    "events": (root / "output" / "events.jsonl").read_text()[-6000:],
                })
                self.assertFalse(provider_errors, provider_errors)
                if cancel_turn:
                    self.assertEqual(len(cancellation), 1, cancellation)
                    self.assertTrue(cancellation[0]["cancelled"])
                else:
                    self.assertEqual(len(observations), len(actions), observations)
                    self.assertIn("cognition_coder_shell_run", result["tool_names"])
                    self.assertEqual((root / "output" / "final.txt").read_text(), "benchmark-smoke-ok")
                verify(workspace, observations)
                self.assertIn("cognition_coder_memory_recall", tool_requests[0])
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

    def test_native_coder_executes_tool_in_scored_workspace(self):
        self.run_scenario([
            lambda _: ("cognition_coder_shell_run", {"command": "printf native-coder-ok > result.txt"}),
        ], lambda workspace, _: self.assertEqual((workspace / "result.txt").read_text(), "native-coder-ok"))

    def test_delayed_sidecars_and_login_shell_survive_startup_windows(self):
        self.run_scenario([
            lambda _: ("cognition_coder_shell_run", {"command": "printf native-coder-ok > result.txt"}),
        ], lambda workspace, _: self.assertEqual((workspace / "result.txt").read_text(), "native-coder-ok"),
            delayed_startup=True)

    def test_failed_commands_do_not_end_turn_and_long_command_survives_polling(self):
        actions = [lambda _: ("cognition_coder_shell_run", {"command": "exit 7"}) for _ in range(4)]
        actions.extend([
            lambda _: ("cognition_coder_shell_run", {
                "command": "printf 'started\n'; sleep 17; printf native-coder-ok > result.txt; printf 'finished\n'",
            }),
            lambda _: ("cognition_coder_shell_run", {"command": "printf parallel > parallel.txt"}),
            lambda seen: ("cognition_coder_shell_run", {"session_id": seen[4]["session_id"], "poll": True}),
        ])
        def verify(workspace, seen):
            self.assertTrue(all(seen[index].get("exit_code") == 7 for index in range(4)), seen)
            self.assertEqual(seen[4]["status"], "running", seen)
            self.assertTrue(seen[4]["ok"])
            self.assertFalse(seen[4]["interrupted"])
            self.assertNotEqual(seen[4]["session_id"], seen[5]["session_id"])
            self.assertEqual(seen[6]["exit_code"], 0, seen)
            self.assertTrue(seen[6]["completed"])
            self.assertEqual((workspace / "result.txt").read_text(), "native-coder-ok")
            self.assertEqual((workspace / "parallel.txt").read_text(), "parallel")
        self.run_scenario(actions, verify)

    def test_explicit_interrupt_stops_running_command(self):
        def verify(workspace, seen):
            self.assertEqual(seen[0]["status"], "running", seen)
            self.assertTrue(seen[2]["interrupted"], seen)
            self.assertTrue(seen[2]["completed"], seen)
            self.assertNotEqual(seen[2]["exit_code"], 0)
            self.assertFalse((workspace / "cancelled.txt").exists())
        self.run_scenario([
            lambda _: ("cognition_coder_shell_run", {"command": "sleep 30; touch cancelled.txt", "wait_ms": 100}),
            lambda seen: ("cognition_shell_session_interrupt", {"session_id": seen[0]["session_id"]}),
            lambda seen: ("cognition_coder_shell_run", {"session_id": seen[0]["session_id"], "poll": True}),
        ], verify)

    def test_busy_command_rejects_a_second_script_and_accepts_interactive_input(self):
        def verify(workspace, seen):
            self.assertEqual(seen[0]["status"], "running")
            self.assertFalse(seen[1]["ok"])
            self.assertIn("running command", seen[1]["error"])
            self.assertTrue(seen[2]["completed"], seen)
            self.assertEqual(seen[2]["exit_code"], 0)
            self.assertEqual((workspace / "result.txt").read_text(), "native-coder-ok")
            self.assertFalse((workspace / "unexpected.txt").exists())
        self.run_scenario([
            lambda _: ("cognition_coder_shell_run", {
                "command": "read answer; printf '%s' \"$answer\" > result.txt", "wait_ms": 100,
            }),
            lambda seen: ("cognition_coder_shell_run", {
                "session_id": seen[0]["session_id"], "command": "touch unexpected.txt",
            }),
            lambda seen: ("cognition_shell_session_run", {
                "session_id": seen[0]["session_id"], "input": "native-coder-ok\n",
            }),
        ], verify)

    def test_turn_cancellation_interrupts_command_before_daemon_shutdown(self):
        self.run_scenario([
            lambda _: ("cognition_coder_shell_run", {
                "command": "echo $$ > running.pid; sleep 30; touch leaked.txt",
            }),
        ], lambda workspace, _: self.assertFalse((workspace / "leaked.txt").exists()), cancel_turn=True)


if __name__ == "__main__":
    unittest.main()
