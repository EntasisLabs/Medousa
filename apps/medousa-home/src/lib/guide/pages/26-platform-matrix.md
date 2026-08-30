# Desktop, web, and phone

What each way of running Medousa can do.

- **Desktop app** — full workshop on your computer.
- **Web** — browser window connected to a workshop address.
- **Phone** — embedded Personal workshop, with optional portals into other workshops.

Related: [Getting started](guide:getting-started) · [Sharing and phone](guide:sharing-phone)

## Capability matrix

| Capability | Desktop app | Web | Phone |
|------------|-------------|-----|-------|
| Chat and notes with a workshop | Yes | Yes | Yes |
| Install Offline brain / packages | Yes | No | No |
| Install MCP / external tools | Yes | Use desktop | Hosted HTTP/SSE MCP in Personal; desktop for local binaries |
| Built-in browser | Yes | Limited | Yes (mobile Web) |
| Pop-out windows | Yes | No | No |
| Change models & tool safety | Yes | Limited | Yes in Personal; follows the selected portal when remote |
| Shared mode / seat invites | Yes (host) | No | Pair on the computer |
| Phone pairing QR | Yes | No | No |
| Start workshop at login | Yes when offered | No | No |
| Pin local folders | Yes | Limited | Personal uses private app storage; portals use the host's files |
| Push / Live Activity | — | — | In Preferences when available |

## Simple picture

```
Phone app ───── embedded Personal (notes, models, mobile tools)
     │
     └──── optional portal ──── Desktop workshop (its files and tools)

Web browser ───── connected to a selected workshop address
```

## Tips

1. Install packages and Offline brain on the **desktop**.
2. Pair a phone only when you want it to open a different workshop.
3. Use the desktop for advanced tool safety changes.
4. Web is fine for light chat and notes when the workshop is reachable.

Next: [Where your data lives](guide:data-lifecycle) · [FAQ and limits](guide:faq-limits).
