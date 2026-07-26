# Desktop, web, and phone

What each shell can do. **Desktop Tauri** is the full host. **Web** is a browser client to a workshop URL. **Phone** is a companion portal into a host workshop.

Related: [Getting started](guide:getting-started) · [Sharing and phone](guide:sharing-phone) · [Settings reference](guide:settings-reference)

## Capability matrix

| Capability | Desktop | Web | Phone |
|------------|---------|-----|-------|
| Chat / vault against a workshop | Yes | Yes (daemon URL) | Yes (paired host) |
| Local engine / Offline brain install | Yes | No | No |
| Packages install | Yes | Stub only | Stub only |
| MCP gateway + servers | Yes | Use desktop | Use desktop |
| Human Browser (embedded) | Yes | No native embed | Yes (mobile Web) |
| Pop-outs (chat, note, web, guide, views) | Yes | No | No |
| Runtime → **Routing** tab | Yes | If Runtime exposed | Hidden / limited |
| Edit Agent / Runtime Controls | Yes | Charter save needs Tauri host | **Read-only** (host-managed) |
| Shared mode toggle / seat QR | Yes (host) | No | No (pair on host) |
| Phone pairing QR mint | Yes | No | No |
| Login autostart (engine) | Yes when supported | No | No |
| Pin local folders / reveal in Finder | Yes | Limited | Files live on **host** |
| Remote push / Live Activity | — | — | Preferences → Everyday |
| Content zoom (whole UI) | Yes (shortcuts) | No-op | No-op |
| Welcome wizard (full) | Yes | Limited | Connect-only path |

## Mental model

```
Desktop host ──engine──► vault, models, tools, pairing secrets
     ▲
     │ workshop URL / QR
Web client ──────────────┘
Phone companion ─────────┘  (portal, not a second brain)
```

## Operator tips

1. Install packages, MCP, and Offline brain on the **desktop host**.
2. Treat phone as a **portal** — teach identity and pair seats from the host when using Shared mode.
3. Prefer desktop for shell-enabled specialists and allowlist changes.
4. Web is fine for light chat/notes against a reachable workshop; it is not a substitute for Packages.

Next: [Data and recovery](guide:data-lifecycle) · [Known limits and FAQ](guide:faq-limits).
