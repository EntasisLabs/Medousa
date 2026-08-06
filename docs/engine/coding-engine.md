# Coding engine integration

`medousa-code` is Medousa's language-provider boundary. Home's editor uses a
transparent project-language channel, while HTTP agent operations use pooled
project-language sessions. Both run on the workshop. Home and agents reach them
through the daemon, so repository paths and Forge working copies always resolve
on the workshop machine.

## Daemon routes

The daemon exposes `/v1/code/lsp` as a WebSocket and proxies the following HTTP
routes:

- `GET /v1/code/capabilities`
- `GET /v1/code/diagnostics` and `/v1/code/workspace-diagnostics`
- `GET /v1/code/symbols` and `/v1/code/workspace-symbols`
- `GET /v1/code/conventions`
- `GET /v1/code/language-root`
- `GET /v1/code/hover` and `/v1/code/definition`
- `POST /v1/code/request`

Every route accepts a Forge `work_id`. The daemon resolves that identifier to
the governed working copy and replaces it with an internal `workspace_root`;
clients must not send workshop paths as authority.

`POST /v1/code/request` accepts the whitelisted actions `references`, `rename`,
`format`, `code_actions`, and `organize_imports`. Results remain native LSP
values so the caller can preserve provider-specific detail. Home checks the
initialize capabilities before revealing an action.

## Project and language roots

The daemon resolves `work_id` to the canonical governed working copy; that is
the outer authority boundary. For a document-aware request, `medousa-code`
decodes and canonicalizes the file URI, rejects other schemes, encoded path
separators, symlink escapes, and paths outside that working copy, then walks
upward only as far as the project root. The closest registered marker — such as
`Cargo.toml`, `package.json`, `go.mod`, or `pyproject.toml` — becomes the
language-server root. No marker above the Forge working copy can participate.

`GET /v1/code/language-root?work_id=…&uri=…&language=…` reports that resolved
root as a file URI and project-relative path. Home uses it as the LSP `rootUri`
and pooling identity, while the daemon independently forwards the active
document and authoritative project root to the coding engine. The coding engine
revalidates both and rewrites initialize root fields before launching the server
in that directory. Nested monorepo packages therefore get distinct sessions;
files under the same language root reuse one Home client. With an older coding
engine that lacks the discovery route, Home explicitly falls back to the whole
project root for rolling-upgrade compatibility.

## Workspace diagnostics

`GET /v1/code/workspace-diagnostics?work_id=…` returns the latest diagnostics
known to every active editor and agent language session for that governed
working copy. The response includes `scope: "active_sessions"`, the contributing
language ids, and documents with their URI, language, optional version, and
complete LSP diagnostic payload. An empty aggregate request does not start a
placeholder language server.

Supplying `language=…` preserves the earlier per-language behavior and may
initialize that language's pooled agent session. This is also the rolling-
upgrade fallback used by Home when an older coding engine does not advertise
the aggregate scope. Home's Problems panel groups the result by project file,
filters by severity or text, and can open an unopened diagnostic target. It
refreshes while visible until the resumable project event stream replaces
polling.

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
