# Undertakings & ForgeLens

Undertakings are Medousa’s **governed work** surface: intentional changes in a
git worktree with lease-fenced attempts, sealed evidence, and human disposition.

They live under **Work → Undertakings** (beside **Activity**, which is the live
Stasis work board). They are not the same as activity-card worker ids.

## Loop

```text
Intent → Prepare → Work → Understand → Review → Preserve
```

1. Choose **New undertaking**, then provide a title, brief, repo path, and base ref.
2. **Prepare workspace** provisions the governed worktree.
3. Continue with Codex/Cursor (bound Chat) or in Terminal (human attempt + lease).
4. **Review changes** seals the active lease and opens its evidence in ForgeLens.
5. **Approve checkpoint**, then separately **Apply** it. Discard remains available
   under the secondary actions menu.

Chat and Terminal show the same compact undertaking context. Open the context
chip to move between the undertaking, ForgeLens, Terminal, and a coding agent
without rebuilding context. The chip also shows the current phase and executor;
the full Forge state stays under **Workspace details** when it is useful.

## Observe vs manipulate

| Surface | Role |
|---------|------|
| **Git control** (Undertaking actions) | Mutate via Forge verbs only |
| **ForgeLens** | Review baseline → sealed evidence |
| **World** | Observe Detamu / git-as-world (never mutates) |
| **Versions** | Vault material memory (separate) |

Seal does not wait for Detamu indexing. World bindings show `queued` /
`indexing` / `ready` / `failed`. Missing analyzers are unavailable — not “zero
impact.”

## Terminal ownership

- **Work in Terminal** begins a human attempt when needed and opens the PTY with
  `work_id` + `lease_id` so commands can enter sealed evidence.
- Tracked Terminal tabs retain their undertaking when restored and keep their
  active lease fresh while open.
- Choose **Diagnostic** in the Terminal header to open a separate untracked
  shell. Its commands are not part of sealed evidence.

## ForgeLens and World

ForgeLens starts with changed files and compact code-understanding coverage.
Patch and command evidence remain scrollable supporting detail. Policy
exceptions and risky content are called out before approval; an exception must
be explicitly acknowledged. Applying an approved checkpoint has its own
confirmation boundary.

World is snapshot-aware. Choose **Baseline** or **Sealed**, search entities, and
select one to inspect impact. Analyzer capabilities marked unavailable are
missing evidence, not a zero score. World is always observe-only; repository
changes still go through Forge actions.

## API (Home clients)

See `apps/medousa-home/src/lib/forge.ts` and daemon routes:

- `GET /v1/forge/items`, `…/review`, `…/evidence/{id}/patch|commands`
- `POST …/decisions` with **review intent** (server builds the decision)
- `GET /v1/forge/stream` for freshness
- `GET /v1/world/bindings/{work_id}` for World status
