# Medousa for Obsidian

Medousa for Obsidian is a vault-native companion. It brings the Medousa session
and interaction model into Obsidian without recreating Home's entire shell.
Obsidian supplies the active note and editor context; the workshop daemon
remains authoritative for Medousa sessions, vault search, reads, and writes.

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
- **Medousa: Open in Medousa Home** hands advanced workshop, automation, and
  artifact work to the full Medousa surface.
- **Medousa: Ask about current note** opens the view and sends a note-aware
  prompt.
- **Medousa: Ask about selection** sends the active editor selection with the
  current note context.
- **Medousa: New conversation** creates a new daemon-owned session.
- **Medousa: Configure connection** changes the endpoint and supplies an
  in-memory token for the current Obsidian session.
- **Medousa: Search Medousa vault** searches daemon-managed notes and lets you
  open a result or insert a wikilink into the current Markdown editor.
- **Medousa: Show backlinks for current note** explores the notes that link to
  the active note.
- **Medousa: Save last answer as note** and **Medousa: Append last answer to
  current note** open an explicit preview before writing.
- **Medousa: Generate daily synthesis** and **Medousa: Generate weekly
  synthesis** start focused vault synthesis prompts.

Click the conversation title in the chat header to search, switch, rename,
delete, or start conversations. Once a reply settles, its **Copy**, **Save as
note**, and **Append to note** actions appear beneath the answer.

## Note safety

The plugin never silently mutates a note. New notes use create-only daemon
writes. Appends re-read the target and send its `content_hash` as `If-Match`, so
a note changed in the meantime fails safely with a refresh message. Wikilink
insertion is a deliberate editor action in Obsidian and is separate from
daemon-authoritative note writes.
