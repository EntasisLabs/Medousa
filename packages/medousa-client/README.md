# `@medousa/client`

The shared TypeScript client for Medousa external surfaces.

This package is the Phase 0/1 integration boundary for the VS Code, Neovim,
Obsidian, and browser adapters. It is intentionally dependency-free and uses the host's
`fetch` implementation so it can run in Node, extension hosts, and embedded
JavaScript runtimes.

## Current slice

- `health()` and `capabilities()` probes
- session listing, history, naming, and deletion
- workshop vault search, read, create, update, and backlinks
- interactive turn start and cancellation
- streaming SSE with sequence deduplication and bounded reconnect
- explicit worker/workshop handoff detection so host composers can release while
  the durable workshop result is followed separately
- bounded host-context helpers that produce typed `host_context` request data;
  visible prompts are never rewritten with context wrappers
- bounded browser page title, URL, selection, and page-text context helpers
- registered client-tool helpers for host-owned capabilities: advertise tool
  definitions, long-poll daemon requests, and return tool results
- generated daemon request/response types from
  `sdk-contract/medousa-types.schema.json`

The client does not store credentials. Host adapters provide a bearer token at
construction time and own secure persistence.

## Registered client tools

An integration can register tools that execute in its native runtime while the
daemon remains the owner of the agent turn. The daemon exposes the definitions
only to turns whose `surface.channel_surface` matches the registration, then
queues each model invocation for the client to pull:

```ts
await client.registerClient({
  client_id: "browser-…",
  channel_surface: "browser",
  supports_browser_host: false,
  tools: [{
    name: "browser_page_snapshot",
    description: "Read the active tab",
    input_schema: { type: "object" },
    effect_class: "external_read",
  }],
});

const request = await client.nextClientToolRequest("browser-…");
if (request) {
  await client.completeClientToolRequest("browser-…", request.request_id, {
    output: { title: "Example", text: "…" },
  });
}
```

Hosts should keep polling while their surface is available and return either
`output` or `error` for every request. The daemon applies a bounded request
timeout; client tool names must be unique within a registration and are
validated before they become model-visible. The current bridge requires every
registered tool to declare `effect_class: "external_read"`; write and
side-effecting classes will be enabled only with an approval flow.

When a turn emits `worker_ack` or `workshop_ack`, the event is intentionally
non-terminal: the host turn has handed work to a background lane. Surfaces that
need to release their composer immediately can pass
`{ stopOnHandoff: true }` to `streamTurn`, then follow the same stream or poll
session history for the later synthesis.

Conversation surfaces can manage the shared catalog and promote settled replies
without reaching around the daemon:

```ts
await client.renameSession(sessionId, "Compiler investigation");
await client.searchVault("compiler investigation");
const current = await client.getVaultNote("inbox/compiler-investigation.md");
await client.updateVaultNote(
  "inbox/compiler-investigation.md",
  `${current.content}\n\nNew finding\n`,
  current.note.content_hash,
);
await client.createVaultNote({
  path: "inbox/compiler-investigation.md",
  content: "# Compiler investigation\n\n…",
  session_id: sessionId,
  semantic_tags: ["chat-turn"],
});
await client.deleteSession(sessionId, true);
```

`updateVaultNote` sends raw Markdown and an optional `If-Match` content hash;
the daemon returns `412 Precondition Failed` when the note changed since it was
read. Hosts should surface that as a refresh/review action rather than retrying
over the user's edit.

## Local development

```bash
npm run generate:types
npm run build
```

The package is not yet published and is not a replacement for the Rust or
Python SDKs. It is the shared host integration layer for JavaScript/TypeScript
surfaces.
