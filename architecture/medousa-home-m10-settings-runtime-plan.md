# Medousa Home — M10 Plan (Settings & Workshop Controls)

> **Status:** **M10a–M10e shipped** in repo (2026-06-07)  
> **Date:** 2026-06-07  
> **Epic:** **M10 — Settings premium + workshop controls home**  
> **Builds on:** M8 (Real Garage), shipped M7 product surfaces (Cron, Messaging, Skills)  
> **Related:** [medousa-home-product-ux-plan.md](medousa-home-product-ux-plan.md), [medousa-home-polish-plan.md](medousa-home-polish-plan.md)

## North star

Settings should feel like **System Settings on a device you paid for** — status first, instant toggles, no TUI dump. Workshop tuning belongs under **Runtime**, not buried in a scroll of checkboxes.

**Mantra:** Home prefs here. Workshop tuning there. Plumbing in the basement.

**Steve test (epic exit):** Operator opens Settings, knows connection + theme in 2 seconds, never sees “Verifier min supported claim ratio” unless they opened Runtime → Workshop on purpose.

---

## The critique M10 answers

| Steve says | Today | M10 response |
|------------|-------|--------------|
| Am I connected? | Raw URL + Save & test | Status pill + human label; edit URL on demand |
| Why is this a form? | Vertical stack of `workshop-inset` cards | Left nav + one section at a time |
| Where’s Messaging/Cron? | Text links in “Related views” | Integrations hub rows with live status |
| Why 40 fields in Settings? | `WorkshopDefaultsPanel` in Settings scroll | Move to Runtime → **Workshop** tab |
| Dark mode + 8 themes? | Redundant checkbox | Theme cards apply light/dark pair; mode toggle stays explicit |

---

## Boundary rules

| Surface | Owns |
|---------|------|
| **Settings** | Connection (status-first), appearance, Home behavior toggles, integrations hub, advanced basement (files + diagnostics) |
| **Runtime** | Telemetry (Now, Jobs, Delivery), quick model/depth (**Controls**), full `tui_defaults.json` (**Workshop**), stage routing read-only (**Routing**) |
| **Messaging / Cron / Skills** | Unchanged — Settings links in, never duplicates |

---

## Epic overview

| Phase | Name | Theme | Exit criterion |
|-------|------|-------|----------------|
| **M10a** | Settings shell | Master-detail nav | One section visible; no infinite form scroll |
| **M10b** | Connection + summary strip | Status-first | Connected/offline without reading a URL |
| **M10c** | Workshop relocation | Runtime → Workshop tab | Zero workshop defaults in Settings |
| **M10d** | Integrations hub | Live rows | Messaging + Cron status on row |
| **M10e** | Advanced basement | Single door | Files + diagnostics merged; dev vault dev-only |

**Next epic (not M10):** **Context view** — read-only STTP + identity memories (simple here; full experience in Resonantia).

---

## M10a — Settings shell

### Layout (desktop)

```text
┌─────────────────────────────────────────────────────────┐
│ Settings                                                │
│ Connected · Obsidian dark · 3 cron active               │
├──────────────┬──────────────────────────────────────────┤
│ General      │  (active section content)              │
│ Appearance   │                                          │
│ Connection   │                                          │
│ Integrations │                                          │
│ Advanced     │                                          │
└──────────────┴──────────────────────────────────────────┘
```

### Touch

| File | Role |
|------|------|
| `SettingsPanel.svelte` | Shell + summary strip + section router |
| `settings/SettingsNav.svelte` | Left nav / mobile segment |
| `settings/SettingsGeneralSection.svelte` | Activity + notifications |
| `settings/SettingsAppearanceSection.svelte` | Theme grid + dark mode |
| `settings/SettingsConnectionSection.svelte` | Status-first connection |
| `settings/SettingsIntegrationsSection.svelte` | Hub rows |
| `settings/SettingsAdvancedSection.svelte` | Files + diagnostics |
| `types/settings.ts` | `SettingsSectionId` |
| `app.postcss` | `.settings-shell`, `.settings-nav`, `.settings-hub-row` |

---

## M10b — Connection

- **Default view:** pill (Connected / Offline), backend whisper, “Local workshop” vs “Remote workshop” from URL host
- **Edit:** “Change connection…” reveals URL field + Save & test (unchanged API)
- Remove engineer copy (“running workshop backend”)

---

## M10c — Workshop relocation

- Add `RuntimeTab`: `"workshop"`
- Embed existing `WorkshopDefaultsPanel.svelte` under Runtime → Workshop (desktop + mobile)
- Remove from `SettingsPanel.svelte`
- Mobile You hub: drop separate **Advanced** destination; workshop defaults via Runtime → Workshop
- Update Runtime Controls copy: point to Workshop tab, not Settings

---

## M10d — Integrations hub

Rows (tappable, chevron):

| Row | Status source |
|-----|----------------|
| Messaging | Channel connected count from `messaging` store |
| Cron jobs | `recurring.activeCount()` |
| Runtime health | `health.ok`, optional “Open telemetry →” |

---

## M10e — Advanced basement

Single nav item containing:

- Collapsible on-disk files (`product_config.json`, …) — unchanged behavior
- Diagnostics dl (revision, worker, tools) — unchanged fields
- Dev-only: developer vault notes + garage wizard reset

---

## Doc updates (parallel)

| Doc | Change |
|-----|--------|
| `medousa-home-product-ux-plan.md` | Mark M7 surfaces shipped; Settings = M10 |
| `medousa-home-polish-plan.md` | P2.5 theme preview — satisfied by Appearance section |

---

## Document history

| Date | Change |
|------|--------|
| 2026-06-07 | M10 plan — Settings premium + Runtime workshop relocation |
