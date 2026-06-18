# Medousa Home — Messaging Polish Plan

> **Status:** **Shipped** (2026-06-07)  
> **Epic:** Hermes-parity Messaging surface  
> **Related:** [medousa-home-product-ux-plan.md](medousa-home-product-ux-plan.md) M7a

## North star

Messaging is its **own comms identity** — not Settings, not JSON files. Operators configure channels without reading `product_config.json`.

**Hermes test:** Search channel → pick Telegram → see status badges → guided credentials → Required / Recommended / Advanced → Save.

## Design principle

**Structure from Hermes. Skin from Medousa. Life from the theme.**

- Master-detail + Required/Recommended/Advanced — yes
- Competitor brand hex (Telegram blue, etc.) — no
- **Theme-native channel accents** — each channel maps to a workshop palette slot (tertiary / secondary / primary / success) so icons feel alive and adapt to Obsidian, Black Lily, Cupertino, etc.
- **Global theme vitality** — slightly brighter primary + surface text; subtle Obsidian radial wash

## Shipped

| Area | Change |
|------|--------|
| **Layout** | Master-detail: searchable channel list + detail pane (mobile: list → detail with back) |
| **List** | Brand-colored icons, status dots (live / configured / needs setup) |
| **Detail hero** | Channel tagline + status badges (Needs setup, Configured, Live, Workshop offline) |
| **Guided setup** | Credentials section + external setup guide links |
| **Form structure** | Required · Recommended · Advanced (collapsible) |
| **Copy** | Human taglines per channel; secrets never shown, “Stored securely” when set |
| **Components** | `MessagingChannelList`, `MessagingChannelDetail`, slim `MessagingPanel` |

## Not in this pass

- Per-channel enable/disable toggle (no backend field yet)
- Live gateway telemetry beyond workshop health
- Email / SMS / additional Hermes channels (future comms expansion)
- Channel icons as brand SVGs (Lucide + brand colors for now)

## Verification

```bash
cd Medousa/apps/medousa-home && npm run check && npm run build
```

Manual: Open Messaging → Telegram shows badges + setup guide → save token + allowlist → list dot turns green when daemon live.
