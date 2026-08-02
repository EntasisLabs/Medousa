# Medousa for Obsidian

The Obsidian adapter is a vault-native companion, not a copy of Medousa Home.
It uses the daemon-owned session and the shared `@medousa/client` package while
capturing bounded note context from Obsidian.

## Development

Use a separate development vault. Obsidian's official plugin workflow loads the
compiled `main.js`, `manifest.json`, and `styles.css` from
`.obsidian/plugins/medousa/`.

```bash
npm install
npm run build
mkdir -p "/path/to/dev-vault/.obsidian/plugins/medousa"
cp main.js manifest.json styles.css "/path/to/dev-vault/.obsidian/plugins/medousa/"
```

Enable **Medousa** in Obsidian's Community plugins settings, then use the
Command Palette to run **Medousa: Open chat**.

The default workshop is `http://127.0.0.1:7419`. Configure another endpoint or
an in-memory bearer token with **Medousa: Configure connection**. The token is
not persisted in vault plugin data by this development slice.

Use the Medousa daemon endpoint, not the MCP gateway (`:7420`) or local
inference endpoint (`:7421`). The connection modal's **Test connection** action
checks the exact endpoint and token before saving.

## Current slice

- native Obsidian chat view;
- daemon session restoration, conversation history, switching, naming, deletion,
  and new conversations;
- bounded active-note, selection, and outgoing-link context;
- streaming responses with sequence-aware reconnects;
- visible tool/recovery status and explicit budget/permission prompts;
- settled-answer actions for copy, save-as-note, and append-to-note;
- daemon-backed vault search and backlinks with open-note and insert-link
  actions;
- daily and weekly synthesis prompts that can be saved through the same preview;
- explicit note previews, create-only writes, append writes guarded by
  `If-Match`, and no silent note mutations.

All Medousa-managed reads and writes go through the workshop daemon. Obsidian's
editor remains the host-native place for inserting a wikilink, while note
creation and append operations show their target and content before applying.
An `If-Match` conflict leaves the note untouched and asks the user to refresh
the preview.

## Commands and interactions

- **Open chat** opens the native chat view. Click the conversation title to
  search, switch, rename, delete, or start a conversation.
- **Open in Medousa Home** hands advanced workshop, automation, and artifact
  work to the richest Medousa surface.
- **Ask about current note** and **Ask about selection** seed a turn with the
  bounded note context shown in the context line.
- **Search Medousa vault** searches note titles and content through the daemon.
  Open a result or insert a wikilink into the current Markdown editor.
- **Show backlinks for current note** lists notes that link to the active note.
- A settled answer exposes **Copy**, **Save as note**, and **Append to note**.
  Save and append both require a visible path/content preview.
- **Generate daily synthesis** and **Generate weekly synthesis** start focused
  vault synthesis turns; the settled answer can then use the same save flow.
