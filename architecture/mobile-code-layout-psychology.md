# Mobile Code layout psychology

> **Status:** Proposed (locked 2026-08-12)
>
> **Scope:** How a neovim/VS Code lifer who now mostly reviews agents moves
> through the mobile Code workspace — rooms, jumps, chrome budget, and a
> Termius-class PTY. Not a new information architecture.
>
> **Companion to:** [Mobile Code workspace](mobile-code-workspace-plan.md)
>
> **Related:** [Home Code workbench parity](home-code-vscode-parity-plan.md),
> [Code flow-state roadmap](code-flowstate-roadmap.md), and
> [Coding session terminal](coding-session-terminal.md)

## Decision

The locked workspace IA is the right house. This document constrains **how you
move through it** so M1 does not ship a segmented control that still feels like
a compressed desk.

Neovim and VS Code are not chrome to copy. They are mental models:

- **Neovim = presence.** You are in the buffer. Navigation is a verb (`gf`,
  jumplist, picker). Chrome is not the product. Latency to “I am looking at the
  thing” is the whole game.
- **VS Code = a house.** Explorer, editor, terminal, and SCM live in stable
  rooms. You always know which room you are in. The activity bar is jobs, not
  decoration.

Mobile Code honors both: a house of four rooms with zero ceremony once you are
inside a room. The person who got used to agents doing the coding opens a
project to see what happened, then sits down at a real desk only when a hunk is
wrong.

## Why the current surface fights both models

`MobileCodePanel` mounts the desktop `UndertakingsPanel` / `CodeSourceEditor`
stack. Nested toolbars mean you cannot find the rooms *and* you cannot just be
in the file.

Desktop editor chrome makes this worse on a phone by mixing **buffer verbs**
(back/forward, find, save) with **room switches** (Changes, Terminal, Review)
on one toolbar. That mix is tolerable on a wide desk, where regions have
spatial addresses. On a phone it is three competing mental models.

## Layout contract

```text
Project list
    └── Project house
            Files (picker) ──jump──► Editor (presence)
            Changes (newspaper) ──jump──► Editor
            Terminal (glass) ──gf──► Editor
            Files / Editor / Terminal / Changes  = sibling rooms
            Thread = door out to Chat, restore the same room on return
```

### 1. Rooms are siblings; going into a file is a stack

The surface switcher is the VS Code activity bar distilled to four jobs:
Files, Editor, Terminal, Changes. Switching rooms is not a push. You are not
going deeper; you are changing rooms.

Opening a file, a diff hunk, or a Terminal path (`gf`) is a Neovim jump. That
*is* a push. Back pops to the place you jumped from.

Hardware / back-swipe order:

1. Close a detail inside the active surface (sheet, find UI, session picker,
   diff).
2. Pop a jump (Editor opened from Files, a hunk, or `gf`).
3. Leave the project for the project list.

A sibling room switch does not invent a fake “return to Files” unless Files was
the actual jump origin. “I switched Editor → Terminal in the switcher” returns
to Editor, not Files.

Motion: room switches use the short sibling transition language. Jumps use the
existing push/pop stack. Both honor `prefers-reduced-motion`.

### 2. Split chrome by job, or the buffer dies

**Buffer chrome** (Editor only): jumplist back/forward, which-file (`:ls` as a
sheet from `codeWorkspace.orderedTabsFor`), dirty/save, find, overflow
(problems, outline, selection → agent).

**Room chrome:** the four-job surface switcher. Changes, Terminal, and Files
are not Editor toolbar buttons.

**Door chrome:** Thread lives on the project header / `MobileTopChrome` trailing
actions (same pattern as Notes’ `noteChat`). Thread leaves the house; it is not
a room you inhabit. Review stays a decision you take from Changes, not a sixth
switcher item.

Layout budget, matching Scripts’ mobile editor:

- `MobileTopChrome` (safe area + project-mode actions)
- One compact project identity row (title, phase/executor, overflow)
- Canvas
- Surface switcher — or the Terminal key row while the keyboard is up

No inner desktop toolbar under that. Code project mode extends `MobileTopChrome`
the way `script-editor` already does. Three chrome bands plus a switcher kill
the buffer.

### 3. Agent-native landing

The reviewer does not open a project to type. They open it to see what happened,
then crack at a mistake.

Landing, in order:

1. Project has attention (dirty working copy, agent just finished, review
   available) → **Changes**
2. Else an already-open file → **Editor**
3. Else → **Files**

Editor is always one tap from a hunk, a file row, a Terminal path, or the
switcher. It must feel like sitting down at a real desk (syntax theme, real
save, jumplist, conflict), not like entering a mini-IDE.

### 4. Files is a picker, not a place you live

Neovim does not live in a tree. VS Code’s explorer is how you *arrive*. On a
phone the tree is the fallback.

- Default filter when the project has changes: **Changed**
- Otherwise: **Recent**
- Full tree only when you do not know the name
- Tap a row → you are in the buffer. No ceremony.

### 5. Editor is presence

The canvas already feels like home (`CodeMirrorHost` + the selected code syntax
theme). Protect that:

- Maximum canvas; chrome recedes.
- Jumplist is sacred and thumb-reachable.
- Open buffers are a sheet, not a tab strip eating rows.
- Problems, outline, and references are sheets — VS Code regions as drawers.
- Selection → agent uses overflow or long-press, then Thread as the door,
  carrying path, line, and selection.

The crack-at-code loop: tap hunk → land on the line → edit a few lines → save →
back to the same diff position.

### 6. Terminal vs Termius

Termius (the mobile SSH client people mean by “terminus”) wins because the glass
*is* the app: full-bleed PTY, accessory keys, keyboard geometry is sacred,
reconnect does not kill scrollback, it looks like a unix box.

Do not compete on hosts, keys, SFTP, or snippet libraries. Medousa’s PTY is
already authenticated and project-bound. Beat Termius on the thing it cannot
do: this is the same shell the agent is on, in the same worktree as the buffer
and the diff.

Termius-class feel (maps onto workspace slices M3/M4; does not add scope):

- Full-height xterm. The 13rem dock is why it feels like a toy.
- Key row as good as Termius: Escape, Tab, Ctrl latch with visible state,
  arrows, Enter, paste, dismiss, interrupt. 44px targets, haptic. Replaces the
  switcher while the keyboard is up.
- The `vim` test is the credibility test. If Esc/Ctrl/rows are not enough to
  survive `vim`, it is a log viewer.
- Font size readable on a phone. Desktop `TerminalPane` at 12px is too small
  for a full-glass surface. Pinch-zoom is later, not an M3 blocker.
- Session sheet = this project’s shells + New shell. Never a host inventory.
- Output paths are `gf` into Editor.
- Reconnect keeps the xterm buffer. Termius users will rage if scrollback dies.
- Quiet “agent on this session” when a peer is attached — capability, not lease
  vocabulary.
- `terminalThemeFor(...)` so it looks like their box, not a hard-coded violet
  toy.

## Out of scope

Vim modal in CodeMirror, pixel-for-pixel VS Code chrome, Termius host/key/SFTP
features, embedding Chat in Code, a sixth Review switcher item, and pinch-zoom
as an M3 blocker.
