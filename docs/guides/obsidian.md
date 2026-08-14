# Medousa for Obsidian

Medousa for Obsidian is a vault-native companion. It brings the Medousa session
and interaction model into Obsidian without recreating Home's entire shell.
Obsidian supplies the active note and editor context; the workshop daemon
remains authoritative for Medousa sessions, vault search, reads, and writes.
That note context is attached as structured turn metadata rather than inserted
into the visible chat message.

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
Protected routes require that paired bearer for local loopback workshops too;
loopback alone does not identify the plugin.

The plugin needs the Medousa daemon endpoint, not the MCP gateway (`:7420`) or
the local inference endpoint (`:7421`). In Medousa Home, use the active
workshop's **Connection** details when the workshop is remote or uses a custom
port. The connection dialog includes **Test connection** before saving.

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

Assistant replies use Obsidian's Markdown renderer and also support portable
Liquid Markdown cards, charts, reports, tabs, slides, and actions. Vault-relative
media stays vault-aware. Home-only chart editing and live feed loading remain in
Medousa.

## Note safety

The plugin never silently mutates a note. New notes use create-only daemon
writes. Appends re-read the target and send its `content_hash` as `If-Match`, so
a note changed in the meantime fails safely with a refresh message. Wikilink
insertion is a deliberate editor action in Obsidian and is separate from
daemon-authoritative note writes.

## Troubleshooting connection

The chat view shows the endpoint it is trying to reach. Use **Retry** after
starting the daemon, or **Configure** to correct the endpoint/token. From a
terminal, the local default can be checked with:

```bash
curl -i http://127.0.0.1:7419/health
```

An HTTP `401` or `403` means the endpoint is reachable but needs the current
bearer token. A connection refusal or fetch failure means the daemon is not
listening at that address, so copy the active workshop address from Medousa
Home or start the local daemon.
