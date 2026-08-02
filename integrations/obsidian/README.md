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

## Current slice

- native Obsidian chat view;
- daemon session restoration and new conversations;
- bounded active-note, selection, and outgoing-link context;
- streaming responses with sequence-aware reconnects;
- visible tool/recovery status and explicit budget/permission prompts;
- no silent note mutations.

Search, backlinks, synthesis, note creation, append/link previews, and
conflict-aware writes are follow-on phases.
