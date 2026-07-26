# Getting started

Think of Medousa as a bench: the **workshop** is the machine under the bench, **Home** is the shell you stand in, and **chat + vault** are the two tools you reach for first.

## The pieces

- **Home app** — the desktop shell (this window, status bar, rails, pop-outs).
- **Workshop / engine** — the daemon that runs models, tools, sessions, and vault IO.
- **Vault** — your notes and artifacts for that workshop.
- **Chat** — turns with an agent that can read the vault and call tools.

## First connection

1. Open **Settings → Workshop**.
2. Confirm you have an active workshop (often *This device* on a local engine).
3. Wait for the status bar connection mark to go healthy.
4. Open **Chat** from the rail and send a short message.

If the link fails, stay on Workshop status — address, engine health, and the connection runbook live there. See [Workshops and connections](guide:workshops-connections).

## The daily loop

1. **Ask** in chat — steer with Voice / Stance / Reasoning under the composer when it matters.
2. **Keep** what matters in the vault — notes, boards, sheets.
3. **Return** via surfaces and panes — don't reopen the whole app for one note.

```callout
tone: tip
title: Sticky note + chat pop-out
body: Pop chat and a note into their own windows when you are deep in a task. The desktop toolbar can summon them without hunting the main window. See Navigation and surfaces.
```

## Profiles and agents

- **User profile** — who you are to the workshop (preferences, identity).
- **Agent** — how Medousa answers (prompt, tools, voice presets).

You can switch either from the composer **+** menu without leaving chat. Deep model and provider setup lives in Settings → Agent; the guide chapter [Chat](guide:chat) covers the turn-time controls.

## Common first-day mistakes

| Mistake | Fix |
|---------|-----|
| Typing while offline | Check Workshop status before rewriting prompts |
| Fighting the empty chat hero | Start a message — or continue where you left off |
| Ignoring the active workshop | Wrong workshop ⇒ wrong vault and sessions |
| Over-tuning every turn | Leave Voice / Stance / Reasoning alone until a turn needs it |

Next: [Navigation and surfaces](guide:navigation-surfaces).
