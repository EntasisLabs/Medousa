# Permissions, budgets, and tool safety

Agents ask before risky tools and before burning extra tool rounds. Your job is to approve with intent — not to click Allow on everything.

Related: [Chat](guide:chat) · [Work and background jobs](guide:work-jobs) · [Troubleshooting](guide:troubleshooting)

## Tool permissions

When a turn needs an elevated capability, chat shows **Agent needs permission**:

| Control | Meaning |
|---------|---------|
| **Allow** | Grant this request so the turn can continue |
| **Deny** | Refuse; the turn proceeds without that capability (or stops, depending on the agent) |

The bar may show which **Runtime** asked (Medousa, Cursor, Codex). Home does **not** expose a separate “always allow” toggle — treat each prompt as a decision for this request.

If you Deny by accident, send a clearer follow-up or start a new turn with a narrower ask.

## Tool-round budgets

Long tool loops pause with **Needs your approval**:

| Control | Meaning |
|---------|---------|
| **Approve** | Grant the requested **+N tool round(s)** |
| **Deny** | Stop extending the turn |
| **Work** | Jump to the linked Work card (when present) |

Slash shortcuts (also in Spotlight):

- `/budget` or `/budget list` — pending approvals
- `/budget approve [id]` — grant
- `/budget deny [id]` — refuse

Budget pressure often appears as a **blocked** card on the [Work](guide:work-jobs) board. Inspect the card to approve or deny from there as well.

```callout
tone: warning
title: Budgets are cost and blast-radius
body: Approving more rounds lets the agent keep calling tools. Deny when the goal is done, the path looks wrong, or you need to redirect in chat.
```

## Browser verification

If the agent hits a CAPTCHA or similar check:

1. Chat or the Web tab shows **Medousa needs help with a verification**.
2. Choose **Open in Web** (if needed), complete the check yourself.
3. Choose **Continue agent** so the turn resumes.

Do not paste passwords into chat to “help” — use the human browser surface. Deeper browser docs land in a later chapter; for stuck turns see [Troubleshooting](guide:troubleshooting#browser-captcha--verification).

## Runtime Controls (day-one)

**Settings → Runtime Controls** shapes what tools can do on this workshop. Spotlight: **Runtime controls**.

| Band | What to know |
|------|----------------|
| **Reach** | **Tool posture**, **Specialists**, web search provider, **Tool rounds** defaults |
| **Shell** | **Agent shell tools** on/off, **Network ceiling**, timeouts, max output |
| **Allowed tools** | Module allowlist — **empty means the full catalog is allowed** |
| **Allowlists** | Binary allowlist for shell commands — empty + shell on is a warning sign |
| **Engine** | Memory backend and diagnostics |

Day-one safe defaults:

1. Leave shell **off** until you need it.
2. If you enable shell, set a **binary allowlist** — do not leave it empty.
3. Prefer a tight **module allowlist** when experimenting with new specialists or MCP.
4. Raise tool rounds only when you understand the task; prefer budget prompts over a huge default.

MCP servers and packages have their own Settings sections (desktop). Misconfigured MCP shows up as missing tools — [Troubleshooting](guide:troubleshooting#mcp-unavailable).

## Where safety settings live

| Concern | Settings section |
|---------|------------------|
| Models / API keys | Medousa Agent → Models / Providers |
| Tool posture, shell, allowlists | Runtime Controls |
| Shared seats / pairing exposure | Sharing |
| Optional binaries / offline brain | Packages |
| External tool servers | MCP |

Next: [Work and background jobs](guide:work-jobs) · [Troubleshooting](guide:troubleshooting)
