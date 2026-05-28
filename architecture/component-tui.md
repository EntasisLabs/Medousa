# Component: medousa-tui

## Role in the Product

medousa-tui is the primary operator workspace.

It combines interaction, observability, and control in one surface:

- conversational operation with streaming output
- artifact and verification workflows
- runtime/routing/depth controls
- keyboard-first panel and overlay navigation

## Entry Point

- Binary: medousa/src/bin/medousa_tui.rs

## Runtime Assembly

Runtime composition is built through build_tui_runtime(...) in medousa/src/tools.rs.

Assembly includes:

- Stasis runtime composition
- memory reader/writer bindings
- tool registry and policy-aware allowlist enforcement
- prompt execution pipeline and tool-loop pipeline

Runtime surface exposed to UI loop:

- TuiRuntime { runtime, tool_loop_pipeline, tool_registry, memory_reader, memory_writer }

## State Ownership

TuiState in medousa_tui.rs is the central state machine.

It owns:

- conversation + scrolling
- observability + recent job status
- settings and routing drafts
- script editor state
- thinking traces and grapheme console output
- mode transitions across overlays

## Event and Update Model

The TUI loop multiplexes:

1. keyboard input events
2. asynchronous runtime/tool events (TuiEvent)
3. redraw cadence

TuiEvent is the explicit async boundary between runtime activity and UI projection.

## Interaction Surfaces

Primary surfaces:

- chat loop (prompt execution and streaming)
- slash command control plane
- history/session management overlay
- settings and routing editor overlay
- observability panel
- thinking views and grapheme console
- command palette and script editor

## Persistence and Secrets

User-level persistence (session.rs):

- history: ~/.local/share/medousa/history/<session>.jsonl
- defaults: ~/.local/share/medousa/tui_defaults.json
- last session pointer: ~/.local/share/medousa/last_session
- API key: OS keyring first, file fallback at ~/.local/share/medousa/secrets/api_key

## Configuration Semantics

Settings behavior is transactional:

- edit in draft
- validate
- apply or revert

Runtime/env notes:

- env overrides are parsed as KEY=VALUE
- env overrides are applied before runtime rebuild
- applied settings and routing/depth preferences are persisted

## Operational Expectations

- TUI state is ephemeral but deterministic while running
- user-facing persistence survives restart
- execution durability depends on selected backend (in-memory or surreal-mem)
- evidence/confidence visibility is progressive, not forced
