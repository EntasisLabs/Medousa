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
- `GET /v1/code/language-sessions`
- `GET /v1/code/language-matrix`
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
`Cargo.toml`, `package.json`, `svelte.config.js`, `go.mod`, or `pyproject.toml`
— becomes the language-server root. No marker above the Forge working copy can
participate.

`GET /v1/code/language-root?work_id=…&uri=…&language=…` reports that resolved
root as a file URI and project-relative path. Home uses it as the LSP `rootUri`
and pooling identity, while the daemon independently forwards the active
document and authoritative project root to the coding engine. The coding engine
revalidates both and rewrites initialize root fields before launching the server
in that directory. Nested monorepo packages therefore get distinct sessions;
files under the same language root reuse one Home client. With an older coding
engine that lacks the discovery route, Home explicitly falls back to the whole
project root for rolling-upgrade compatibility.

## Session lifecycle and configuration

Every editor and agent language process has a bounded workshop-side lifecycle
record with its project root, resolved language root, phase, recent work-done
progress, LSP messages, and captured process stderr. Home reads the matching
history through
`GET /v1/code/language-sessions?work_id=…&uri=…&language=…`; the daemon again
derives the project path from `work_id`, and the coding engine revalidates the
document before selecting records. Logs are memory-bounded diagnostic history,
not an unbounded project file.

The editor WebSocket terminates when the underlying language server exits or
its protocol stream fails. Home removes the dead client, keeps the source
buffer editable, and retries at 250 ms, 750 ms, and 1.5 seconds. A visible
degraded banner then retains **Restart**, **Logs**, and package **Repair**
actions. Manual restart replaces only the matching project/language-root client;
it does not close the project or another nested package's server.

On the editor channel the coding engine rewrites `initialize` to advertise
workspace configuration, workspace folders, and work-done progress, then
answers those common server-to-client requests itself, supplies bounded
first-party settings, and sends `workspace/didChangeConfiguration` after
initialization. This keeps servers that require configuration functional even
though the embedded CodeMirror LSP client does not implement arbitrary server
requests. Project/user settings are added later through the versioned
contribution contract.

## Language matrix

`GET /v1/code/language-matrix` returns every registered workshop language with
its command, file extensions, root markers, optional package id, and a
`binary_available` / `usable` probe against `{dataDir}/bin` and `PATH`. Home
uses this before treating a language as supported and for **Repair language
support**, which installs `coding-engine` plus the row's exact `package_id`
when one exists. Registry membership alone never means the language is usable.

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
refreshes while visible and reconciles when resumable
`GET /v1/forge/items/{work_id}/project-events` reports source changes. Home
subscribes with `?since=` replay, updates every open buffer (dirty drafts keep
compare/rebase), recovers rename/delete, and notifies the language server via
`workspace/didChangeWatchedFiles`.

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

## Language dogfood pack

Home grammar and workshop language servers stay paired for the first-party
dogfood languages:

| Editor language | Grammar | Workshop server | Package |
|---|---|---|---|
| TypeScript / TSX | CodeMirror JS with `typescript` / `jsx` | `typescript-language-server` | `langservers` |
| JavaScript / JSX | CodeMirror JS with optional `jsx` | `typescript-language-server` | `langservers` |
| Svelte | `codemirror-lang-svelte` | `svelteserver` | `langservers` |
| Python | CodeMirror Python | `pyright-langserver` | `langservers` |

Every other registered LSP language also has an editor grammar (official
CodeMirror packs for Go/C++/Java/PHP, legacy stream modes for C#/Kotlin/Ruby/
Lua/Swift). Those servers still resolve from `{dataDir}/bin` or `PATH`; they do
not yet ship as Medousa package ids, so Repair explains the missing binary
instead of installing an unrelated pack.

`.svelte` files resolve a language root from `svelte.config.*` or the nearest
`package.json` inside the governed worktree. Editor **Repair language support**
installs `coding-engine` plus the language's package (`langservers` for this
dogfood set) on a co-located workshop. Remote Home still opens Settings →
Packages on the connected workshop instead of installing client-side binaries.

## Degradation

If `medousa-code` or a language server is unavailable, plain editing continues.
Home shows starting, progress, reconnecting, and failed states rather than
leaving a dead smart-editing client attached. Recent language output remains
available from the editor menu for crash diagnosis.
Co-located Home can install the optional `coding-engine` and `langservers`
packages. Remote Home never installs binaries on the client while implying
that it repaired the workshop.
