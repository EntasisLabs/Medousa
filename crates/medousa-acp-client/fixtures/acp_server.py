"""Small ACP peer for Rust transport conformance tests; never calls a model."""
import json
import os
import sys
import time

mode = sys.argv[1] if len(sys.argv) > 1 else "normal"
session = "fixture-session"
model = "fixture-one"
prompt_id = None
permission_id = None
permission_stage = 0


def send(frame):
    print(json.dumps({"jsonrpc": "2.0", **frame}), flush=True)


def result(request, value):
    send({"id": request["id"], "result": value})


def config():
    return [{"id": "model", "name": "Model", "category": "model", "type": "select",
             "currentValue": model, "options": [
                 {"value": "fixture-one", "name": "Fixture One"},
                 {"value": "fixture-two", "name": "Fixture Two"}]}]


def update(kind, **fields):
    send({"method": "session/update", "params": {
        "sessionId": session, "update": {"sessionUpdate": kind, **fields}}})


def permission(identifier):
    send({"id": identifier, "method": "session/request_permission", "params": {
        "sessionId": session, "toolCall": {"toolCallId": "shell-call", "title": "Run a shell command"},
        "options": [
            {"optionId": "allow-first", "name": "Allow once", "kind": "allow_once"},
            {"optionId": "allow-second", "name": "Allow for session", "kind": "allow_always"},
            {"optionId": "reject", "name": "Reject", "kind": "reject_once"}]}})


for line in sys.stdin:
    request = json.loads(line)
    with open("acp-requests.jsonl", "a", encoding="utf-8") as log:
        log.write(json.dumps(request) + "\n")
    method = request.get("method")
    params = request.get("params", {})
    if method == "initialize":
        assert params["protocolVersion"] == 1
        assert params["clientCapabilities"]["terminal"] is True
        assert params["clientCapabilities"]["fs"]["readTextFile"] is True
        if mode == "stderr":
            sys.stderr.write("unicode-🌈 diagnostic\n" * 10000)
            sys.stderr.flush()
        caps = {"loadSession": mode != "false-caps",
                "promptCapabilities": {"image": mode != "false-caps", "audio": False},
                "sessionCapabilities": {"resume": False, "fork": False} if mode == "false-caps" else {} if mode == "load-history" else {"resume": {}, "fork": {}}}
        result(request, {"protocolVersion": 1, "agentCapabilities": caps,
                         "agentInfo": {"name": "fixture", "version": "1"}})
    elif method == "session/new":
        assert params["cwd"] == os.getcwd()
        assert params["mcpServers"] == []
        result(request, {"sessionId": session, "configOptions": config()})
    elif method in ("session/resume", "session/load"):
        session = params["sessionId"]
        assert mode != "false-caps", "false capabilities cannot authorize resume"
        if mode == "load-history":
            assert method == "session/load"
            update("user_message_chunk", content={"type": "text", "text": "OLD USER PROMPT"})
            update("agent_message_chunk", content={"type": "text", "text": "OLD ASSISTANT ANSWER"})
            update("agent_thought_chunk", content={"type": "text", "text": "OLD THOUGHT"})
            update("tool_call", toolCallId="historical-tool", title="Old tool", status="completed", rawOutput="OLD OUTPUT")
            model = "fixture-two"
            update("config_option_update", configOptions=config())
            permission_id = "hydration-choice"
            permission_stage = 0
            permission(permission_id)
            result(request, {"sessionId": session})
        else:
            result(request, {"sessionId": session, "configOptions": config()})
    elif method == "session/set_config_option":
        if mode == "slow-config":
            open("config-waiting", "w").close()
            time.sleep(0.7)
        assert params["configId"] == "model"
        model = params["value"]
        update("config_option_update", configOptions=config())
        result(request, {"configOptions": config()})
    elif method == "session/prompt":
        prompt_id = request["id"]
        text = params["prompt"][0]["text"]
        if text == "permissions":
            permission_id = prompt_id  # Deliberate opposite-direction RPC id collision.
            permission_stage = 1
            permission(permission_id)
        elif text == "quiet":
            time.sleep(0.3)
            update("agent_message_chunk", content={"type": "text", "text": "still working"})
            result(request, {"stopReason": "end_turn"})
        elif text in ("eof", "crash"):
            sys.stderr.write("do-not-expose-vendor-secret\n")
            sys.stderr.flush()
            sys.exit(7 if text == "crash" else 0)
        elif text == "malformed":
            print("invalid-json", flush=True)
        elif text == "stall":
            continue
        elif text == "prompt-error":
            send({"id": prompt_id, "error": {"code": -32000, "message": "fixture provider error"}})
        else:
            update("user_message_chunk", content={"type": "text", "text": "CURRENT USER ECHO"})
            update("agent_thought_chunk", content={"type": "text", "text": "thinking"})
            update("tool_call", toolCallId="tool-1", title="Read a file", status="in_progress", rawInput={"path": "example.rs"})
            update("tool_call_update", toolCallId="tool-1", status="completed", rawOutput="read result", content=[{"type": "content", "content": {"type": "text", "text": "read result"}}])
            update("agent_message_chunk", content={"type": "text", "text": "Hello 🌈"})
            result(request, {"stopReason": "end_turn"})
    elif method == "session/cancel":
        sys.exit(0)
    elif method is None and permission_id is not None:
        assert request["id"] == permission_id
        assert type(request["id"]) is type(permission_id), "original JSON-RPC id type must survive"
        assert request["result"]["outcome"] == {"outcome": "selected", "optionId": "allow-second"}
        if permission_stage == 0:
            permission_id = None
        elif permission_stage == 1:
            permission_id = str(prompt_id)
            permission_stage = 2
            permission(permission_id)
        else:
            permission_id = None
            update("agent_message_chunk", content={"type": "text", "text": "permission granted"})
            send({"id": prompt_id, "result": {"stopReason": "end_turn"}})
    else:
        raise AssertionError("unexpected ACP request: " + str(request))
