#!/usr/bin/env python3
"""Run ADR-021 against two loopback daemons and a deterministic model on macOS."""

import argparse, json, os, socket, subprocess, sys, tempfile, threading, time
import urllib.error, urllib.request
from datetime import datetime, timezone
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from contextlib import ExitStack

SCENARIOS = ("success", "failure", "cancel", "peer-policy", "portal-policy")
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument(
    "--daemon", required=True, type=Path, help="built medousa_daemon binary"
)
parser.add_argument("--case", choices=(*SCENARIOS, "all"), default="all")
args = parser.parse_args()
if sys.platform != "darwin":
    parser.error(
        "the fixture uses the macOS sandbox for loopback and Keychain isolation"
    )
if not args.daemon.is_file():
    parser.error("daemon binary does not exist")
if args.case == "all":
    for scenario in SCENARIOS:
        subprocess.run(
            [
                sys.executable,
                __file__,
                "--daemon",
                str(args.daemon),
                "--case",
                scenario,
            ],
            check=True,
        )
    sys.exit(0)
BIN = str(args.daemon.resolve())
case = args.case
fixture = tempfile.TemporaryDirectory(prefix="medousa-session-")
root = Path(fixture.name).resolve()


def reserve():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


ports = {k: reserve() for k in ["A", "C", "providerA", "providerC"]}


def call(k, path, body=None, method=None):
    h = {"Content-Type": "application/json"}
    cred = root / k / "credentials" / "home-local.secret"
    if cred.exists():
        h["Authorization"] = "Bearer " + json.loads(cred.read_text())["token"]
    req = urllib.request.Request(
        f"http://127.0.0.1:{ports[k]}" + path,
        data=None if body is None else json.dumps(body).encode(),
        headers=h,
        method=method,
    )
    try:
        with urllib.request.urlopen(req, timeout=13) as r:
            return json.loads(r.read())
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"{k} {path} {e.code}: " + e.read().decode()[:500])


state = {}
calls = {"A": 0, "B": 0, "C": 0}
done = threading.Event()
errors = []


def response(k, data):
    raw = json.dumps(data["messages"], ensure_ascii=False)
    role = (
        "C"
        if k == "C"
        else (
            "B"
            if "Native assignment execution policy: complete this work" in raw
            else "A"
        )
    )
    calls[role] += 1
    n = calls[role]
    print("round", role, n, data["model"], flush=True)
    defs = [t["function"]["name"] for t in data.get("tools", [])]

    def action(name, args):
        if name not in defs:
            raise ValueError(f"{role} missing {name}; visible {defs}")
        return name, args

    events = " ".join(
        str(m.get("content", ""))
        for m in data["messages"]
        if m.get("role") == "user"
        and "[Agent session event]" in str(m.get("content", ""))
    )
    if role == "A":
        contract = (
            data["model"],
            sorted(defs),
            [m["content"] for m in data["messages"] if m["role"] == "system"],
        )
        if "contract" not in state:
            state["contract"] = contract
        assert (
            contract[:2] == state["contract"][:2]
            and contract[2][0] == state["contract"][2][0]
        ), "A changed model or tools"
    if role == "B":
        if "ABC clarification blue" in events:
            return action(
                "cognition_turn",
                {
                    "action": "turn.finish",
                    "message": "B confirms blue",
                    "intent": "report result",
                },
            )
        for _ in range(180):
            if state.get("sent"):
                break
            time.sleep(0.15)
        time.sleep(0.3)
        return action(
            "cognition_turn",
            {
                "action": "turn.update_user",
                "message": "B working",
                "intent": "progress",
            },
        )
    if role == "C":
        if case.endswith("-policy"):
            previous = next(
                (
                    str(m.get("content", ""))
                    for m in reversed(data["messages"])
                    if m.get("role") == "tool"
                ),
                "",
            )
            if n == 1:
                return action(
                    "cognition_store_read",
                    {
                        "action": "code.read",
                        "path": "sample.txt",
                        "intent": "Read the authorized fixture file",
                    },
                )
            if n == 2:
                assert (
                    "ABC fixture" in previous
                ), f"authorized read failed: {previous[:400]}"
                state["allowed"] = True
                if case == "peer-policy":
                    policy = {**state["policy"], "allowedToolDomains": ["turn", "code"]}
                    call("C", state["policy_path"], policy, "PUT")
                    path = "sample.txt"
                else:
                    path = "../outside.txt"
                return action(
                    "cognition_store_read",
                    {
                        "action": "code.read",
                        "path": path,
                        "intent": "Verify the destination permission boundary",
                    },
                )
            assert (
                "fixture confidential value" not in previous
                and "ABC fixture" not in previous
            ), previous[:400]
            assert any(
                word in previous.lower()
                for word in [
                    "deni",
                    "outside",
                    "escap",
                    "relative",
                    "travers",
                    "policy",
                    "invalid",
                ]
            ), previous[:400]
            state["denied"] = True
        if case == "cancel":
            for _ in range(100):
                if state.get("cancelled"):
                    break
                time.sleep(0.1)
        time.sleep(0.7)
        return action(
            "cognition_turn",
            {
                "action": "turn.finish",
                "message": "C finished green",
                "intent": "report result",
            },
        )
    has_c = (
        "C finished green" in events
        if case in ("success", "peer-policy", "portal-policy")
        else ("failed" if case == "failure" else "cancelled") in events.lower()
    )
    if case.endswith("-policy") and state.get("allowed") and "failed" in events.lower():
        has_c = True
    if "B confirms blue" in events and has_c:
        done.set()
        return action(
            "cognition_turn",
            {"action": "turn.finish", "message": "A received both worker outcomes"},
        )
    if n == 1:
        return action(
            "cognition_peer_handoff",
            {
                "request_key": "abc-B",
                "instructions": "Return B confirms blue only after A clarifies.",
                "forge_work_id": state["Bwork"],
                "after_entry_seq": 0,
                "through_entry_seq": 1,
                "runtime": "medousa",
            },
        )
    if n in (2, 3):
        accepted = json.loads(
            next(
                message["content"]
                for message in reversed(data["messages"])
                if message["role"] == "tool"
            )
        )
        worker = "B" if n == 2 else "C"
        assert accepted["status"] == (
            "accepted" if worker == "B" else "started"
        ), accepted
        state[worker + "request"] = accepted["request_id"]
        state[worker + "session"] = accepted["session_id"]
    if n == 2:
        return action(
            "cognition_peer_delegate",
            {
                "execution_runtime_id": state["Cdevice"],
                "forge_work_id": state["Cwork"],
                "runtime": "medousa",
                "instructions": "Return C finished green",
                "request_key": "abc-C",
            },
        )
    if n == 3:
        if case == "cancel":
            reply = call("C", f"/v1/sessions/{state['Csession']}/active-turn", {})
            assert reply["cancelled"], reply
            state["cancelled"] = True
        other = call("A", "/v1/sessions", {"display_name": "ABC different chat"})
        state["other"] = other["session_id"]
        pref = root / "A/tui_defaults.json"
        values = json.loads(pref.read_text()) if pref.exists() else {}
        values.update(provider="openai", model="picker-changed")
        pref.write_text(json.dumps(values))
        # Keep A in its ordinary model round until the signed C result has been applied.
        for _ in range(200):
            files = list((root / "A").rglob("*completion*.json"))
            if any(
                any(
                    o.get("completionApplied")
                    for o in json.loads(f.read_text()).get("origins", [])
                )
                for f in files
            ):
                break
            time.sleep(0.1)
        return action(
            "cognition_turn",
            {"action": "turn.update_user", "message": "Checking on B."},
        )
    if has_c and not state.get("sent"):
        state["sent"] = True
        return action(
            "cognition_agent_message",
            {
                "action": "send",
                "target": state["Bsession"],
                "message": "ABC clarification blue",
                "request_key": "abc-clarify",
            },
        )
    return action(
        "cognition_turn",
        {"action": "turn.checkpoint", "message": "Waiting for B and C"},
    )


def mock(k):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *a):
            pass

        def do_POST(self):
            try:
                data = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                if k == "C" and case == "failure":
                    calls["C"] += 1
                    self.send_error(400, "ABC worker failure")
                    return
                name, args = response(k, data)
                tc = {
                    "id": f"call_{k}_{time.monotonic_ns()}",
                    "type": "function",
                    "function": {"name": name, "arguments": json.dumps(args)},
                }
                base = {
                    "id": "chatcmpl-fixture",
                    "model": data["model"],
                    "created": int(time.time()),
                }
                if data.get("stream"):
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.end_headers()
                    for delta, reason in [
                        (
                            {"role": "assistant", "tool_calls": [dict(tc, index=0)]},
                            None,
                        ),
                        ({}, "tool_calls"),
                    ]:
                        chunk = dict(
                            base,
                            object="chat.completion.chunk",
                            choices=[dict(index=0, delta=delta, finish_reason=reason)],
                        )
                        self.wfile.write(
                            ("data: " + json.dumps(chunk) + "\n\n").encode()
                        )
                    self.wfile.write(b"data: [DONE]\n\n")
                    self.wfile.flush()
                else:
                    body = json.dumps(
                        dict(
                            base,
                            object="chat.completion",
                            choices=[
                                {
                                    "index": 0,
                                    "message": {
                                        "role": "assistant",
                                        "content": None,
                                        "tool_calls": [tc],
                                    },
                                    "finish_reason": "tool_calls",
                                }
                            ],
                            usage={
                                "prompt_tokens": 256,
                                "completion_tokens": 20,
                                "total_tokens": 276,
                            },
                        )
                    ).encode()
                    self.send_response(200)
                    self.send_header("Content-type", "application/json")
                    self.send_header("Content-length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
            except Exception as e:
                errors.append(str(e))
                print("fixture provider:", e, flush=True)
                self.send_error(500, "fixture failed")

    server = ThreadingHTTPServer(("127.0.0.1", ports["provider" + k]), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


with ExitStack() as stack:
    stack.callback(fixture.cleanup)
    for k in ["A", "C"]:
        d = root / k
        d.mkdir()
        (d / "migrations").mkdir()
        (d / "migrations/daemon-legacy-secrets-v1.complete").write_text("complete\n")
        now = datetime.now(timezone.utc).isoformat()
        catalog = d / "model_catalog"
        catalog.mkdir()
        snap = {
            "provider": "openai",
            "fetched_at": now,
            "source": "fixture",
            "models": [
                {
                    "provider": "openai",
                    "model_id": "gpt-4o",
                    "display_name": "ABC test model",
                    "input_modalities": ["text"],
                    "output_modalities": ["text"],
                    "max_input_tokens": 128000,
                    "max_output_tokens": 4096,
                    "supports_tool_calling": True,
                    "supports_vision": False,
                    "pricing": None,
                    "source": "fixture",
                    "fetched_at": now,
                }
            ],
        }
        (catalog / "openai.json").write_text(json.dumps(snap))
        (catalog / "index.json").write_text(
            json.dumps(
                {
                    "ttl_secs": 86400,
                    "providers": {
                        provider: {
                            "fetched_at": now,
                            "model_count": 1,
                            "source": "fixture",
                            "error": None,
                        }
                        for provider in ["openai", "openrouter"]
                    },
                }
            )
        )
        server = mock(k)
        stack.callback(server.server_close)
        stack.callback(server.shutdown)
        url = f'http://127.0.0.1:{ports["provider"+k]}/v1/'
        env = {
            key: os.environ[key]
            for key in ["PATH", "TMPDIR", "LANG"]
            if key in os.environ
        }
        env.update(
            MEDOUSA_DATA_DIR=str(d),
            MEDOUSA_CONFIG_DIR=str(d / "config"),
            MEDOUSA_IROH="0",
            OPENAI_API_KEY="local-fixture",
            MEDOUSA_LLM_BASE_URL=url,
            MEDOUSA_LLM_BASE_URL_OPENAI=url,
            MEDOUSA_LLM_PROVIDER="openai",
            MEDOUSA_LLM_MODEL="gpt-4o",
            NO_PROXY="127.0.0.1,localhost",
        )
        policy = '(version 1)(allow default)(deny mach-lookup (global-name "com.apple.securityd") (global-name "com.apple.SecurityServer"))(deny network-outbound)(allow network-outbound (remote ip "localhost:*"))'
        proc = subprocess.Popen(
            [
                "/usr/bin/sandbox-exec",
                "-p",
                policy,
                BIN,
                "--bind",
                f"127.0.0.1:{ports[k]}",
                "--provider",
                "openai",
                "--model",
                "gpt-4o",
                "--base-url",
                url,
            ],
            stdout=stack.enter_context((d / "daemon.log").open("w")),
            stderr=subprocess.STDOUT,
            cwd=root,
            env=env,
        )
        if k == "A":
            state["restart"] = (proc, proc.args, env)

        def stop(proc):
            proc.terminate()
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()

        stack.callback(stop, proc)
        for _ in range(120):
            if proc.poll() is not None:
                raise RuntimeError(
                    f"{k} daemon stopped: " + (d / "daemon.log").read_text()[-600:]
                )
            try:
                call(k, "/health")
                break
            except Exception:
                time.sleep(0.25)
        else:
            raise RuntimeError("startup timeout")
        print(k, "ready", flush=True)
    qr = call("C", "/qr")
    remote = f'http://127.0.0.1:{ports["C"]}'
    if case == "peer-policy":
        pair = call(
            "A", "/v1/peers/pair", {"invite": qr["url"], "url": remote, "name": "ABC C"}
        )
    else:
        pair = call(
            "A",
            "/v1/workers/pair",
            {"pairingUrl": qr["url"], "daemonUrl": remote, "label": "ABC C"},
        )
    state["Cdevice"] = pair["workshopDeviceId"]
    for k in ["A", "C"]:
        repo = root / k / "repo"
        repo.mkdir()

        def git(args):
            subprocess.run(
                ["git", "-C", str(repo), *args], check=True, capture_output=True
            )

        git(["init", "-b", "main"])
        (repo / "sample.txt").write_text("ABC fixture\n")
        git(["add", "."])
        git(
            [
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@localhost",
                "commit",
                "-m",
                "Fixture",
            ]
        )
        item = call(
            k,
            "/v1/forge/items",
            {
                "title": "ABC work " + k,
                "brief": "A/B/C verification",
                "repo_path": str(repo),
                "base_ref": "main",
                "workspace_mode": "attached_checkout",
            },
        )
        work = item.get("id") or item.get("work_id") or item.get("item", {}).get("id")
        if not work:
            raise RuntimeError("work response keys " + str(item.keys()))
        state["Bwork" if k == "A" else "Cwork"] = work
        call(k, "/v1/forge/items/" + work + "/provision", {})
    (root / "C/outside.txt").write_text("fixture confidential value")
    if case == "peer-policy":
        source = call("A", "/pair/status")["deviceId"]
        state["policy_path"] = f"/v1/peers/{source}/execution-policy"
        state["policy"] = {
            "preset": "custom",
            "enabled": True,
            "assistantWork": True,
            "coderWork": True,
            "allowAgentTargeting": True,
            "hostShell": False,
            "allowedProjectIds": [str(root / "C/repo/.git")],
            "allowedToolDomains": ["turn", "code", "data"],
        }
        call("C", state["policy_path"], state["policy"], "PUT")
    session = call("A", "/v1/sessions", {"display_name": "ABC origin"})
    sid = session["session_id"]
    call("A", f"/v1/sessions/{sid}/code-binding", {"work_id": state["Bwork"]}, "PUT")
    roles = [
        "orchestrator",
        "chunker",
        "extractor",
        "summarizer",
        "verifier",
        "packer",
        "final_response",
    ]
    routing = {
        r: {
            "role": r,
            "provider": "openai",
            "model": "gpt-4o",
            "policy_profile": "balanced",
            "fallback_chain": [],
        }
        for r in roles
    }
    reply = call(
        "A",
        "/v1/interactive/turn",
        {
            "session_id": sid,
            "prompt": "Start a local Coder B on this project and C on the paired workshop. Clarify the color for B after C returns.",
            "agent_mode": "assistant",
            "provider": "openai",
            "model": "gpt-4o",
            "persist_user_turn": True,
            "response_depth_mode": "standard",
            "stage_routing": routing,
            "max_tool_rounds": 18,
        },
    )
    print("origin admitted", sid, reply["turn_id"], flush=True)
    for _ in range(250):
        if errors:
            raise RuntimeError(errors[0])
        if "interactive_turn_failed" in (root / "A/daemon.log").read_text():
            raise RuntimeError("turn failed; see fixture daemon log")
        if done.wait(0.4):
            break
    else:
        raise RuntimeError("A did not finish; counts " + str(calls))
    time.sleep(1)
    assert (
        calls["B"] >= 2 and (calls["C"] >= 1 or case == "cancel") and state.get("sent")
    ), str(calls)
    for _ in range(50):
        transcript = call("A", f"/v1/sessions/{sid}/history")
        if "A received both worker outcomes" in json.dumps(transcript):
            break
        time.sleep(0.1)
    else:
        raise RuntimeError("final response missing in original chat")
    outcomes = []
    for entry in transcript["turns"]:
        try:
            receipt = json.loads(entry["content"])
        except ValueError:
            continue
        if (
            isinstance(receipt, dict)
            and "outcome" in receipt
            and "request_id" in receipt
        ):
            outcomes.append(receipt)
    assert len(outcomes) == len({r["request_id"] for r in outcomes}) == 2, outcomes
    assert [r["request_id"] for r in outcomes] == [
        state["Crequest"],
        state["Brequest"],
    ], outcomes
    assert outcomes[1]["result"] == "B confirms blue", outcomes
    assert outcomes[0]["outcome"] == {"failure": "failed", "cancel": "cancelled"}.get(
        case, "completed"
    ), outcomes
    assert call("A", f"/v1/sessions/{state['other']}/history")["turns"] == []
    assert calls["A"] == 6 and calls["B"] == 2, calls
    assert len([p for p in (root / "A/history").glob("*.jsonl")]) == 2
    if case == "success" or case == "failure":
        assert calls["C"] == 1, calls
    if case.endswith("-policy"):
        assert state.get("allowed"), "destination never allowed the initial read"
        if not state.get("denied"):
            logs = (root / "C/daemon.log").read_text()
            assert (
                "current peer policy denies cognition_store_read" in logs
            ), "missing denial for the second read"
    # Replay a verified, signed terminal outcome after losing the source process.
    if case == "success":
        before = call("A", f"/v1/sessions/{sid}/history")["turns"]
        a_proc, cmd, env = state["restart"]
        stop(a_proc)
        journal = next(
            f
            for f in (root / "A").rglob("*completion*.json")
            if json.loads(f.read_text()).get("origins")
        )
        obj = json.loads(journal.read_text())
        obj["origins"][0]["completionApplied"] = False
        journal.write_text(json.dumps(obj))
        reopened = subprocess.Popen(
            cmd,
            stdout=stack.enter_context((root / "A/restarted.log").open("w")),
            stderr=subprocess.STDOUT,
            cwd=root,
            env=env,
        )
        stack.callback(stop, reopened)
        for _ in range(350):
            if json.loads(journal.read_text())["origins"][0].get("completionApplied"):
                break
            time.sleep(0.1)
        else:
            raise RuntimeError("signed duplicate was not recovered")
        after = call("A", f"/v1/sessions/{sid}/history")["turns"]
        assert [(e["entry_id"], e["content_digest"]) for e in after] == [
            (e["entry_id"], e["content_digest"]) for e in before
        ]
        assert calls == {"A": 6, "B": 2, "C": 1}
    print("PASS", case, calls, flush=True)
