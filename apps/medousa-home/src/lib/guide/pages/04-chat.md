# Chat

Type at the bottom, press Enter, and Medousa answers. This chapter covers sending messages, models, conversations, and what happens during a long reply. Approvals: [Permissions](guide:permissions-budgets). Background work: [Work](guide:work-jobs).

## Composer

- Type naturally; Enter sends (Shift+Enter for a new line, depending on preferences).
- **+** opens **Attach**, **Profile**, and **Agent** (on mobile, **Workshop** may also appear).
- Model picker (when enabled in Preferences) sits in the composer chin — name + chevron.
- Mic (dictation) and send sit on the right.

On an empty session you may see a quiet presence prompt above the composer. It is not a second app — just an invitation to begin.

### Attachments

| Rule | Detail |
|------|--------|
| Types | Images, PDF, CSV/TSV, text/Markdown, Excel, Word |
| Limits | Up to **5** files per message; **25 MB** each |
| Upload | Chips show while pending; **+** disabled during upload |
| Images | Need a **Vision model** in Settings → Models — otherwise send is blocked |

Unsupported types get a plain hint to try PDF, images, spreadsheets, or text.

### Voice / dictation

- Requires the **Medousa desktop app** (not browser-only preview).
- Needs a working **Dictation model** (STT) on the workshop and mic permission.
- Flow: mic → record → transcript appended to the draft; cancel restores the previous draft.
- Common failures: workshop software too old for dictation, mic unavailable, no speech detected, empty recording.

### Context chips

When a vault note, passage, or script is attached as context, a chip appears above the composer. Clear it when the turn should not see that note. Context also affects the usage meter after a turn.

```chips
- Note context | tone: accent
Passage | tone: default
Script | tone: warn
```

## Model picker

Open the model control to search and pick. Rows show name, quiet meta (provider · pricing · context when known), and a **Vision** mark when relevant. Favorites sort first. **Add models** / **Open Models** jumps to Settings.

Full assignment lives in **Settings → Medousa Agent → Models**:

| Profile | Role |
|---------|------|
| **Chat model** | Main replies |
| **Vision model** | Images |
| **Dictation model** | Mic / STT |

Optional **fallbacks** (backup 1 & 2) and **Stages & providers** (API keys, stage routes) live on the same tab. Changes apply immediately on the workshop. On phone, model picks are usually **host-managed**.

Preferences → **Model picker** toggles whether the chin control appears at all.

## Under the box: runtime and turn controls

Below the composer (main Chat surface — not embedded vault chat):

```chips
- Runtime | tone: accent | value: runtime
Voice | tone: default | value: voice
Stance | tone: default | value: stance
Reasoning | tone: warn | value: reasoning
```

1. **Runtime** — who runs the turn:
   - **Medousa** — native turns
   - **Cursor** — external Cursor agent
   - **Codex** — external Codex agent
2. **Voice** — personality / voice preset
3. **Stance** — Concise / Standard / Deep
4. **Reasoning** — effort for models that honor it

Runtime always shows a chevron. Voice, Stance, and Reasoning stay quieter — chevrons on hover or when open.

```callout
tone: tip
title: Change instruments, not every message
body: Pick a Voice and Stance for a stretch of work. Revisit Reasoning when the task gets hard or expensive.
```

## Slash commands

Type `/` in the composer for hints. Spotlight mirrors several of these.

```chips
- /ask | tone: accent
/budget | tone: warn
/usage | tone: default
/help | tone: default
```

| Command | Effect |
|---------|--------|
| `/ask …` | Start **background** work (also Spotlight **Background task**) |
| `/budget` / `/budget list` | Pending tool-round approvals |
| `/budget approve [id]` | Grant more tool rounds |
| `/budget deny [id]` | Stop that turn’s budget request |
| `/usage` / `/context` | Open context usage panel |
| `/help` / `/commands` / `/?` | Command hints |

Background asks show cues like “Working in background” and “see Work”. Details: [Work and background jobs](guide:work-jobs).

## Thinking, tools, and long turns

While a turn runs:

- **Thinking** stays collapsed by default — expand for the trace.
- Tool calls stream as they happen.
- Status bar activity shows motion.

You may be asked to **Allow** / **Deny** a tool permission, **Approve** / **Deny** more tool rounds, or help with a **web verification**. See [Permissions, budgets, and tool safety](guide:permissions-budgets).

**Steer:** during a workshop handoff, the composer can say **Steer the handoff…** — your next message continues the worker.

**Cancel:** there is no always-visible Cancel button on every runtime. If a turn feels stuck, check connection health, Work (blocked cards), pending budgets, and browser verification before rewriting the prompt. Restarting the engine (Settings → Workshop) pauses active chats.

## Offline gate

When the workshop is offline, chat shows a full-screen gate instead of the composer:

- Desktop: **Start Medousa** / **Fix and start** / **Restart**, **Connection settings**, engine log
- Phone: cannot reach Medousa on your computer yet
- Browser preview: run the desktop app to chat

## Context usage

After a turn, the **context usage** ring opens a token breakdown. Use `/usage` or Spotlight **See context usage**. Until you send a message, there may be no snapshot yet.

## Sessions

Sidebar:

| Action | Notes |
|--------|--------|
| Search | Filter sessions |
| Pin / unpin | Pinned section at top |
| Rename | Dialog on the row |
| Delete | Removes transcript, catalog entry, and Locus memory — irreversible |
| **New shared room** | Requires [Shared mode](guide:architecture#shared-mode); sessions show a **Room** badge |

**Export** (Spotlight): Markdown download, PDF preview, JSON (debug).

Use panes or pop-outs when two threads need to stay visible — [Navigation and surfaces](guide:navigation-surfaces).

## Artifacts and Liquid chat

- Turns can produce an **artifact strip** — inline, panel, or fullscreen presentation.
- Cleanup: Settings → Medousa Agent → **Presentations cleanup** (keep-for days, max per session).
- Experimental **Liquid chat** (scene renderer): Settings → Preferences → **Liquid chat**.

## Related settings

| Topic | Where |
|-------|--------|
| Models, stance, presentations | Settings → Medousa Agent |
| Model picker visibility, Liquid chat | Settings → Preferences |
| Tool posture, shell, allowlists | Settings → Runtime Controls — [Permissions chapter](guide:permissions-budgets) |
| Themes | [Themes and customization](guide:themes-customization) |

Next: [Permissions, budgets, and tool safety](guide:permissions-budgets), or [Work and background jobs](guide:work-jobs).
