# LSP Interoperability Orchestrator + Detamu bridge

## Role split

| Layer | Owns |
|-------|------|
| Medousa Home (CM6) | Buffer, chrome, find/replace, prefs, outline/problems |
| `medousa-code` | Many language servers ↔ many clients; document versions; diagnostics fan-out |
| `medousa_daemon` | Custody, Packages, discovery, cognition tool proxies, Forge |
| Detamu (later) | Versioned world model / `code_avec` — not the keystroke loop |

Home never spawns language servers against a foreign workshop disk. The daemon
advertises `/v1/coding-engine` and proxies `/v1/code/lsp` to the co-located
orchestrator.

## Forge roots

On spawn, the daemon passes:

- `--workspace` = Grapheme scripts library root
- `--allow-root` for each Forge worktree under `{dataDir}/forge/worktrees/…`

Undertaking-bound agent operations use an Orchestrator session pool keyed by
`(governed_project_root, resolved_language_root, language)`. Transparent Home
editor channels use the same bounded root resolution; Home pools those channels
by the returned language-root URI so files in one nested package reuse a server.

Each editor and agent session also has a bounded lifecycle record. The
orchestrator captures process stderr, LSP log/show-message notifications,
work-done progress, resolved roots, and terminal state. Home receives live
progress on the editor channel and can inspect recent workshop-side records via
the daemon without gaining direct process or filesystem authority. Editor
`initialize` is rewritten to advertise configuration, workspace folders, and
work-done progress; common server-to-client requests for those capabilities
plus dynamic registration are answered at this boundary because the CodeMirror
client does not implement them.

## Detamu bridge (M5 hooks)

`medousa-code` exposes:

- `GET /v1/detamu/snapshot` — open document URIs + versions
- `GET /v1/detamu/handles` — opaque server-session handles

Rust types live in `crates/medousa-code/src/detamu.rs`
(`DetamuDocumentSnapshot`, `DetamuServerHandle`, `DetamuObserver`).

Detamu may observe or ingest; it must not own `rust-analyzer` or sit in the
editor hot path. Any score surface in Medousa APIs uses **`code_avec`**, never
bare `avec`.

## Packages

| Package id | Binaries |
|------------|----------|
| `coding-engine` | `medousa-code` → `{dataDir}/bin` |
| `langservers` | `pyright-langserver`, `typescript-language-server`, `svelteserver` |

Install from Settings → Packages. Orchestrator resolves stdio servers from
`{dataDir}/bin` first, then `PATH`.
