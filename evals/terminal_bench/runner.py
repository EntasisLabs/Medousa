#!/usr/bin/env python3
"""One ordinary daemon + Coder conversation inside a disposable task environment."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import signal
import socket
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def git(workspace, *args):
    return subprocess.check_output(
        ["git", "-C", str(workspace), *args], stderr=subprocess.PIPE
    ).decode().strip()


def prepare_repository(workspace):
    """Keep the task's paths and dirty files; Forge attaches this exact checkout."""
    workspace = workspace.resolve(strict=True)
    if workspace == Path(workspace.anchor):
        raise ValueError("Choose a task workspace, not the filesystem root (--workspace).")
    try:
        root = Path(git(workspace, "rev-parse", "--show-toplevel")).resolve()
    except subprocess.CalledProcessError:
        git(workspace, "init", "-b", "codex/medousa-benchmark")
        git(workspace, "add", "--all")
        git(workspace, "-c", "user.name=Medousa Benchmark",
            "-c", "user.email=benchmark@localhost", "-c", "commit.gpgsign=false",
            "commit", "--allow-empty", "-m", "Benchmark input")
        root = workspace
    if root != workspace:
        raise ValueError(f"Workspace must be its repository root: {root}")
    try:
        git(workspace, "rev-parse", "--verify", "HEAD")
    except subprocess.CalledProcessError as error:
        raise ValueError("Existing task repository needs an initial commit") from error
    branch = git(workspace, "branch", "--show-current")
    if not branch:
        branch = "codex/medousa-benchmark-" + uuid.uuid4().hex[:8]
        git(workspace, "switch", "-c", branch)
    return branch


class DaemonClient:
    def __init__(self, base_url, token):
        self.base_url = base_url
        self.token = token
        # Loopback traffic must never inherit a provider HTTP proxy.
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def open(self, path, method="GET", body=None, timeout=30, accept="application/json"):
        parsed = urllib.parse.urlsplit(path)
        if parsed.scheme or parsed.netloc:
            origin = urllib.parse.urlsplit(self.base_url)
            if (parsed.scheme, parsed.netloc) != (origin.scheme, origin.netloc):
                raise ValueError("Refusing to send daemon credentials to another origin")
            path = urllib.parse.urlunsplit(("", "", parsed.path, parsed.query, ""))
        if not path.startswith("/") or path.startswith("//"):
            raise ValueError("Expected a daemon-relative URL")
        request = urllib.request.Request(
            self.base_url + path,
            data=None if body is None else json.dumps(body).encode(),
            method=method,
            headers={"Authorization": "Bearer " + self.token,
                     "Content-Type": "application/json", "Accept": accept},
        )
        return self.opener.open(request, timeout=timeout)

    def request(self, path, method="GET", body=None, timeout=30):
        try:
            with self.open(path, method, body, timeout) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            with error:
                detail = error.read(4096).decode("utf-8", errors="replace")
            raise DaemonHTTPError(method, path, error.code, detail) from error


class DaemonHTTPError(RuntimeError):
    def __init__(self, method, path, status, detail):
        self.status = status
        super().__init__(f"{method} {path}: HTTP {status}: {detail}")


def sse_events(response):
    data = []
    for raw in response:
        line = raw.decode("utf-8").rstrip("\r\n")
        if not line:
            if data:
                yield json.loads("\n".join(data))
                data = []
        elif line.startswith("data:"):
            data.append(line[5:].removeprefix(" "))
    # An incomplete frame is replayed on reconnect, never treated as completion.


def follow_turn(client, turn, output, process, deadline=None):
    cursor = 0
    stream_path = turn["stream_url"]
    if urllib.parse.urlsplit(stream_path).query:
        raise ValueError("Unexpected query in daemon stream URL")
    with (output / "events.jsonl").open("w", encoding="utf-8") as log:
        while True:
            if process.poll() is not None:
                raise RuntimeError("Daemon exited before the turn completed")
            if deadline is not None and time.monotonic() >= deadline:
                raise TimeoutError("Coder turn timed out")
            timeout = 15 if deadline is None else max(0.1, min(15, deadline - time.monotonic()))
            try:
                with client.open(
                    f"{stream_path}?since={cursor}", timeout=timeout,
                    accept="text/event-stream; medousa-version=3",
                ) as response:
                    for envelope in sse_events(response):
                        if deadline is not None and time.monotonic() >= deadline:
                            raise TimeoutError("Coder turn timed out")
                        if envelope["schema_version"] != 3 or str(envelope["turn_id"]) != str(turn["turn_id"]):
                            raise ValueError("Unexpected turn stream identity/version")
                        if envelope["seq"] <= cursor:
                            continue
                        cursor = envelope["seq"]
                        log.write(json.dumps(envelope) + "\n")
                        log.flush()
                        event = envelope["event"]
                        if event["type"] == "turn_completed":
                            return event
                        if event["type"] in {"permission_request", "budget_approval_required", "secret_request"}:
                            # A benchmark must not invent user approvals or extra budget.
                            return {"type": "turn_completed", "outcome": "needs_input",
                                    "aggregate_text": "", "pending_event": event}
            except urllib.error.HTTPError:
                raise
            except (TimeoutError, ConnectionError, urllib.error.URLError):
                pass
            time.sleep(0.2)


def run_coder(client, instruction, workspace, branch, output, process, deadline=None, control_path=None):
    defaults = client.request("/v1/runtime/defaults")
    session = client.request("/v1/sessions", "POST", {"display_name": "Terminal-Bench"})
    session_id = session["session_id"]
    if control_path is not None:
        write_json(control_path, {"base_url": client.base_url, "session_id": session_id})
    item = client.request("/v1/forge/items/start", "POST", {
        "title": "Terminal-Bench", "brief": instruction,
        "repo_path": str(workspace), "base_ref": branch,
        "workspace_mode": "attached_checkout",
    })
    if Path(item["environment"]["worktree"]).resolve() != workspace.resolve():
        raise RuntimeError("Forge workspace differs from the workspace Harbor will score")
    if item["state"].lower() != "ready":
        raise RuntimeError(f"Forge undertaking is not ready: {item['state']}")
    session_path = "/v1/sessions/" + urllib.parse.quote(session_id, safe="")
    client.request(session_path + "/code-binding", "PUT", {"work_id": item["id"]})
    client.request(session_path + "/agent-mode", "PUT", {"mode": "coder", "scope": "session"})
    request = {key: defaults[key] for key in (
        "provider", "model", "response_depth_mode", "reasoning_effort", "stage_routing"
    )}
    request.update({
        "session_id": session_id, "prompt": instruction,
        "agent_mode": "coder", "persist_user_turn": True,
        "code_context": {"work_id": item["id"]},
        "surface": {"channel_surface": "api", "supports_ui_artifacts": False,
                    "supports_liquid_markdown": False, "supports_browser_host": False},
    })
    write_json(output / "request.json", request)
    write_json(output / "undertaking.json", item)
    turn = client.request("/v1/interactive/turn", "POST", request)
    write_json(output / "turn.json", turn)
    try:
        if turn.get("fallback_to_local") or turn.get("stream_ready") is False:
            raise RuntimeError("Daemon did not accept the ordinary interactive turn")
        result = follow_turn(client, turn, output, process, deadline)
        write_json(output / "result.json", result)
        (output / "final.txt").write_text(result["aggregate_text"], encoding="utf-8")
        return result
    finally:
        # Also ends a turn waiting for a human or interrupted by Harbor's timeout.
        try:
            client.request(session_path + "/active-turn", "POST", timeout=5)
        except (OSError, ValueError, DaemonHTTPError):
            pass


def stop_daemon(state, wait=False):
    pid_file = state / "daemon.pid"
    if not pid_file.exists():
        return
    pid = int(pid_file.read_text())
    # On Harbor cancellation, cancel the actual turn before killing its daemon
    # so the normal runtime can interrupt lease-owned tools and shell sessions.
    if wait and (state / "control.json").exists():
        control = json.loads((state / "control.json").read_text())
        secret = json.loads((state / "data" / "credentials" / "medousa-cli.secret").read_text())
        try:
            client = DaemonClient(control["base_url"], secret["token"])
            session_id = urllib.parse.quote(control["session_id"], safe="")
            client.request(f"/v1/sessions/{session_id}/active-turn", "POST", timeout=2)
        except (OSError, ValueError, DaemonHTTPError):
            pass
    try:
        os.killpg(pid, signal.SIGTERM)
        if wait:
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                os.killpg(pid, 0)
                time.sleep(0.1)
            os.killpg(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def run(args):
    workspace = args.workspace.resolve(strict=True)
    daemon = args.daemon.resolve(strict=True)
    instruction = args.instruction.read_text(encoding="utf-8")
    output = args.output.resolve()
    state = args.state.resolve()
    if state.is_relative_to(workspace) or output.is_relative_to(workspace):
        raise ValueError("State and logs must be outside the task workspace")
    output.mkdir(parents=True, exist_ok=False)
    state.mkdir(parents=True, mode=0o700, exist_ok=False)
    branch = prepare_repository(workspace)
    baseline = git(workspace, "rev-parse", "HEAD")
    data = state / "data"
    credentials = data / "credentials"
    credentials.mkdir(parents=True, mode=0o700)
    token = secrets.token_urlsafe(32)
    # Use the daemon's supported owner-only local-client credential bootstrap.
    # Its normal provision_first_party() validates and registers this secret.
    secret_path = credentials / "medousa-cli.secret"
    secret_path.touch(mode=0o600)
    write_json(secret_path, {"credentialId": str(uuid.uuid4()), "token": token})
    config = state / "config"
    config.mkdir()
    (config / ".env").touch()
    defaults = json.loads(args.defaults.read_text()) if args.defaults else {}
    defaults.update(provider=args.provider, model=args.model)
    if args.reasoning_effort is not None:
        defaults["reasoning_effort"] = args.reasoning_effort
    if args.base_url is not None:
        defaults["base_url"] = args.base_url
    write_json(data / "tui_defaults.json", defaults)
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    env = dict(os.environ, MEDOUSA_DATA_DIR=str(data), MEDOUSA_CONFIG_DIR=str(config),
               STASIS_ENV_FILE=str(config / ".env"))
    command = [str(daemon), "--backend", "in-memory", "--bind", f"127.0.0.1:{port}",
               "--provider", args.provider, "--model", args.model]
    if args.base_url:
        command.extend(["--base-url", args.base_url])
    digest = hashlib.sha256()
    with daemon.open("rb") as binary:
        for chunk in iter(lambda: binary.read(1024 * 1024), b""):
            digest.update(chunk)
    write_json(output / "manifest.json", {
        "daemon_sha256": digest.hexdigest(),
        "backend": "in-memory", "workspace": str(workspace), "baseline": baseline,
        "provider": args.provider, "model": args.model, "workspace_mode": "attached_checkout",
        "memory": "fresh daemon data directory per trial",
    })
    client = DaemonClient(f"http://127.0.0.1:{port}", token)
    with (output / "daemon.log").open("w") as log:
        process = subprocess.Popen(command, cwd=config, env=env, stdout=log,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        (state / "daemon.pid").write_text(str(process.pid))
        try:
            startup_deadline = time.monotonic() + 120
            while True:
                if process.poll() is not None:
                    raise RuntimeError("Daemon failed to start; see daemon.log")
                try:
                    client.request("/v1/runtime/defaults", timeout=2)
                    client.request("/v1/forge/items", timeout=2)
                    break
                except DaemonHTTPError as error:
                    if error.status != 503 or time.monotonic() >= startup_deadline:
                        raise
                    time.sleep(0.2)
                except (OSError, ValueError):
                    if time.monotonic() >= startup_deadline:
                        raise TimeoutError("Daemon startup timed out; see daemon.log")
                    time.sleep(0.2)
            deadline = time.monotonic() + args.timeout if args.timeout else None
            return run_coder(client, instruction, workspace, branch, output, process, deadline,
                             control_path=state / "control.json")
        finally:
            stop_daemon(state)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            (state / "daemon.pid").unlink(missing_ok=True)
            # Preserve native inference receipts, but never copy credential stores.
            if (data / "turn_ledger").is_dir():
                shutil.copytree(data / "turn_ledger", output / "turn_ledger")
            for name, git_args in (
                ("workspace-status.txt", ["status", "--short"]),
                ("workspace.patch", ["diff", "--binary", baseline]),
            ):
                with (output / name).open("wb") as snapshot:
                    subprocess.run(["git", "-C", str(workspace), *git_args],
                                   stdout=snapshot, stderr=subprocess.DEVNULL, check=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stop", type=Path, help="Stop the daemon belonging to this trial state directory")
    parser.add_argument("--daemon", type=Path)
    parser.add_argument("--instruction", type=Path)
    parser.add_argument("--workspace", type=Path, default=Path.cwd())
    parser.add_argument("--output", type=Path)
    parser.add_argument("--state", type=Path)
    parser.add_argument("--provider")
    parser.add_argument("--model")
    parser.add_argument("--defaults", type=Path, help="Optional normal tui_defaults.json")
    parser.add_argument("--reasoning-effort")
    parser.add_argument("--base-url")
    parser.add_argument("--timeout", type=float, help="Optional standalone turn timeout in seconds; Harbor owns trial timeouts")
    args = parser.parse_args()
    if args.stop:
        stop_daemon(args.stop, wait=True)
        return
    for name in ("daemon", "instruction", "output", "state", "provider", "model"):
        if not getattr(args, name):
            parser.error(f"--{name} is required")
    def interrupted(*_):
        raise KeyboardInterrupt

    signal.signal(signal.SIGTERM, interrupted)
    try:
        result = run(args)
    except (Exception, KeyboardInterrupt) as error:
        if args.output.is_dir():
            write_json(args.output / "adapter-error.json", {"type": type(error).__name__, "message": str(error)})
        raise
    # Incomplete agent outcomes still leave their files for Harbor's verifier.
    # Infrastructure/protocol failures above raise and are reported as errors.
    print(json.dumps({"outcome": result["outcome"]}))


if __name__ == "__main__":
    main()
