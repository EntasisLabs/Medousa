# Spotify & Apple Music widgets

Medousa can pin **native** Spotify and Apple Music players on custom views — not inside HTML artifact iframes (those block nested embeds).

Normie guide: [Custom views & canvas](custom-views-and-canvas.md)

---

## Ask Medousa

Examples:

- “Add my focus playlist to Writing Studio — https://open.spotify.com/playlist/…”
- “Put this Apple album on my dashboard: https://music.apple.com/…/album/…”

Medousa creates a `media_embed` component with provider `spotify` or `apple_music` and a validated HTTPS embed URL.

---

## Share links

| Service | What to paste |
|---------|----------------|
| **Spotify** | Playlist, album, or track share link, or `https://open.spotify.com/embed/…` from Share → Embed |
| **Apple Music** | `music.apple.com` share link, or embed URL from **Copy Embed Code** (`embed.music.apple.com`) |

The app normalizes share URLs to embed URLs client-side; the engine rejects unknown hosts.

---

## Playback notes

- **Login** — Spotify and Apple may require you to sign in inside the embed for full playback (not just previews).
- **Previews** — Some tracks are preview-only unless you have a subscription.
- **Mobile** — Players follow each service’s mobile embed rules; keep the widget on a dashboard layout for best fill.

---

## Not supported in HTML artifacts

Do **not** ask Medousa to paste `<iframe src="open.spotify.com/…">` into widget HTML. Use `media_embed` instead.

Future: optional `config.hidden` and a `MedousaWidget` controls API for play/pause without showing the player — not available yet.
