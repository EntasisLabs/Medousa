# Code flow-state roadmap

## Product boundary

Medousa is a user-domain agentic workspace, not a coding-centric agent hub.
Code is one permanent workspace surface alongside Chat, Vault, Terminal,
Projects, and other forms of work. The engineer should orchestrate intent while
Medousa carries context, Forge governs execution, and Detamu supplies domain
understanding.

The roadmap follows three interaction rules:

1. Infer safe defaults and ask only when context is ambiguous or risk is real.
2. Preserve intent and location across every surface transition.
3. Surface exceptions and decisions; keep continuous machinery internal.

## Completed foundation

The first flow-state pass established folder-first project start, repository
inspection, inferred Git context, Quick Open and navigation history, durable
editor context, coding-agent prompt context, detected verification tasks,
review synthesis, conflict recovery, undoable file deletion, and humanized
Terminal sessions.

## Milestone 1 — Seamless human and agent handoff

Status: complete (2026-07-29).

### Goal

Move between direct editing and coding-agent execution without losing drafts,
location, intent, or Forge custody.

### Deliverables

- A typed active-code context: project outcome, active file, cursor/selection,
  containing symbol when known, open files, diagnostics, and last verification.
- A governed executor handoff that saves or explicitly preserves drafts,
  interrupts the current lease, and begins the next executor without changing
  worktrees.
- Contextual actions earned by a selection or issue: Ask, Change, Fix, Explain,
  and Add test.
- Preferred-agent continuation with an unobtrusive runtime chooser.
- Agent-to-human reclaim with the same context preservation guarantees.
- User-facing provenance that distinguishes human, agent, and Terminal work
  without exposing lease or attempt vocabulary.

### Success criteria

An engineer can select code, ask their preferred agent to change it, inspect
the result, and resume editing without naming the file, managing a lease, or
reconstructing what they were doing.

## Milestone 2 — Workshop-native repository discovery

### Goal

Make first-use project selection equally effortless for local and remote
workshops while preserving daemon filesystem authority.

### Deliverables

- Daemon-owned recent and pinned repository catalog.
- Scoped remote directory and repository browser.
- Duplicate-project detection with Continue existing / Start another change.
- Repository trust and dirty-state explanation before worktree creation.
- Optional clone flow through a provider adapter; no local picker for remote
  workshops and no upload-based vault flow.

### Success criteria

A remote user can find and start work in a repository without typing a server
path, and Medousa never confuses the Home device filesystem with the workshop.

## Milestone 3 — Review, provenance, and recovery

### Goal

Turn Review into a concise decision surface rather than a raw Git report.

### Deliverables

- Per-file inline and side-by-side diffs with binary fallbacks.
- Changed-line gutter indicators in Code.
- Human, agent, Terminal, and verification attribution.
- Outcome, risk, verification, and unresolved-issue synthesis.
- Project timeline backed by Forge attempts, evidence, and checkpoints.
- Compare and restore paths that retain newer work until the user decides.

### Success criteria

Review answers whether the intended outcome was achieved, what deserves human
attention, and what can safely happen next.

## Milestone 4 — Project-wide intelligence

### Goal

Use Detamu and language servers to deepen the same workspace rather than build
language-specific editor implementations.

### Deliverables

- Project-wide diagnostics prioritized around current and changed code.
- File, workspace-symbol, and go-to-line modes in Quick Open.
- Breadcrumbs and sticky containing-symbol context.
- References, rename, formatting, imports, and language code actions when the
  active provider supports them.
- `.editorconfig`, repository convention, and language-default resolution.
- One-click installation or repair when a language capability is missing.

### Success criteria

Language depth appears progressively when available, while every file remains
editable and understandable at the best supported layer.

## Milestone 5 — Interactive run and test loop

### Goal

Let engineers validate intent without remembering project-specific commands or
turning the workspace into a permanent panel dashboard.

### Deliverables

- A project-task provider port for checks, tests, builds, and long-running
  development processes.
- Named, cancellable Terminal sessions for interactive tasks.
- Test discovery, targeted execution, rerun, and inline status when supported.
- Clickable compiler, test, and stack-trace locations.
- Verification results captured as Forge evidence and promoted in Review.

### Success criteria

Run, Test, and Verify mean the correct thing for the current project, and a
failure takes the engineer directly to the relevant code.

## Milestone 6 — Optional provider handoff

### Goal

Deliver governed work to repository and ticket providers without making those
providers the center of the Medousa workspace.

### Deliverables

- Optional GitHub/GitLab-style clone and repository adapters.
- Share branch and open/update pull request actions after Forge completion.
- Issue, PR, and ticket links carried as project context.
- Review comments importable as new intent or follow-up work.
- Forge evidence and outcome summary attached to the external handoff.

### Success criteria

Completed work can leave Medousa cleanly, while project custody and the user’s
mental model remain provider-independent.

## Explicit non-goals

- A full source-control staging client or branch graph.
- An extension marketplace.
- Permanent Terminal, Problems, Tests, or Agent panels.
- Multi-root configuration before demonstrated user need.
- A full debugger workbench before the simpler run/test loop earns it.
- AI controls on every line or settings for machinery Medousa can infer.

## Delivery order

Milestones are ordered by dependency and cognitive-load reduction. Handoff
precedes richer provider integrations; remote discovery precedes hosted clone;
provenance precedes external publishing; project intelligence feeds the test
loop. A later milestone may reuse earlier ports, but should not bypass their
custody, context, or recovery contracts.
