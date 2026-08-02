# ADR-012 — Medousa Anywhere surface taxonomy

> **Status:** Accepted  
> **Date:** 2026-08-02  
> **Scope:** External surfaces, host integrations, and agent adapters

## Context

Medousa is expanding beyond Home. The product promise is not that every host
must reproduce Home's UI; it is that wherever Medousa appears, the interaction
should carry Home's care, continuity, trust, and recovery behavior.

The hosts do not all have the same relationship to Medousa:

- VS Code, Neovim, and Obsidian have a local editor or vault surface where a
  plugin can provide native context, commands, and reversible editing.
- Notion, Slack, and similar products can host Medousa as an agent participant.
  They should not receive a second copy of Home or a generic embedded chat
  client.

Notion's platform also distinguishes the direction of its agent products. The
Notion Agent SDK brings Notion agents into another application. Medousa's
desired direction is the reverse: Medousa should appear inside Notion as an
external agent. The Notion External Agents API is the relevant boundary; its
availability and exact contract are beta-gated and must be verified before we
commit to implementation details.

## Decisions

### 1. Classify surfaces by host relationship

We use two implementation families:

| Family | Current members | Responsibility |
|---|---|---|
| Native host surfaces | VS Code, Neovim, Obsidian | Run inside the host, capture host-native context, present Medousa actions, and offer host-native reversible edits. |
| External-agent adapters | Notion, Slack, and future channels | Translate host messages, assignments, context, approvals, progress, and results into Medousa sessions and back into the host. |

“Integration” remains the broad product term. “Native surface” and
“external-agent adapter” describe the architecture and runtime boundary.

### 2. Keep the daemon as Medousa authority

Every surface uses the existing `medousa_daemon` runtime for identity, sessions,
tools, policy, budgets, permissions, vault authority, and streaming. A host
adapter must not embed inference, duplicate the turn loop, or create a second
filesystem/vault authority.

Home remains the richest surface and the destination for workflows that do not
belong in a host.

### 3. Treat Notion as a hosted Medousa agent

Notion is not planned as a Notion browser inside Medousa and not as a clone of
the VS Code sidebar. The target interaction is:

```text
Notion mention or assignment
  → page/block/task context
  → Medousa session in the daemon
  → progress, questions, approvals, and proposals
  → approved Notion block/page changes
```

The first Notion implementation is therefore a server-side or connection
adapter around the External Agents API. It must preserve a durable mapping
between Notion sessions/tasks and Medousa sessions/turns, support idempotent
event delivery, and make proposed writes reviewable before applying them.

The Notion Agent SDK is an adjacent future option only if Medousa later wants
to host Notion agents in Home or another Medousa surface. It is not the primary
SDK for bringing Medousa into Notion.

### 4. Make Obsidian a vault-native plugin

Obsidian is the next native surface. The plugin should feel like an extension
of note-making rather than a separate chat product:

- current note, selection, links, and bounded note content become explicit
  context;
- the active Medousa conversation continues across note changes and reloads;
- search, backlinks, synthesis, note creation, append, and link insertion are
  visible, previewable actions;
- direct note writes remain explicit and reversible;
- Obsidian's normal editing remains usable and Medousa does not silently
  compete with it for filesystem authority.

The first slice is intentionally read-and-chat focused. Mutation workflows are
added only after context capture, session continuity, and conflict behavior are
proven.

## Consequences

### Positive

- VS Code, Neovim, and Obsidian can share host-context and editing primitives
  without forcing Notion and Slack into an IDE-plugin model.
- Notion can become a first-class Medousa work surface while Medousa keeps its
  own runtime and trust model.
- A future Slack-like adapter can reuse the external-agent lifecycle instead
  of introducing another chat client architecture.
- Home remains the complete product surface rather than an implementation
  dependency every host must reproduce.

### Tradeoffs

- Notion requires a separate beta-contract and event-delivery spike before
  production work can begin.
- Native plugins need host-specific UX and conflict handling; they cannot be
  reduced to a single generic chat widget.
- The shared client must stay small and daemon-contract-driven while adapters
  own their host-specific presentation.

## Implementation anchors

- Anywhere plan: [`architecture/medousa-anywhere-plan.md`](../../../architecture/medousa-anywhere-plan.md)
- Shared client: [`packages/medousa-client/`](../../../packages/medousa-client/)
- VS Code native surface: [`integrations/vscode/`](../../../integrations/vscode/)
- Neovim native surface: [`integrations/neovim/`](../../../integrations/neovim/)
- Obsidian native surface: [`integrations/obsidian/`](../../../integrations/obsidian/)
- Daemon HTTP contract: [`docs/engine/http-api.md`](../../engine/http-api.md)
