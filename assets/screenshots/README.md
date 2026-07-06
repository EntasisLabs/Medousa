# README & marketing screenshots

**Source of truth for product shots.** Commit PNGs here, then link from `README.md` as `assets/screenshots/<file>.png`.

After capture, sync to the landing repo:

```bash
cp assets/screenshots/*.png ../medousa-landing/public/assets/screenshots/
# Story shots → ../medousa-landing/public/assets/story/
```

## Capture

From **medousa-landing** (daemon on `:7419`, app dev on `:1420`):

```bash
cd ../medousa-landing
npm run assets:capture
```

Or manual export at **1440×900** @2x PNG, Obsidian dark theme (`data-theme="medousa"`).

---

## README set (current)

| File | Surface | In README |
|------|---------|-----------|
| `chat.png` | Chat | Thinking trace + memory |
| `vault.png` | Vault | Split editor / bonsai note |
| `presentation.png` | Library → Presentations | Voidsurge sandbox artifact |
| `canvas.png` | Canvas | Vibe Studio widgets |
| `automations.png` | Automations | Flows + Plan steps |
| `peers.png` | Peers | Inbox + workshop chat |
| `pairing.png` | Peers → Add peer | QR LAN invite |
| `channels.png` | Messaging | Telegram + WhatsApp configured |
| `identity.png` | Context → You | Identity canvas |
| `themes.png` | Settings → Room | Color palettes |
| `settings-memory.png` | Settings → Memory | Charter + long-chat rules |
| `settings-models.png` | Settings → Models | Add favorite providers |
| `settings-reach.png` | Settings → Reach | Tools, search, specialists |
| `capabilities.png` | Capabilities | MCP gateway |
| `tui.png` | Engine | TUI setup (developers section) |

## Optional (builders / engine page — not README hero)

| File | Surface |
|------|---------|
| `web.png` | Web — tabs + save-to-vault affordance |
| `capabilities.png` | Capabilities — skill catalog |
| `context.png` | Context → Threads (skip Map if sparse) |
| `work.png` | Work — **only** if board has cards in flight |

## Drop (stale or redundant)

| Old shot | Why |
|----------|-----|
| `maps.png` / context map | Redundant with threads; map fidelity varies |
| Runtime as “work” | Was mislabeled; use Automations or real Work board |
| `settings.png` (Connection) | Redundant with pairing unless showing tunnel |
| `skills.png` alone | Fold into capabilities or drop if duplicate |
| `trace.png` crop | Bake trace into `chat.png` instead |

## Landing sync

Landing `public/assets/screenshots/` uses kebab-case keys in `constants.js` (`new-chat.png`, etc.). Either rename on copy or keep both naming schemes — README uses simple names above.
