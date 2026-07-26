# Chat

Chat is the primary loop: you speak, Medousa thinks and acts, you steer.

## Composer

The composer sits at the bottom of chat:

- Type naturally; Enter sends (Shift+Enter for a new line, depending on preferences).
- **+** opens profile, agent, and attach actions.
- Model picker (when enabled) sits in the composer chin — name + chevron only.
- Mic and send sit on the right.

On an empty session you may see a quiet presence prompt above the composer. It is not a second app — just an invitation to begin.

## Model picker

Open the model control to search and pick a model. Rows show:

- Model name
- A quiet meta line (provider · pricing · context) when the catalog knows it
- A Vision mark when relevant

**Add models** at the foot jumps to Settings → Agent. The picker itself stays focused on choosing a model — turn knobs live under the box.

## Under the box: runtime and turn controls

Below the composer:

1. **Runtime** (Medousa / Cursor / Codex) — who runs the turn.
2. **Voice** — personality preset.
3. **Stance** — Concise / Standard / Deep.
4. **Reasoning** — effort for models that honor it.

Runtime always shows a chevron. Voice, Stance, and Reasoning stay quieter — chevrons appear on hover or when open — so the row doesn't look like a settings toolbar.

```callout
tone: tip
title: Change instruments, not every message
body: Pick a Voice and Stance for a stretch of work. Revisit Reasoning when the task gets hard or expensive.
```

## Thinking and tools

While a turn runs:

- **Thinking** stays collapsed by default — expand when you care about the trace.
- Tool calls stream as they happen.
- Status bar activity shows that something is in motion.

Cancel or steer according to the runtime — native Medousa turns and external agents behave differently. If a turn feels stuck, check connection health before rewriting the prompt.

## Sessions

Sessions live in the chat sidebar. Name them when a thread becomes a project. Use panes or pop-outs when two threads need to stay visible.

## Related settings

- Providers and favorites → Settings → Agent — [Learn more path from Settings](guide:chat)
- Themes that affect chat chrome → [Themes and customization](guide:themes-customization)

Next: [Themes and customization](guide:themes-customization), or jump to [Vault and notes](guide:vault-notes).
