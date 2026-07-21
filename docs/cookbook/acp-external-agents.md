# ACP external agents (bones)

**Audience:** engineers wiring Home chat to Cursor / Codex

Home chat will talk to an external **agentic runtime** via ACP while that runtime reaches Medousa space via the [MCP server](mcp-server-setup.md). Full runtime-picker UX is later; 0.4.0 ships traits + stubs.

## Crate

`crates/medousa-acp-client`

- `AgentRuntimeKind`: `medousa` | `cursor` | `codex`
- `AcpClient` trait: `create_session` / `prompt` / `cancel` / `next_event`
- `StubAcpClient` for wiring tests without Cursor/Codex installed
- Default commands: `agent acp` (Cursor), `codex acp` (Codex)

## Cut line

| In 0.4.0 bones | Later |
|----------------|--------|
| Trait + config + stub session | Composer “Runtime: …” picker |
| Cursor + Codex QA targets | UX parity with native turns |
| | Per-workshop default runtime |

## Related

- [ADR-008](../architecture/decisions/adr-008-hot-swappable-agent-runtime.md)
- Native turns remain the default Home path until ACP is wired through the daemon.