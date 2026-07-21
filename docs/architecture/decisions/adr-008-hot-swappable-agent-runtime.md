# ADR-008: Hot-swappable agentic runtime (bones in 0.4.0)

## Status

Accepted

## Context

Medousa Home already has a strong vault, context, and native turn loop. Users who love that **space** may still want a different **agentic runtime** for a given chat (Cursor for coding, Codex, later others) — without abandoning Medousa’s workshop.

We must not marry the product to a single model **or** a single agent loop. The durable constant is Medousa space; the loop is swappable.

## Decision

1. **North star:** Home chat is a client over a pluggable agentic runtime. External runtimes reach vault/context/calendar/artifacts via a first-party **Medousa MCP server**. Home drives external loops via an **ACP client**.
2. **0.4.0 ships bones only:**
   - `crates/medousa-mcp-server` — stdio MCP surface for space tools; **deny** spawn/turn/host orchestration
   - `crates/medousa-acp-client` — traits + Cursor/Codex config stubs + stub session
   - Prove the pipes; do **not** require polished “Runtime: Medousa | Cursor | Codex” composer UX
3. **QA bar for bones:** Cursor + Codex. Claude Code / Devin are watch-only.
4. **Later (not 0.4.0):** per-session runtime picker, UX parity across loops, per-workshop defaults, Medousa-as-ACP-agent.

```text
User → Home chat → [Medousa loop | Cursor/Codex via ACP]
                              ↓
                    Medousa MCP server → vault / context / calendar
```

## Consequences

- Native Medousa turns remain the default path; ACP is opt-in once wired.
- External agents see markdown literals for workbook formulas (overlay-only); an evaluate API may come later.
- Gateway (`medousa_mcp_gateway`) stays the MCP **client** broker for third-party servers; `medousa_mcp_server` is the inverse — Medousa as a server.

## Code anchors

- `crates/medousa-mcp-server/`
- `crates/medousa-acp-client/`
- `src/mcp_gateway/` (existing client gateway)
- `apps/medousa-home/src/lib/components/chat/ChatPanel.svelte` (future thin runtime branch)
- `docs/cookbook/mcp-server-setup.md` (operator docs — follow-up)
- `docs/cookbook/acp-external-agents.md` (operator docs — follow-up)
