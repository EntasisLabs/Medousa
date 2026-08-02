# Medousa Anywhere — external surfaces plan

> **Status:** In progress / living plan
> **Date:** 2026-07-31  
> **Owner:** Medousa platform  
> **Related:** [`architecture/ROADMAP.md`](ROADMAP.md), [`ADR-012 — surface taxonomy`](../docs/architecture/decisions/adr-012-medousa-anywhere-surfaces.md), [`docs/sdk/README.md`](../docs/sdk/README.md), [`docs/engine/http-api.md`](../docs/engine/http-api.md), [`docs/cookbook/integrate-without-the-app.md`](../docs/cookbook/integrate-without-the-app.md)

## Product promise

**Wherever Medousa is, it feels like Home.** Home remains the richest, most
complete Medousa surface. External integrations are focused windows into the
active workshop, not smaller attempts to reproduce Home.

The first external surfaces are:

1. **VS Code** — the reference integration and broadest development surface.
2. **Neovim** — fast, keyboard-first commands and context-aware editing.
3. **Obsidian** — vault-native memory, search, linking, and note workflows.

Each integration should feel native to its host while preserving the same
daemon, identity, sessions, vault authority, capabilities, and streaming
semantics as Home.

### Surface taxonomy

The Anywhere family has two different host relationships:

- **Native host surfaces:** VS Code, Neovim, and Obsidian run as plugins inside
  an editor or vault host. They capture host-native context and offer
  host-native, explicit editing actions.
- **External-agent adapters:** Notion, Slack, and future channel hosts invoke
  Medousa as a participant. They translate host messages, assignments,
  context, approvals, progress, and results into daemon sessions and back.

Notion is therefore not a second embedded Medousa chat client. It is a rich
external-agent adapter using Notion's External Agents API. The Notion Agent SDK
is the opposite direction—bringing Notion agents into another app—and is not
the primary boundary for Medousa-in-Notion.

### Interaction standard: Home is a behavior, not a skin

Plugins should adopt their host's visual language while preserving Home's
interaction character:

- **Intent survives navigation.** Drafts, context, session choice, and scroll
  position belong to the work the user was doing, not to a disposable panel.
- **State is honest and proportionate.** Show the current useful state—opening,
  thinking, using a tool, waiting, recovering, done—without leaking engine logs.
- **Actions appear when they are valid.** A streaming reply is not yet a settled
  artifact; destructive and irreversible actions name their consequence.
- **Every wait acknowledges the gesture.** Switching, renaming, saving, and
  reconnecting respond immediately and finish with clear feedback.
- **Recovery preserves momentum.** Cancellation, transient failure, stale
  sessions, and reconnection keep drafts and provide the most relevant next step.
- **Context changes the invitation.** Empty states and suggested actions reflect
  the selected code, note, diagnostics, or workspace instead of generic prompts.
- **Keyboard, pointer, touch, narrow widths, and assistive technology are equal
  paths.** Host-native shortcuts and focus restoration are product behavior.
- **Advanced work hands off without becoming a dead end.** Opening Medousa keeps
  the same workshop and session mental model.

Visual resemblance to Home is welcome where it fits the host, but it is never a
substitute for these behavioral guarantees.

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
on runtime dogfooding, richer response/presentation behavior, and the first
vault/Home handoff actions. The extension now contributes a persistent
activity-bar chat view; its client runtime is bundled into the VSIX.
The 0.2 polish sprint adds Home-standard stream projection, a persistent chat
shell and composer, safe Markdown/code actions, structured tool and approval
states, session restoration, connection diagnostics, and editor context chips.
The 0.3 conversation library adds cross-surface history, naming, deletion, and
Home's settled-reply actions. The 0.4 interaction pass adds per-session drafts,
deliberate navigation/loading states, contextual invitations, settled-action
timing, focus recovery, and long-transcript navigation.

The Neovim first slice now lives in [`integrations/neovim/`](../integrations/neovim/).
It uses a transient, keyboard-first coding room rather than a persistent chat
pane: the current buffer, visual selection, and diagnostics are supplied as
bounded context; replies stream with recovery; and code application is always
an explicit confirmed action.

## Target architecture

```mermaid
flowchart TB
  subgraph hosts [Host surfaces]
    VS[VS Code extension]
    NV[Neovim plugin]
    OB[Obsidian plugin]
  end

  subgraph adapters [External-agent adapters]
    NO[Notion external agent]
    SL[Future Slack-like adapters]
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
  NO --> HTTP
  SL --> HTTP
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
persistent chat pane. The first slice is deliberately a transient “coding
room” opened by a hotkey or command. It restores the active daemon session,
keeps focus in the prompt after a turn, exposes concise tool/recovery states,
and lets `MedousaApply` insert or replace a fenced code block only after
confirmation.

The first polish pass adds a multiline per-session composer, motion/range
context, cursor-centered buffer excerpts, a shared conversation picker with
rename/delete, optional Telescope discovery, compact tool states, a statusline
API, and unified-diff previews guarded by buffer revision before one-step
undoable application.

The Obsidian native surface now lives in
[`integrations/obsidian/`](../integrations/obsidian/). It opens a native
Obsidian view, restores and manages daemon-owned conversations, captures bounded
current note/selection/link context, streams a turn with reconnect semantics,
and exposes daemon-backed search, backlinks, synthesis prompts, and explicit
answer-to-note workflows. Note creation is create-only; append uses a fresh
read plus `If-Match`; link insertion stays a deliberate Obsidian editor action.

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

### Notion — external-agent adapter

Notion is a hosted Medousa agent, not a native editor plugin. The adapter is
responsible for:

- registering or connecting Medousa through the External Agents API;
- mapping mentions, assignments, pages, comments, and tasks to daemon
  sessions and turns;
- translating progress, questions, approvals, and terminal results back into
  Notion events;
- representing page/block changes as reviewable proposals before applying them;
- persisting idempotency keys, event cursors, and the Notion-session to
  Medousa-session mapping.

The first action is a private-beta contract/access spike. We must verify the
real authentication, callback/webhook, session, message, event, and approval
semantics before adding a production adapter. Notion Workers may later expose
selected Medousa tools to Notion Custom Agents, but Workers are not a
substitute for the External Agents API.

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

### Phase 3 — Neovim adapter (first slice implemented)

**Goal:** deliver a fast, keyboard-first surface on the same client contract.

- Add Lua host adapter and configurable command/keymap layer. **Complete for
  the first slice.**
- Add floating streaming response and explicit confirmed fenced-code
  application. **Complete for the first slice.**
- Add current buffer/visual selection/diagnostic context. **Complete for the
  first slice.**
- Add optional Telescope integration without making it mandatory. **Complete.**
- Add vault search and Forge status commands.

**Exit:** a Neovim user can complete the common ask/explain/fix flows without a
persistent TUI-like shell. Search, vault, Forge, and richer patch/diff review
remain follow-on work.

### Phase 4 — Obsidian adapter

**Goal:** make Medousa feel native to the vault.

- Add current-note/selection/link-neighborhood context. **Complete.**
- Add search, backlinks, note creation, append, and link insertion previews.
  **Complete.**
- Define conflict handling for notes changed by both Obsidian and Medousa.
  **Complete for previewed append writes via `If-Match`.**
- Add daily/weekly synthesis as an explicit note-generation workflow.
  **Complete for the first workflow.**
- Add Home handoff for advanced workflows. **Complete for the first handoff.**

**Exit:** an Obsidian user can ask about their knowledge base, explore linked
notes, create or update notes with visible, reversible intent, and hand advanced
work to Home. Rich Markdown rendering and broader vault actions remain
hardening/follow-on work.

### Phase 5 — Notion external-agent adapter

**Goal:** make Medousa a first-class agent inside Notion without moving the
Medousa runtime into Notion.

- Verify External Agents API beta access and capture the exact contract.
- Implement durable Notion-session ↔ Medousa-session and task ↔ turn mapping.
- Ship mention/assignment → context → streaming progress → terminal result.
- Add page/block context reads and approval-aware write proposals.
- Add retries, idempotency, event cursors, connection diagnostics, and a Home
  handoff for work that exceeds the Notion surface.

**Exit:** a Notion user can assign work to Medousa, follow honest progress,
answer a question or approval request, and review an explicit page/block change.

### Phase 6 — hardening and distribution

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
| Notion | external-agent contract, session mapping, event replay, approvals, block proposals |
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
7. Which Notion External Agents API beta contract and distribution path are
   available to Medousa?

## Initial implementation order

1. Resolve the Phase 0 contract and package placement decisions.
2. Build the shared client around health, capabilities, sessions, interactive
   streaming, vault read/search, and Home handoff.
3. Ship the VS Code vertical slice.
4. Reuse the client for Neovim’s focused command surface.
5. Reuse the client for Obsidian’s vault-native workflows.
6. Spike and then implement Notion as an external-agent adapter.
7. Add Forge, artifacts, richer code actions, and workspace activity only
   after the core connection/context/session loop is reliable.
