# `@medousa/client`

The shared TypeScript client for Medousa external surfaces.

This package is the Phase 0/1 integration boundary for the VS Code, Neovim,
and Obsidian adapters. It is intentionally dependency-free and uses the host's
`fetch` implementation so it can run in Node, extension hosts, and embedded
JavaScript runtimes.

## Current slice

- `health()` and `capabilities()` probes
- session listing, history, naming, and deletion
- workshop vault note creation
- interactive turn start and cancellation
- streaming SSE with sequence deduplication and bounded reconnect
- bounded host context helpers
- generated daemon request/response types from
  `sdk-contract/medousa-types.schema.json`

The client does not store credentials. Host adapters provide a bearer token at
construction time and own secure persistence.

Conversation surfaces can manage the shared catalog and promote settled replies
without reaching around the daemon:

```ts
await client.renameSession(sessionId, "Compiler investigation");
await client.createVaultNote({
  path: "inbox/compiler-investigation.md",
  content: "# Compiler investigation\n\n…",
  session_id: sessionId,
  semantic_tags: ["chat-turn"],
});
await client.deleteSession(sessionId, true);
```

## Local development

```bash
npm run generate:types
npm run build
```

The package is not yet published and is not a replacement for the Rust or
Python SDKs. It is the shared host integration layer for JavaScript/TypeScript
surfaces.
