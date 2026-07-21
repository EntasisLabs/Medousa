# ACP external agents (bones)

**Audience:** engineers wiring channels to Cursor / Codex via the SDK + daemon

Any Medousa **channel** talks to the daemon **through the Medousa SDK** (`client.agents()`). The daemon owns agent runtimes — Medousa-native (`/v1/turns`) or external ACP (`/v1/agents`). External agents reach Medousa space via the [MCP server](mcp-server-setup.md).

See [ADR-008](../architecture/decisions/adr-008-hot-swappable-agent-runtime.md).

## SDK

```rust
let runtimes = client.agents().list_runtimes().await?;
let session = client
    .agents()
    .create_session(&CreateAgentSessionRequest {
        session_id: "…".into(),
        runtime: "cursor".into(),
        prompt: Some("hello".into()),
        cwd: None,
        command: None,
        args: None,
        surface: None,
    })
    .await?;
// SSE: client.agents().stream(session.stream_url)
```

Python: `client.agents().list_runtimes()` / `create_session(...)`.

Home: Tauri commands → `client().agents()` only (`daemon/agents.rs`).

## Daemon routes

| Method | Path |
|--------|------|
| GET | `/v1/agents/runtimes` |
| POST | `/v1/agents/sessions` |
| POST | `/v1/agents/sessions/{id}/prompt` |
| GET | `/v1/agents/sessions/{id}/stream` |
| POST | `/v1/agents/sessions/{id}/cancel` |
| GET | `/v1/agents/permission-requests` |
| POST | `/v1/agents/permission-requests/{id}/approve\|deny` |

## ACP crate

`crates/medousa-acp-client` — `ExternalAcpClient` spawns Cursor/Codex when the binary is on PATH; otherwise stub bridge events. Force stub: `MEDOUSA_ACP_FORCE_STUB=1`. Demo permissions: `MEDOUSA_ACP_STUB_PERMISSION=1`.

## Cut line

| In 0.4.0 bones | Later |
|----------------|--------|
| SDK + daemon + thin Home Runtime select | Polished pickers on every channel |
| Cursor + Codex spawn/stub | Full ACP wire parity |
| Permission approve/deny | UX parity with native tool cards |
