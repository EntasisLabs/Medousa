# Coding session terminal — architecture

> Status: implemented (M0–M2). The workshop owns PTYs; Home is the window
> manager; agents are peers on shared sessions. Capability, not identity.

Companion to [coding-engine-orchestrator.md](coding-engine-orchestrator.md).
This document covers the attachable Terminal + opt-in coding toolkit.

## Role split

| Owner | Owns | Does not own |
|-------|------|--------------|
| `medousa-session` (workshop sidecar) | One OS PTY per `session_id`, cwd/env, byte fan-out to attaches + agent readers, resize/interrupt, path allowlist (scripts root + Forge worktrees) | VT rendering, tab/split layout, sandbox policy source-of-truth, Forge seals/leases |
| `medousa_daemon` | Lazy spawn + discovery (`/v1/shell-sessions`), HTTP/WS proxy (`/v1/sessions/shell*`), Forge `work_id` → worktree cwd bind, command-log staging | PTY lifetime (sidecar crash isolation) |
| Medousa Home | Window manager (`shellTabs`, `terminal` tab kind), VT parse + input encode in Tauri, attach/detach | Local PTY against a remote workshop (never) |
| Agents (coding domain) | `code_read/search/apply_patch`, `shell_session_run/interrupt/status` via daemon | Default interactive palette (opt-in only) |

Same authority rule as the coding engine: remote Home never opens a local PTY
against a foreign workshop disk.

## Transport

Daemon-proxied to the session host (`medousa-session`, default
`127.0.0.1:7862`):

- `GET /v1/shell-sessions` — discovery + spawn status (mirrors `/v1/coding-engine`)
- `GET /v1/sessions/shell` — list sessions
- `POST /v1/sessions/shell` — create `{ work_id?, cwd? , lease_id? }`
- `WS /v1/sessions/shell/{id}` — frames `{type: stdout|stdin|resize}`; payload
  base64 in text frames, raw bytes in binary frames
- `POST /v1/sessions/shell/{id}/signal` — `{signal: interrupt|kill}`

## Home = window manager (no tmux)

- Tab kind `terminal` in `shellTabs`; opening a second Terminal tab/split is a
  **new session** (or attach-existing) — the host never multiplexes one PTY
  into panes.
- `TerminalPane.svelte` renders the cell grid and forwards keys through Tauri.
- Tauri `terminal.rs` runs the VT parser on a **dedicated thread per tab**
  (parser state is `!Send` by design) and bridges the workshop WS.

### VT parser pivot (Zig constraint)

The plan targeted `libghostty-vt` in Tauri. Its `-sys` crate builds the
Ghostty VT archive from source via the Zig toolchain, which is not available
in this environment and we do not want as a dependency. The shipped parser is
[`vte`](https://crates.io/crates/vte) (pure Rust, Alacritty lineage) driving a
minimal cell grid (cursor, scroll region, CSI/SGR subset — bold tracked).

Consequences:

- Same dedicated-thread + `terminal-frame` IPC shape; `TerminalPane` and the
  WS bridge are parser-agnostic.
- Exotic sequences (kitty graphics, full SGR attribute set, reflow) are not
  emulated in v1.
- `libghostty-vt` remains a drop-in upgrade behind the same feed channel if a
  Zig toolchain (or prebuilt archive) ever becomes acceptable.

## Forge bind

- `POST /v1/sessions/shell { work_id }` → daemon loads the Forge item; cwd =
  provisioned worktree (`environment.worktree`); sessions refuse
  unprovisioned work.
- Optional `lease_id` on create stages a `shell_session_open` line into the
  attempt's `commands.jsonl` via lease-fenced `append_command_log` (review
  evidence). Seal stays explicit Forge HTTP — session exit ≠ seal.
- Session host receives Forge worktree roots as `--allow-root` at spawn; the
  coding tools' read/patch surface is constrained to scripts root + those
  worktrees.

## Coding toolkit gating

Domain `coding` in `tool_bootstrap` (worker lane) — unlocked via
`ensure_coding_domain_for_session(session_id)` when a session surface opts in
(manuscript / Forge `work_id` bind / Settings). Tools registered in
`runtime_services.rs`:

`cognition_code_read`, `cognition_code_search`, `cognition_code_apply_patch`,
`cognition_shell_session_status`, `cognition_shell_session_run`,
`cognition_shell_session_interrupt`.

`cognition_code_read` keeps whole-file reads for model-safe files and returns a
SHA-256 content digest. Oversized whole-file requests return a successful
`orientation_required` observation with size, digest/line metadata when a
bounded scan is safe, and suggested line/byte range calls. Ranged reads return
exact coverage plus a continuation call; payload limits are navigation
boundaries rather than opaque failures. Every
`cognition_code_apply_patch` call must present that digest as
`expected_sha256` (or `missing` when creating a file), so stale observations
fail before mutation. Reads, writes, recursive search, and shell output have
hard payload limits; symlinks cannot be used to escape an allowed root.

One-shot `cognition_shell_run` remains for non-coding probes; coding mode
prefers the shared session. Default interactive palette is unchanged.

## Packages

`shell-session` (binary `medousa-session`) sits next to `coding-engine` in
Settings → Packages. The daemon lazy-spawns the binary from `{dataDir}/bin`,
then a sibling of `current_exe`, then `PATH` (`MEDOUSA_SESSION_BIN` override).

## Non-goals

- tmux / session-internal pane multiplexing
- DAP
- Medousa-default coding system prompt
- Merging OpenShell gateway into this PTY
- Replacing ACP Cursor/Codex executors
