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

Language servers propose edits; Forge performs them. Home normalizes the full
LSP `WorkspaceEdit` — ordered text edits plus create, rename, and delete
resource operations — and shows the complete before/after refactor before the
user applies it. Application uses
`PUT /v1/forge/items/{work_id}/source/workspace-edit` with the active lease and
an explicit digest-or-absence precondition for every touched path. The daemon
validates the whole operation sequence before mutation, applies it in order,
and restores every original path if an I/O or response failure occurs.

For rolling upgrades, Home falls back to the older digest-fenced
`PUT …/source/batch` contract only when the proposal contains text writes and
the connected daemon does not expose `source/workspace-edit`. Resource edits
remain unapplied with an explicit daemon-upgrade message; Home never splits an
atomic refactor across the older create/rename/delete endpoints.

## Degradation

If `medousa-code` or a language server is unavailable, plain editing continues.
Co-located Home can install the optional `coding-engine` and `langservers`
packages. Remote Home never installs binaries on the client while implying
that it repaired the workshop.
