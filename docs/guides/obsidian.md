# Medousa for Obsidian

Medousa for Obsidian is a vault-native companion. The first development slice
opens a native chat view, restores a daemon-owned conversation, and includes a
bounded snapshot of the active Markdown note, selection, and outgoing links.
It does not silently modify notes.

## Install a development checkout

Obsidian plugins are loaded from a vault's `.obsidian/plugins` directory. Use a
separate development vault while building the plugin:

```bash
cd integrations/obsidian
npm install
npm run build
mkdir -p "/path/to/dev-vault/.obsidian/plugins/medousa"
cp main.js manifest.json styles.css "/path/to/dev-vault/.obsidian/plugins/medousa/"
```

In Obsidian, reload the app, enable **Medousa** under **Community plugins**, and
run **Medousa: Open chat** from the Command Palette.

## Connect a workshop

The default workshop is `http://127.0.0.1:7419`. Change the endpoint under
**Settings → Community plugins → Medousa**. The local development slice keeps a
remote bearer token in memory only; it is not written into vault plugin data.

## Use the companion

- **Medousa: Open chat** opens the native view.
- **Medousa: Ask about current note** opens the view and sends a note-aware
  prompt.
- **Medousa: Ask about selection** sends the active editor selection with the
  current note context.
- **Medousa: New conversation** creates a new daemon-owned session.
- **Medousa: Configure connection** changes the endpoint and supplies an
  in-memory token for the current Obsidian session.

The first slice is deliberately read-and-chat focused. Search, backlinks,
note creation, append, link insertion previews, synthesis, and conflict-aware
mutation are the next Obsidian phases.
