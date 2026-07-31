# VS Code Chat Polish — Home-standard companion surface

> **Status:** Implementation complete · packaged dogfood pending  
> **Date:** 2026-07-31  
> **Scope:** `integrations/vscode/` chat surface  
> **Related:** [Medousa Anywhere plan](medousa-anywhere-plan.md), [Home chat panel](../apps/medousa-home/src/lib/components/chat/ChatPanel.svelte), [stream display policy](../apps/medousa-home/src/lib/utils/chatStreamDisplay.ts)

## Purpose

Bring the VS Code integration from a working daemon proof into a calm,
trustworthy chat companion that meets the behavioral standard of Medousa Home
without attempting to reproduce Home’s entire shell.

The screenshot captured the correct product concept but also exposes the first
polish boundary: the sidebar currently displays daemon lifecycle and telemetry
as if they were conversation messages. Home already has the correct answer in
`chatStreamDisplay.ts`: operator-facing progress is a status line, engine
telemetry is hidden by default, tool activity is structured, and only authored
answer content belongs in the assistant bubble.

## Product target

The VS Code sidebar should feel like:

- a persistent Medousa room attached to the current workspace;
- quiet while the engine is working, informative when user attention is
  required;
- useful with only the keyboard;
- native to VS Code’s theme and webview constraints;
- explicitly aware of the active file, selection, diagnostics, and workshop
  connection;
- one click away from advanced work in Home.

It should not feel like a terminal log, an ACP debug console, or a second Home
window squeezed into the activity bar.

## Screenshot findings → acceptance outcomes

| Current observation | Target outcome |
|---------------------|----------------|
| `interactive turn accepted` appears in the transcript | Hidden or rendered as a quiet transient status |
| Context/token telemetry appears in the transcript | Hidden by default; optional “View details” affordance |
| `cognition_turn_finish` appears as conversation text | Hidden from the answer; retained only for diagnostics |
| Tool calls appear as raw text | Compact collapsible tool-run cards with running/succeeded/failed state |
| Assistant greeting is duplicated | One user bubble, one assistant bubble, one projected stream |
| Text is plain and hard to scan | Markdown paragraphs, headings, lists, code fences, and copy actions |
| Composer is visually detached | Bottom-anchored composer with send/cancel, keyboard shortcut, and context chips |
| No connection/session identity | Header status, workshop endpoint label, session continuity, reconnect state |
| No empty/loading/error design | Presence-style empty state, progress state, friendly errors, retry action |

## Design boundaries

### Home parity means behavioral parity

Reuse Home’s stream classification semantics and durable-turn assumptions:

- `content_delta` and `final_text` feed the answer body;
- `operator_message` feeds a human-facing status line;
- `debug_message` and known telemetry remain hidden unless diagnostics are
  explicitly enabled;
- `tool_started` / `tool_finished` become tool-run state, not prose;
- terminal error events become a friendly error with optional raw details;
- sequence numbers remain the source for deduplication and reconnect replay.

The VS Code visual language may be simpler than Home’s, but it must not expose
lower-level events that Home deliberately hides.

## Sprint status

| Slice | Status |
|-------|--------|
| P0 Stream projection and message model | ✅ Implemented + regression fixtures |
| P1 Chat shell and visual hierarchy | ✅ Implemented |
| P2 Composer quality | ✅ Implemented |
| P3 Answer rendering | ✅ Implemented with CSP-safe Markdown/code actions |
| P4 Status, tools, and attention states | ✅ Implemented |
| P5 Session and connection UX | ✅ Implemented |
| P6 Context and Medousa-native actions | ✅ Implemented; Forge editing remains deferred |
| P7 Quality gate and distribution | ✅ Tests + VSIX package; live daemon dogfood pending |

### No full Home clone

The sidebar does not need Home’s canvas, multi-pane shell, Liquid scene player,
mobile layout, or every composer attachment. It does need a polished chat loop,
context awareness, session continuity, tool/status honesty, and handoff links.

## Phased polish plan

### P0 — stream projection and message model

**Goal:** Stop presenting daemon events as chat prose.

Add a host-neutral projection layer in the VS Code adapter or shared client:

```ts
type ProjectedEvent =
  | { kind: "answer_delta"; text: string }
  | { kind: "status"; text: string }
  | { kind: "tool_started"; runId: string; name: string; summary?: string }
  | { kind: "tool_finished"; runId: string; name: string; status: string; summary?: string }
  | { kind: "budget_request"; requestId: string; rounds: number }
  | { kind: "permission_request"; requestId: string; message: string }
  | { kind: "terminal"; text?: string; error?: boolean };
```

Tasks:

- Port or share the Home `chatStreamDisplay` classification rules.
- Suppress `event.message` unless it is proven to be operator-facing content.
- Deduplicate `content_delta` and terminal `final_text`.
- Keep engine details behind a collapsed diagnostics disclosure.
- Preserve `seq` and `turn_id` in the projection state.
- Add unit fixtures for the exact event sequence shown in the screenshot.

**Exit:** a greeting turn produces one clean answer; lifecycle, context, and
tool telemetry do not appear as raw transcript lines.

### P1 — chat shell and visual hierarchy

**Goal:** Make the sidebar read as a chat room.

- Add a compact header: Medousa identity, connection dot, workshop label, and
  overflow actions.
- Add an intentional empty state with a short welcome and example prompts.
- Render user and assistant turns as grouped bubbles with timestamps only when
  useful.
- Keep the message list scrollable and the composer permanently bottom-anchored.
- Add a new-turn divider or subtle session boundary when the session resumes.
- Use VS Code theme variables exclusively; do not hard-code Home colors.
- Respect narrow sidebar widths and high-contrast mode.

**Exit:** the empty, working, populated, and narrow-sidebar states all look
deliberate rather than like a debug panel.

### P2 — composer quality

**Goal:** Reach the minimum Home-quality input loop.

- Auto-growing textarea with sensible maximum height.
- `Enter` sends; `Shift+Enter` inserts a newline; `Ctrl/Cmd+Enter` remains a
  supported fallback.
- Send button changes to cancel while a turn is active.
- Disable duplicate sends while a request is being accepted.
- Preserve draft text through transient connection failures.
- Add context chips for active file, selection, diagnostics, and workspace.
- Add a small command/context affordance without forcing Home’s full slash menu.
- Add an explicit “clear conversation” or “new session” action.

**Exit:** keyboard-first users can send, cancel, retry, and start a fresh room
without losing their draft or accidentally creating duplicate turns.

### P3 — answer rendering

**Goal:** Make Medousa’s answer readable and useful inside code work.

- Render Markdown safely inside the webview.
- Syntax-highlight fenced code using the declared language when possible.
- Add copy buttons to code blocks.
- Add “Open in editor” / “Insert at selection” only after explicit confirmation.
- Preserve streaming cursor behavior without re-rendering the entire history on
  every token.
- Detect artifact references and show a compact artifact action row.
- Make links open through the VS Code host rather than unrestricted webview
  navigation.

**Exit:** a response with prose, lists, and code is scannable and actionable;
the webview never executes model-authored HTML or scripts.

### P4 — status, tools, and attention states

**Goal:** Expose useful progress without noise.

- Add one transient status row for the current phase/operator message.
- Render tool runs as compact collapsible cards:
  `Reading vault` → `Running` → `Completed` / `Failed`.
- Keep tool input/output summaries collapsed by default.
- Add budget approval UI when `budget_request_id` is present.
- Add permission approval UI when `permission_request_id` is present.
- Distinguish retryable connection errors from turn failures.
- Add friendly copy with a “View technical details” disclosure.

**Exit:** users understand whether Medousa is thinking, using a tool, waiting
for approval, finished, or failed—without reading engine internals.

### P5 — session and connection UX

**Goal:** Make the sidebar feel attached to the current workshop.

- Load the existing session history when the view opens.
- Restore the active session after VS Code reload.
- Show connection states: checking, connected, reconnecting, unavailable,
  unauthorized.
- Provide Configure Connection and Open in Home actions from the header menu.
- Validate a stale session id and transparently create a replacement session.
- Keep pairing/remote endpoint details out of ordinary chat content.

**Exit:** reopening VS Code feels like returning to a room, not starting a
stateless command every time.

### P6 — context and Medousa-native actions

**Goal:** Make the chat meaningfully better than a generic chatbot panel.

- Show active context chips with removable/inspectable state.
- Add “Ask about selection” and “Explain this file” commands.
- Add vault search as a result/action surface.
- Add “Open in Home” for artifacts, advanced workflows, and unsupported actions.
- Add safe code action previews only after the answer renderer and confirmation
  model are stable.
- Add Forge context only through governed APIs, never direct worktree writes.

**Exit:** VS Code users can stay in the editor for common work, while complex
Medousa workflows hand off cleanly to Home.

### P7 — quality gate and distribution

- Add webview unit tests for projection, markdown escaping, message grouping,
  and composer keyboard behavior.
- Add extension-host smoke tests for activation, sidebar registration, command
  routing, and SecretStorage.
- Test local daemon, paired daemon, unavailable daemon, revoked token, stream
  reconnect, cancellation, and stale session.
- Test light/dark/high-contrast themes and 240px/320px sidebar widths.
- Test the packaged VSIX, not only the source extension host.
- Add a screenshot checklist based on the current reference image.

## Recommended implementation order

1. P0 stream projection — fixes the most visible trust problem.
2. P1 chat shell — establishes the visual foundation.
3. P2 composer — makes the surface pleasant to use.
4. P3 answer rendering — makes output useful for code work.
5. P4 status/tools — restores Home’s operational honesty.
6. P5 sessions/connections — makes it durable.
7. P6 context/actions — expands capability without bloating the shell.
8. P7 release gate — prevents the sidebar from regressing into a log viewer.

## Explicitly deferred

- Full Home canvas and Liquid scene support.
- All Home composer slash commands and attachments.
- Full ACP agent management UI.
- Complete Forge review UI.
- Obsidian/Neovim-specific behavior in the VS Code adapter.

## Definition of “Home-standard”

The VS Code plugin is ready for broader capability work when:

- no raw lifecycle/debug event appears in normal chat;
- one turn produces one coherent user/assistant exchange;
- streaming, cancellation, reconnect, and retry are understandable;
- markdown and code are readable;
- tools and approvals are visible but subordinate to the answer;
- session and connection state are recoverable;
- the composer is usable by keyboard in a narrow sidebar;
- every unsupported advanced action has a clear Home handoff.
