# Coding engine integration

`medousa-code` is Medousa's language-provider boundary. It owns one language
server session per workshop repository and language. Home and agents reach it
through the daemon, so repository paths and Forge working copies always resolve
on the workshop machine.

## Daemon routes

The daemon exposes `/v1/code/lsp` as a WebSocket and proxies the following HTTP
routes:

- `GET /v1/code/capabilities`
- `GET /v1/code/diagnostics` and `/v1/code/workspace-diagnostics`
- `GET /v1/code/symbols` and `/v1/code/workspace-symbols`
- `GET /v1/code/conventions`
- `GET /v1/code/hover` and `/v1/code/definition`
- `POST /v1/code/request`

Every route accepts a Forge `work_id`. The daemon resolves that identifier to
the governed working copy and replaces it with an internal `workspace_root`;
clients must not send workshop paths as authority.

`POST /v1/code/request` accepts the whitelisted actions `references`, `rename`,
`format`, `code_actions`, and `organize_imports`. Results remain native LSP
values so the caller can preserve provider-specific detail. Home checks the
initialize capabilities before revealing an action.

## Safe edits

Language servers propose edits; Forge performs them. Multi-file workspace edits
use `PUT /v1/forge/items/{work_id}/source/batch` with the active lease and an
expected digest for every file. The daemon validates every path and digest
before writing, uses atomic file replacement, and rolls back earlier writes if
a later replacement fails.

## Degradation

If `medousa-code` or a language server is unavailable, plain editing continues.
Co-located Home can install the optional `coding-engine` and `langservers`
packages. Remote Home never installs binaries on the client while implying
that it repaired the workshop.
