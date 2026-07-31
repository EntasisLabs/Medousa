# Medousa Anywhere — external surfaces plan

> **Status:** Proposed / living plan  
> **Date:** 2026-07-31  
> **Owner:** Medousa platform  
> **Related:** [`architecture/ROADMAP.md`](ROADMAP.md), [`docs/sdk/README.md`](../docs/sdk/README.md), [`docs/engine/http-api.md`](../docs/engine/http-api.md), [`docs/cookbook/integrate-without-the-app.md`](../docs/cookbook/integrate-without-the-app.md)

## Product promise

**Medousa is available wherever the work is happening.** Home remains the
richest, most complete Medousa surface. External integrations are focused
windows into the active workshop, not smaller attempts to reproduce Home.

The first external surfaces are:

1. **VS Code** — the reference integration and broadest development surface.
2. **Neovim** — fast, keyboard-first commands and context-aware editing.
3. **Obsidian** — vault-native memory, search, linking, and note workflows.

Each integration should feel native to its host while preserving the same
daemon, identity, sessions, vault authority, capabilities, and streaming
semantics as Home.

## Goals

- Make the daemon/SDK contract a first-class integration boundary.
- Provide a shared TypeScript client for JavaScript/TypeScript plugin hosts.
- Support local and paired/remote workshops through the existing connection
  and pairing model.
- Pass host context explicitly: active file or note, selection, workspace,
  diagnostics, language, and current Medousa session.
- Expose the highest-value Medousa workflows without requiring plugin parity
  with Home.
- Let every surface hand advanced work back to Home.
- Make capability negotiation and graceful degradation normal behavior.

## Non-goals

- Rebuilding the Home shell, canvas, settings, onboarding, or full workshop IA
  in each host.
- Making TUI feature-complete with Home.
- Giving plugins a second filesystem or vault authority.
- Embedding inference or the turn loop in a plugin.
- Requiring every daemon capability to have a bespoke plugin UI.
- Designing three unrelated APIs or three separate authentication systems.

## Current foundation

The daemon already exposes most of the required substrate:

| Need | Existing foundation |
|------|---------------------|
| Chat and streaming | `POST /v1/interactive/turn` + reconnecting SSE |
| Sessions | `/v1/sessions/*` and `medousa-sdk` sessions accessor |
| Vault | `/v1/vault/*` and multi-root authority |
| Capabilities | `/v1/capabilities` |
| Code context | `/v1/code/*`, `CodeIntentContext`, coding engine |
| Governed work | `/v1/forge/*` |
| Artifacts | `/v1/runtime/artifact/*` |
| Workspace activity | `/v1/workspace/*` and event streams |
| Connections | local HTTP plus pairing/Workshop transport |
| Contract types | `medousa-types`, Rust SDK, Python SDK, SDK contract manifest |

The main gap is a stable, ergonomic client boundary for plugin hosts, plus
host-specific context and presentation conventions.

### Current progress

The Phase 0 scaffold is now underway in
[`packages/medousa-client/`](../packages/medousa-client/): generated daemon
types, health/capability/session access, interactive turn start/cancel,
reconnecting SSE, bounded context helpers, and a dependency-free build. The
VS Code vertical integration scaffold now lives in
[`integrations/vscode/`](../integrations/vscode/), with the next work focused
on runtime dogfooding and richer response/presentation behavior.

## Target architecture

```mermaid
flowchart TB
  subgraph hosts [Host surfaces]
    VS[VS Code extension]
    NV[Neovim plugin]
    OB[Obsidian plugin]
  end

  subgraph shared [Shared integration layer]
    TS[@medousa/client TypeScript client]
    CTX[Context envelope]
    CONN[Connection and pairing adapter]
  end

  subgraph engine [Medousa authority]
    HTTP[medousa_daemon HTTP + SSE]
    SDK[Existing SDK and typed contracts]
    HOME[Medousa Home]
  end

  VS --> TS
  NV --> TS
  OB --> TS
  TS --> CTX
  TS --> CONN
  CONN --> HTTP
  SDK --> HTTP
  HOME --> SDK
```

### Shared TypeScript client

Create a publishable workspace package, tentatively `@medousa/client`, with:

- typed health, capabilities, sessions, interactive turns, vault, artifacts,
  code context, Forge, and workspace accessors;
- streaming SSE with sequence tracking, bounded reconnect, cancellation, and
  stale-session protection;
- local and paired workshop connection configuration;
- secure-token handoff supplied by the host adapter rather than persisted by
  the shared package;
- capability checks and typed `UnsupportedCapability` behavior;
- a small escape hatch for routes that have not yet earned a typed accessor.

The package must consume the same contract source as the existing SDKs. It
must not become a parallel hand-maintained schema.

### Context envelope

Every host-originated request should be able to carry an optional context
envelope. The initial shape should remain small and explicit:

```ts
type MedousaSurface = "vscode" | "neovim" | "obsidian";

interface MedousaContext {
  surface: MedousaSurface;
  workspace?: string;
  file?: string;
  language?: string;
  selection?: { text: string; start?: Position; end?: Position };
  diagnostics?: Diagnostic[];
  vaultRootId?: string;
  notePath?: string;
  sessionId?: string;
}
```

Context is advisory input to a turn or code action. It does not transfer
filesystem authority to the plugin and must be bounded before entering a
prompt or API request.

### Authority and locality

- The workshop daemon remains the authority for vault roots, remote files,
  sessions, identity, and governed Forge work.
- A plugin may read host-local context through its host API, then send bounded
  context to the daemon.
- Direct local filesystem operations are host UX operations only; they must not
  be used to bypass daemon-owned remote workshop state.
- Obsidian’s local vault is presentation/context integration. Medousa vault
  APIs remain authoritative for Medousa-managed roots and writes.
- Forge worktree edits must use the existing lease/conflict/evidence contract;
  a plugin must not silently write around Forge custody.

## Surface scope

### VS Code — reference integration

**Initial commands and views:**

- Ask Medousa with current file, selection, diagnostics, and workspace context.
- Streaming chat panel with session continuation and cancel.
- Explain, fix, refactor, and generate actions for a selection.
- Search the Medousa vault and insert/link a result.
- Show active Forge undertaking, worktree state, tasks, and review handoff.
- Open an artifact or advanced workflow in Home.

**Later:** inline diff application, code-action provider, workspace activity,
permission/budget prompts, and richer artifact previews.

### Neovim — focused editing companion

**Initial commands and UI:**

- `:MedousaAsk` with visual selection and current buffer context.
- Floating-window streaming response with session continuation.
- Explain/fix selected code and apply an explicit patch or diff.
- Vault search and note insertion through Telescope/floating UI adapters.
- Diagnostics-aware ask and Forge task/status commands.

The Neovim integration should remain composable: a Lua core, configurable
keymaps, no mandatory UI framework, and no assumption that users want a
persistent chat pane.

### Obsidian — vault-native companion

**Initial commands and UI:**

- Ask about the current note, selection, folder, or linked-note neighborhood.
- Semantic vault search and backlink exploration.
- Create a note, append a section, or insert links with an explicit preview.
- Continue a Medousa session from the current note.
- Generate daily/weekly synthesis into a new note or selected location.
- Open Home for broader workshop, automation, artifact, or settings work.

The first version should avoid pretending that every Obsidian plugin API event
is a Medousa event. Synchronization and conflict behavior must be explicit.

## Phased delivery

### Phase 0 — contract and spike

**Goal:** prove the integration boundary without committing to three UIs.

- Confirm the minimum typed API surface against `sdk-contract/manifest.yaml`.
- Define the TypeScript package location and workspace/build conventions.
- Define the context envelope and size/redaction rules.
- Implement a local daemon health check and one streaming turn.
- Build a tiny host-neutral command-line or test harness around the client.
- Decide whether pairing credentials are exposed through a shared helper or
  host-specific secure-storage adapters.

**Exit:** a test client can connect to local Medousa, send bounded context,
stream a turn, reconnect by sequence, cancel it, and resume a session.

### Phase 1 — shared client package

**Goal:** make the external-client boundary reusable.

- Add `@medousa/client` and its tests.
- Add generated/shared DTO consumption from the canonical contract.
- Implement connection lifecycle, capability discovery, sessions,
  interactive streaming, vault search/read/write, and Home deep links.
- Add contract tests against daemon fixtures or a mock transport.
- Document package usage under `docs/sdk/`.

**Exit:** a versioned client can support all three adapters without direct
route strings scattered through plugin code.

### Phase 2 — VS Code vertical slice

**Goal:** validate the broadest host workflow end to end.

- Add extension scaffold, activation, commands, output/logging, and secure
  connection settings.
- Ship Ask Medousa, selection/file/workspace context, streaming panel,
  session continuation, cancel, and capability-aware empty states.
- Add vault search and explicit insertion.
- Add “Open in Medousa Home” links.
- Test local daemon, unavailable daemon, reconnect, malformed events, and
  remote paired workshop paths.

**Exit:** a developer can install the extension, connect to Medousa, ask a
contextual question, continue it, and use a vault result without opening TUI
or reproducing Home.

### Phase 3 — Neovim adapter

**Goal:** deliver a fast, keyboard-first surface on the same client contract.

- Add Lua host adapter and configurable command/keymap layer.
- Add floating streaming response and explicit patch/diff application.
- Add current buffer/visual selection/diagnostic context.
- Add optional Telescope integration without making it mandatory.
- Add vault search and Forge status commands.

**Exit:** a Neovim user can complete the common ask/explain/fix/search flows
without a persistent TUI-like shell.

### Phase 4 — Obsidian adapter

**Goal:** make Medousa feel native to the vault.

- Add current-note/selection/link-neighborhood context.
- Add search, backlinks, note creation, append, and link insertion previews.
- Define conflict handling for notes changed by both Obsidian and Medousa.
- Add daily/weekly synthesis as an explicit note-generation workflow.
- Add Home handoff for advanced workflows.

**Exit:** an Obsidian user can ask about their knowledge base and create or
update notes with visible, reversible intent.

### Phase 5 — hardening and distribution

- Publish install/update guidance for each platform.
- Add compatibility matrix: plugin version, daemon version, capabilities,
  transport, and supported host versions.
- Add telemetry-free diagnostics export and connection troubleshooting.
- Add signed/reproducible release workflows where each host marketplace
  requires them.
- Add end-to-end smoke coverage for local, LAN-paired, and remote workshop
  connections.

## Security and trust requirements

- Never log bearer tokens, pairing QR payloads, note contents, or full code
  selections by default.
- Store credentials in host-provided secure storage where available.
- Keep pairing/session tokens scoped to the selected workshop and user role.
- Bound context by bytes, files, selection length, and diagnostic count.
- Require explicit user confirmation before destructive note/file/Forge actions.
- Make writes previewable and reversible where the host supports it.
- Preserve daemon policy, budget, permission, and capability checks; plugins
  are clients, not policy bypasses.
- Treat remote workshop locality as authoritative from daemon connection
  metadata, not from host filesystem guesses.

## Testing strategy

| Layer | Required coverage |
|-------|-------------------|
| Shared client | DTOs, route contracts, SSE replay, cancellation, backoff, capability gaps |
| Context | bounded payloads, redaction, selection/file/note mapping |
| Connection | local HTTP, pairing token, reconnect, revoked token, unavailable daemon |
| VS Code | command activation, editor context, panel lifecycle, diff preview |
| Neovim | Lua API, headless command tests, streaming window cleanup, patch approval |
| Obsidian | note context, link insertion, conflict handling, previewed writes |
| End to end | daemon + one adapter against local and paired workshop fixtures |
| Docs | SDK contract checks, docs verification, install and troubleshooting paths |

## Release gates

No adapter is considered first-class until it has:

- a documented connection/setup path;
- a clear supported capability matrix;
- a working streaming turn with cancellation and reconnect behavior;
- explicit context and write confirmation behavior;
- an offline/unavailable-daemon state that explains what to do;
- a Home handoff for unsupported advanced workflows;
- tests that do not require a live personal vault or credential.

## Open decisions

1. Should the TypeScript client live in a new `packages/` workspace or beside
   the existing Home TypeScript code?
2. Should generated TypeScript DTOs be expanded from stream types into the
   complete SDK contract, or should the client initially use a smaller hand-
   audited public model?
3. What is the supported minimum daemon version for external plugins?
4. Which pairing/setup flow is appropriate for users who have a plugin but do
   not yet have Medousa Home installed?
5. Should Forge editing begin in VS Code only, or wait until shared client
   conflict semantics are proven?
6. What Obsidian vault mutation policy best preserves the daemon authority
   without fighting normal Obsidian editing?

## Initial implementation order

1. Resolve the Phase 0 contract and package placement decisions.
2. Build the shared client around health, capabilities, sessions, interactive
   streaming, vault read/search, and Home handoff.
3. Ship the VS Code vertical slice.
4. Reuse the client for Neovim’s focused command surface.
5. Reuse the client for Obsidian’s vault-native workflows.
6. Add Forge, artifacts, richer code actions, and workspace activity only
   after the core connection/context/session loop is reliable.
