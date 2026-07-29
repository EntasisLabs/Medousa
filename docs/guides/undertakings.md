# Undertakings & ForgeLens

Undertakings are Medousa’s **governed work** surface: intentional changes in a
git worktree with lease-fenced attempts, sealed evidence, and human disposition.

They live under **Work → Undertakings** (beside **Activity**, which is the live
Stasis work board). They are not the same as activity-card worker ids.

## Loop

```text
Intent → Prepare → Work → Understand → Review → Preserve
```

1. **Create** an undertaking (title, brief, repo path, base ref).
2. **Provision** a governed worktree.
3. **Work** with Codex/Cursor (bound chat) or **Work in Terminal** (human attempt + lease).
4. **Seal** the active lease — ForgeLens shows the evidence patch and World insight.
5. **Approve** (Preserve Branch by default) and **Apply**, or **Discard**.

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
- Mark **Diagnostic** on a Terminal tab for an untracked shell (not part of
  sealed evidence).

## API (Home clients)

See `apps/medousa-home/src/lib/forge.ts` and daemon routes:

- `GET /v1/forge/items`, `…/review`, `…/evidence/{id}/patch|commands`
- `POST …/decisions` with **review intent** (server builds the decision)
- `GET /v1/forge/stream` for freshness
- `GET /v1/world/bindings/{work_id}` for World status
