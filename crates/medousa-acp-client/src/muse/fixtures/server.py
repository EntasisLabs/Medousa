"""Hermetic MSP v1 peer. No network, account, model, or installed Muse needed."""
import json
import os
import sys
import uuid

mode = sys.argv[1] if len(sys.argv) > 1 else "normal"
session = None
turn = None
initialized = False
approval = None


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def event(method, **params):
    send({"method": method, "params": {"sessionId": session, **params}})


def result(request, **value):
    if "commandId" in request.get("params", {}):
        value["commandId"] = request["params"]["commandId"]
    send({"id": request["id"], "result": value})


def done(terminal="completed", **extra):
    event("turn/completed", turnId=turn, terminal=terminal, **extra)


def item(kind, identifier, revision=1, status="inProgress", **extra):
    return {"itemId": identifier, "kind": kind, "revision": revision,
            "status": status, "turnId": turn, **extra}


for line in sys.stdin:
    request = json.loads(line)
    with open("msp-requests.jsonl", "a", encoding="utf-8") as log:
        log.write(json.dumps(request) + "\n")
    method = request.get("method")
    params = request.get("params", {})
    if not method:  # presentation receipt from client
        assert request.get("result") == {}
        continue
    if method == "initialize":
        assert params["clientInfo"]["name"] == "medousa"
        if mode == "exit":
            sys.stderr.write("fixture startup failure\n")
            sys.exit(3)
        if mode == "stderr":
            sys.stderr.write("diagnostic " * 10000)
            sys.stderr.flush()
        result(request, schema={"version": 99 if mode == "version" else 1,
                                "fingerprint": "sha256:fixture"},
               sessionDurability="ephemeral" if mode == "ephemeral" else "durable")
        continue
    if method == "initialized":
        initialized = True
        continue
    assert initialized, "client must notify initialized before requests"
    if method != "model/list":
        assert uuid.UUID(params["commandId"]).version == 7
    if method in ("session/start", "session/resume"):
        session = params["sessionId"]
        assert params["permissionProfile"] == ":ask-me"
        if method == "session/start":
            assert params["workspaceRoot"] == os.getcwd()
            assert params["approvalMode"] == "promptUnmatched"
        if mode == "resume-fail" and method == "session/resume":
            send({"id": request["id"], "error": {"code": -32011,
                  "message": "No such session", "data": {"kind": "sessionNotFound"}}})
            continue
        result(request, session={"sessionId": session, "modelId": "muse-1",
                                 "activeTurnId": None}, viewCursor="v4:fixture:1")
    elif method == "model/list":
        assert params["sessionId"] == session
        result(request, models=[
            {"modelId": "muse-1", "displayLabel": "Muse One", "providerId": "meta",
             "variants": ["low", "high"], "defaultReasoningEffort": "low"},
            {"modelId": "muse-2", "displayLabel": "Muse Two", "providerId": "meta",
             "variants": ["medium", "ultra"], "defaultReasoningEffort": "medium"},
        ])
    elif method == "session/setApprovalMode":
        assert params["mode"] == "promptUnmatched"
        result(request, status="accepted")
    elif method == "session/setModel":
        assert params["model"]["modelId"] == "muse-2"
        result(request, status="accepted")
    elif method == "turn/start":
        assert params["sessionId"] == session
        assert params["workspaceRoots"] == [os.getcwd()]
        turn = params["commandId"]
        text = params["input"][0]["text"]
        result(request, turnId=turn, status="accepted", disposition="started", startedNewTurn=True)
        if text == "malformed":
            print("not json", flush=True)
        elif text == "stall":
            continue
        elif text == "hold":
            approval = {"approvalId": "approval-1", "sessionId": session, "turnId": turn,
                        "currentRequirementId": {"approvalId": "approval-1", "sourceIndex": 0},
                        "toolName": "shell", "subject": {"kind": "shell", "command": "echo yes"},
                        "availableChoices": [
                            {"choiceId": "once", "label": "Allow once", "decision": "approved", "scope": "once"},
                            {"choiceId": "session", "label": "Allow for session", "decision": "approvedForSession", "scope": "session"},
                            {"choiceId": "reject", "label": "Reject", "decision": "abort", "scope": "once"}]}
            send({"id": "present-1", "method": "approval/request", "params": approval})
            event("approval/requested", **{key: value for key, value in approval.items() if key != "sessionId"})
        else:
            event("item/started", item=item("agentMessage", "message"))
            event("item/delta", itemId="message", delta="Hello ")
            event("item/delta", itemId="message", delta="🌈")
            event("item/completed", item=item("agentMessage", "message", 2, "completed", text="Hello 🌈"))
            event("item/started", item=item("toolCall", "tool", tool="shell", args='{"command":"echo yes"}'))
            event("item/completed", item=item("toolCall", "tool", 2, "completed", tool="shell", args='{"command":"echo yes"}', visibleOutput="yes", exitCode=0))
            if text == "fail":
                done("failed", error={"message": "Provider quota exhausted"})
            else:
                done()
    elif method == "approval/decide":
        assert params["requirementId"] == approval["currentRequirementId"]
        assert params["choiceId"] in ["once", "session", "reject"]
        result(request, approvalId=approval["approvalId"], status="accepted", terminal=True)
        event("approval/resolved", approvalId=approval["approvalId"])
        done()
    elif method == "turn/interrupt":
        assert params["turnId"] == turn
        result(request, status="accepted", turnId=turn)
        done("cancelled")
    else:
        raise AssertionError("unexpected method: " + str(method))
