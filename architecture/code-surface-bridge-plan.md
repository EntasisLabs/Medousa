# Code surface bridge

> Status: Implementing on `feat/medousa-home-coding-gap` (truth cleanup, DiffStack Review, soft lease, buffer prefs shipped in-branch)
> Related: [code-flowstate-roadmap.md](code-flowstate-roadmap.md),
> [agent-runtime-modes-plan.md](agent-runtime-modes-plan.md),
> [coder-cognitive-runtime-plan.md](coder-cognitive-runtime-plan.md)

## Product boundary

Workshop owns IDE chrome (tabs, panes, desktops, Spotlight). Code file tabs are
buffers. **Review is a separate project tab** for decide/approve/finish. Human,
agent, and Terminal changes meet in that same Review — provenance distinguishes
authors; the surface does not.

Stacked multi-file scroll is the default Review layout (Codex-inspired). There
is no user-only commit UI distinct from agent Review.

## Delivery slices

1. Truth cleanup — docs, shortcuts, orphan Code tab/split chrome removed.
2. Unified Review back + shared `DiffStack`.
3. Codex-style Review (stack default, unmodified collapse, aggregate +/−).
4. Chat diff summary → same Review tab.
5. Conflict compare on DiffStack.
6. Soft lease + Code buffer prefs/find/fold.
7. Multi-attempt Home awareness and pane-local source tab cycle.

## Non-goals

Full SCM staging, extension marketplace, permanent sidebars, debugger-first,
folding Review into the editor pane, remounting private Code editor groups.
