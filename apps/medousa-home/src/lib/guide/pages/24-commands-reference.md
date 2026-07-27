# Commands and keyboard reference

Full list of keyboard shortcuts and Spotlight commands. Shortcuts can’t be remapped in Settings — these bindings are fixed.

Related: [Keyboard and flow](guide:keyboard-flow) · [Chat](guide:chat) · [Browser and web research](guide:browser)

```callout
tone: note
title: Prefix chord
body: Pane bindings use a prefix — ⌘; on macOS, Ctrl+; elsewhere — then the key (for example ⌘; % to split right). Spotlight always opens with ⌘K / Ctrl+K.
```

## Keyboard shortcuts

### Global

| Action | macOS | Windows / Linux |
|--------|-------|-----------------|
| Open Spotlight | ⌘K | Ctrl+K |
| Toggle left rail | ⌘B | Ctrl+B |
| Summon view toolbar | ⇧⌘. | Ctrl+Shift+. |
| Zoom in / out | ⌘+ / ⌘− | Ctrl++ / Ctrl+− |
| Reset zoom | ⌘0 | Ctrl+0 |
| Open keyboard shortcuts | ⌘; ? | Ctrl+; ? |

### Panes

| Action | macOS | Windows / Linux |
|--------|-------|-----------------|
| Split right | ⌘; % | Ctrl+; % |
| Split down | ⌘; " | Ctrl+; " |
| Focus pane | ⌘; h/j/k/l | Ctrl+; h/j/k/l |
| Zoom pane | ⌘; z | Ctrl+; z |
| Close pane (merge tabs) | ⌘; x | Ctrl+; x |
| Chat tab here | ⌘; c | Ctrl+; c |
| Next / prev tab | ⌘; n/p | Ctrl+; n/p |
| Show tabs | ⌘; w | Ctrl+; w |
| Switch virtual desktop | ⌘; 1–4 | Ctrl+; 1–4 |
| Move tab to another pane | Drag tab | Drag tab |

### Vault

| Action | macOS | Windows / Linux |
|--------|-------|-----------------|
| Save note | ⌘S | Ctrl+S |
| Find in note | ⌘F | Ctrl+F |
| New note | ⌘N | Ctrl+N |
| Toggle edit / preview plane | ⇧⌘E | Ctrl+Shift+E |
| Export PDF | ⇧⌘P | Ctrl+Shift+P |
| Toggle board | ⇧⌘B | Ctrl+Shift+B |

### Chat / Spotlight

| Action | macOS | Windows / Linux |
|--------|-------|-----------------|
| Spotlight (commands & jumps) | ⌘K | Ctrl+K |
| Keyboard shortcuts sheet | ⌘; ? | Ctrl+; ? |

Browser chords (also in Spotlight when Web is focused) include address bar, new/close/reopen tab, bookmarks, find, and open external — see Spotlight list below and [Browser](guide:browser).

## Composer slash commands

| Command |
|---------|
| `/ask …` — background job |
| `/budget list` — pending round approvals |
| `/budget approve [id]` — grant more tool rounds |
| `/budget deny [id]` — stop the turn |
| `/usage` — context window / token breakdown |
| `/help` — show commands |

## Spotlight — Go destinations

| Destination | Subtitle |
|-------------|----------|
| Chat | Talk with Medousa |
| Workspace | Notes, files, scripts, agents, and flows |
| Work | Tasks and kanban board |
| Browser | Built-in web workshop |
| Automations | Scripts and schedules |
| Agents | Specialist agents in Workspace |
| Map | Sessions, moments, and notes |
| Peers | Nearby workshops and inbox |
| Profiles | People and identity |
| Engine status | Jobs, delivery, health |
| Settings | Preferences and connection |
| Channels | Telegram, Discord, Slack — Settings → Sharing |
| MCP connections | Manage MCP servers in Settings → MCP |

## Spotlight — commands

Contextual commands (rename desktop, per-desktop switch, etc.) appear when relevant. Static catalog:

| Command | Notes |
|---------|-------|
| Rename workspace | Rename “…” |
| Split pane right | … — TMUX-style vertical split |
| Split pane down | … — TMUX-style horizontal split |
| Focus pane left | … — move focus left |
| Focus pane down | … — move focus down |
| Focus pane up | … — move focus up |
| Focus pane right | … — move focus right |
| Zoom pane | … — maximize / restore active pane |
| Zoom in | … — whole UI, like the browser |
| Zoom out | … — whole UI, like the browser |
| Reset zoom | … — back to 100% |
| Close pane | … — merge tabs into the nearest pane |
| New chat in pane | … — open chat tab in the active pane |
| Next tab in pane | … — cycle tabs forward |
| Previous tab in pane | … — cycle tabs backward |
| Show pane tabs | … — briefly reveal the tab strip |
| Keyboard shortcuts | … — panes, vault, global binds |
| Toggle left rail | … — show or hide the master rail |
| Summon view toolbar | … — compact toolbar at the cursor (or shake the mouse) |
| Toggle desktop toolbar | Floating chat / note / web / views strip |
| Open Operator's Guide | In-app manual for navigation, chat, themes, and more |
| Focus address bar | … — select the URL / search field |
| New browser tab | … — open a blank tab |
| Open bookmarks | … — history, bookmarks, and library saves |
| Find in page | … — search text on the current page |
| Open in browser | Navigate to a URL from the clipboard |
| Reopen closed tab | Restore the last closed browser tab |
| Copy browser link | Copy the current tab URL |
| Open markdown file… | Edit a single .md without adding a vault folder |
| Write a new message | Jump to chat composer |
| Start fresh conversation | New chat session |
| Background task | Medousa works while you keep chatting |
| Change model | Medousa Agent — models & stages |
| Runtime controls | Reach, shell, budgets & diagnostics |
| Voice and stance | Medousa Agent — stance & depth |
| Show chat slash commands | Operator shortcuts in chat |
| List budget approvals | Pending tool-round extensions |
| Check daemon health | Connection and backend status |
| Check for updates | Desktop app vs release channel |
| List skills | Imported skill manuscripts |
| Edit stage routes | Stage models in Settings → Medousa Agent |
| Export this conversation | Download session history as Markdown |
| Export conversation as PDF… | Preview and save a PDF of this chat |
| Export conversation as JSON | Raw session history for debugging |
| See context usage | Token breakdown for the last turn |
| New note | Create a note in the Library |
| New chat | Start a fresh conversation |
| New blank script | Open Scripts workbench |
| Run script… | Pick a saved script by name |
| Morning brief | Queue the morning-brief manuscript |
| Toggle Live / Build | Switch note plane |
| Toggle Preview / Edit | Note editor mode |
| Toggle split preview | Build pane beside source |
| Toggle links panel | Wikilinks and backlinks |
| Zoom in | Whole UI larger |
| Zoom out | Whole UI smaller |
| Reset zoom | Back to 100% |
| Resume last Spotlight | Restore previous search (Telescope-style) |
| Clear all pins | Empty the working set |

---

*This list matches the current app. When something is missing from Spotlight, search by name — contextual commands appear only when they apply.*
