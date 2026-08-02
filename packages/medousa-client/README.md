# `@medousa/client`

The shared TypeScript client for Medousa external surfaces.

This package is the Phase 0/1 integration boundary for the VS Code, Neovim,
and Obsidian adapters. It is intentionally dependency-free and uses the host's
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
- bounded host context helpers
- generated daemon request/response types from
  `sdk-contract/medousa-types.schema.json`

The client does not store credentials. Host adapters provide a bearer token at
construction time and own secure persistence.

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
