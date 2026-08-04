# Agent runtime modes

> Status: Coder tools complete; agent-proposed transitions next
> First modes: **General** and **Coder**

## Product decision

Medousa remains one continuous collaborator. A mode changes how Medousa works
for the current turn or task; it does not create a second personality or a
separate chat.

Modes sit above execution lanes, specialists, model routing, and client
surfaces:

| Concept | Responsibility |
|---|---|
| Agent mode | Behavior, context, tools, loop policy, and completion contract |
| Execution lane | Where work runs: host, foreground workshop, background worker |
| Specialist / manuscript | Optional declared specialty |
| Inference profile | Provider/model selection per phase |
| Surface | Where the conversation is rendered |

The default mode is `general`. `coder` is repository-aware and enters a
foreground execution lane directly when the user explicitly selects it.

## Mode contract

Every registered mode resolves to an immutable per-turn snapshot containing:

- mode id and contract revision;
- core Medousa identity plus a mode-specific STTP overlay;
- context providers and their budgets;
- bootstrap, auto-unlocked, discoverable, and denied tools;
- execution-lane and model-profile preferences;
- loop and completion policy;
- entry, exit, and transition policy.

Mode policy overlays and compiled mode world-state context use canonical
`sttp-1.0` nodes: provenance (`⊕`) → envelope (`⦿`) → confidence-weighted
content (`◈`) → metrics (`⍉`). Markdown or ad-hoc XML prompt overlays are not
valid mode contracts. Structural validation is required alongside semantic
prompt tests.

Mode resolution is deterministic and does not require a model call:

1. explicit turn override;
2. active task-scoped mode lease;
3. session-selected mode;
4. `general`.

The resolved snapshot cannot change during a live turn. Transitions commit at
turn boundaries and are recorded as structured transcript events.

## Entry and exit

Entry and exit are runtime hooks, not ordinary model tools.

`enter` validates required capabilities, resolves authoritative environment
state, builds bounded ambient context, compiles the tool surface, and acquires
any task lease. `exit` validates the completion contract, records the result,
and releases temporary resources.

The model may propose a transition through a control action, but only the
runtime can expand authority. Proposals support `task` and `session` scopes.
Denial or expiry resumes the pending request in the existing mode.

## General mode

General mode is the current Medousa life-agent behavior. The first migration
slice must preserve its prompt, tools, lanes, and completion behavior exactly
while routing the turn through the typed mode contract.

## Coder mode

Coder is a repository world-model, not a prompt that merely says “act like a
senior engineer.” Its execution cycle is:

1. observe repository and worktree state;
2. form an evidence-backed hypothesis;
3. plan the smallest safe change;
4. edit against expected digests and Forge authority;
5. run proportional validation;
6. inspect the resulting diff;
7. report outcome and residual risk.

Coder entry resolves the Forge `work_id`, worktree, base revision, dirty state,
lease, repository instructions, language/package map, available language
servers, and recommended checks. Remote Home remains a window into the
workshop daemon's filesystem.

Coder's initial tool surface should include bounded file reads, fast glob and
regex search, git status/diff/log, digest-fenced batch patches, structured
command execution, language intelligence, targeted tests, and final diff
inspection. Coding-engine and language-server packages remain optional and
must degrade gracefully.

## Switching UX

Home exposes a composer mode picker. User selection may apply to this task or
this chat. Medousa may propose Code mode for repository inspection, edits, or
tests, but ordinary programming explanations stay in General.

Explicit user selection switches deterministically. Agent proposals render as
accept/deny controls with bounded expiry. A timeout denies the transition and
continues in the current mode rather than abandoning the request.

## Runtime optimization alongside modes

1. Add mode/lane timing and token telemetry before changing loop semantics.
2. Reconcile host-bus `auto` behavior with its documented conditional routing.
3. Parallelize independent memory, identity, policy, and environment probes.
4. Replace routine classifier calls with one deterministic activation/mode
   resolver; reserve model classification for ambiguous turns.
5. Keep a stable prompt prefix: core identity, mode overlay, surface
   capabilities, then dynamic turn context.
6. Compile a mode's tool surface once per entry and keep it immutable per turn.
7. Run Coder directly in a foreground workshop lane instead of paying a
   host-to-worker-to-synthesis round trip.
8. Persist full tool receipts out of band and feed compact digests back into
   the model loop.
9. Execute independent reads concurrently while serializing conflicting
   mutations.
10. Move completion toward typed runtime outcomes: continue, complete,
    need-input, delegate, propose-mode, and await-approval.

## Delivery slices

1. **Mode kernel / General parity (complete)** — shared type, request plumbing, registry,
   immutable resolved snapshot, telemetry, and tests. No behavior change.
2. **Session and task state (complete)** — persisted mode selection, transition records,
   and task-scoped leases.
3. **Home picker (complete)** — General selection plus unavailable/readiness states for
   Coder.
4. **Coder entry/context (complete)** — Forge binding, bounded repository/editor
   ambient context, Coder STTP overlay, and foreground workshop lane contract.
5. **Coder tools (complete)** — per-turn Forge lease, immutable allowlisted tool
   registry, worktree and mutation-policy fencing, digest-safe edits, shell/LSP/
   Detamu binding, command receipts, and foreground-loop activation. Shell
   sessions are exposed only for unrestricted path policies and are interrupted
   before the lease is released.
6. **Agent proposals (complete)** — boundary-safe `propose_mode` control tool,
   durable accept/deny/expiry state, inline Home controls, and user-configurable
   proposal TTL plus `never`/`task`/`all` auto-accept policy. Applied changes
   begin on the next turn and never mutate a live turn contract.

## First-slice acceptance criteria

- `agent_mode` is part of daemon turn contracts and defaults to `general`.
- The runtime resolves an immutable General-mode snapshot before prompt prep.
- General mode returns the existing system prompt and tool policy unchanged.
- Unsupported/unavailable modes fail explicitly rather than silently falling
  back or expanding authority.
- Home and existing adapters remain compatible when they omit `agent_mode`.
- Unit and contract tests cover defaulting, serialization, resolution, and
  General prompt parity.
