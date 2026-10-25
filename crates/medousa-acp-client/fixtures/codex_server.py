"""Hermetic native app-server peer; never connects to an account or model."""
import json
import os
import sys
import time

mode = sys.argv[1]
assert sys.argv[2:] == [], "handshake must not override account/login policy"
assert all(key not in os.environ for key in ("OPENAI_API_KEY", "CODEX_API_KEY", "MODEL_PROVIDER"))
initialized = False
thread = "fixture-thread"
turn = None
number = 0
waiting = None


def send(value):
    print(json.dumps(value), flush=True)


def reply(request, result):
    send({"id": request["id"], "result": result})


def event(method, **params):
    send({"method": method, "params": {"threadId": thread, "turnId": turn, **params}})


def finish(status="completed"):
    event("turn/completed", turn={"id": turn, "status": status, "error": {"message": "fixture failure"} if status == "failed" else None})


def output():
    send({"method": "item/agentMessage/delta", "params": {"threadId": thread, "turnId": "retired-turn", "itemId": "stale", "delta": "must not leak"}})
    event("item/reasoning/summaryTextDelta", delta="thinking")
    event("item/started", item={"id": "command", "type": "commandExecution", "status": "inProgress", "command": "pwd"})
    event("item/completed", item={"id": "command", "type": "commandExecution", "status": "completed", "aggregatedOutput": "/fixture"})
    event("item/agentMessage/delta", itemId="message", delta=f"answer {number}")
    event("item/completed", item={"id": "message", "type": "agentMessage", "text": f"answer {number}"})
    finish("failed" if mode == "failed" else "completed")


for line in sys.stdin:
    request = json.loads(line)
    with open("codex-requests.jsonl", "a") as log:
        log.write(json.dumps(request) + "\n")
    assert "jsonrpc" not in request
    method = request.get("method")
    params = request.get("params", {})
    if not method:
        assert waiting is not None and type(request["id"]) is type(waiting) and request["id"] == waiting
        assert request["result"]["decision"] == "acceptForSession"
        waiting = None
        output()
    elif method == "initialize":
        assert params["clientInfo"]["name"] == "medousa"
        reply(request, {"userAgent": "fixture"})
    elif method == "initialized":
        initialized = True
    elif method == "account/read":
        assert initialized and params == {"refreshToken": False}
        reply(request, {"account": None if mode == "signed-out" else {"type": "apiKey" if mode == "api-key" else "chatgpt"}, "requiresOpenaiAuth": True})
    elif method == "model/list":
        assert initialized
        second = params.get("cursor") == "second-page"
        model = "fixture-two" if second else "fixture-one"
        reply(request, {"data": [{"id": "id-" + model, "model": model, "displayName": model, "hidden": False, "supportedReasoningEfforts": [{"reasoningEffort": "low", "description": "Fast"}, {"reasoningEffort": "medium", "description": "Balanced"}], "defaultReasoningEffort": "low"}], "nextCursor": None if second else "second-page"})
    elif method in ("thread/start", "thread/resume"):
        assert "approvalPolicy" not in params and "sandbox" not in params
        assert params["cwd"] == os.getcwd()
        thread = params.get("threadId", thread)
        turns = [{"id": "old-turn", "status": "completed", "items": [{"type": "agentMessage", "text": "do not replay"}]}]
        if mode == "active-resume":
            turns.append({"id": "running-turn", "status": "inProgress"})
        reply(request, {"thread": {"id": thread, "turns": turns}, "model": "fixture-one", "modelProvider": "openai", "reasoningEffort": "medium"})
    elif method == "turn/start":
        assert params["threadId"] == thread
        number += 1
        turn = f"turn-{number}"
        reply(request, {"turn": {"id": turn, "status": "inProgress"}})
        event("turn/started", turn={"id": turn, "status": "inProgress"})
        if mode == "crash":
            sys.stderr.write("x" * 100000)
            sys.stderr.flush()
            sys.exit(1)
        elif mode == "unsupported":
            send({"id": "question", "method": "item/tool/requestUserInput", "params": {"threadId": thread, "turnId": turn, "questions": []}})
            # Client must explicitly reject, not invent an Allow/Deny answer.
            response = json.loads(sys.stdin.readline())
            assert response["id"] == "question" and "error" in response
        elif mode in ("approval-number", "approval-string"):
            waiting = request["id"] if mode == "approval-number" else "approval-request"
            send({"id": waiting, "method": "item/commandExecution/requestApproval", "params": {"threadId": thread, "turnId": turn, "itemId": "command", "command": "pwd", "availableDecisions": ["decline", "acceptForSession", "cancel"]}})
        elif mode == "hold":
            pass
        elif mode == "malformed":
            print("not JSON", flush=True)
        elif mode == "oversize":
            print("x" * (10 * 1024 * 1024 + 1), flush=True)
        else:
            if mode == "delayed":
                time.sleep(0.4)
            output()
    elif method == "turn/interrupt":
        assert params == {"threadId": thread, "turnId": turn}
        reply(request, {})
        finish("interrupted")
    else:
        raise AssertionError("Unexpected method: " + method)
