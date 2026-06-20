# Themes & multi-daemon — incremental plan

> **Status:** In progress (Themes T1–T2) · Multi-daemon M0 next  
> **Date:** 2026-06-19  
> **Goal:** Familiar palettes for VS Code/Obsidian adopters + Slack-style multi-workshop connections

---

## Product framing

| Concept | Meaning | User mental model |
|---------|---------|-------------------|
| **Workshop / server** | A Medousa Engine instance (URL + pairing trust) | “Personal” vs “Acme Corp team” |
| **Profile** | Identity lane on *one* engine (`user:work`, `user:default`) | “Which hat on *this* workshop” |
| **Theme** | Client shell appearance (localStorage) | “How the room looks” |

**Do not conflate profiles with servers.** Profiles answer *who* on an engine; servers answer *which* engine.

---

## Part A — Color themes

### Market research (2025–2026)

Sources: VS Code Marketplace install trends, Obsidian community downloads (2026 roundups).

| Theme family | Why include | Medousa id |
|--------------|-------------|------------|
| **GitHub Themes** | #1 VS Code installs (~19M) | `github` |
| **One Dark Pro** | Atom heritage, ~12M installs | `one-dark` |
| **Catppuccin Mocha/Latte** | Fastest-growing community palette; Obsidian top tier | `catppuccin` |
| **Tokyo Night** | ~2.7M installs, strong night-desk crowd | `tokyo-night` |
| **Dracula** | ~10M installs, classic | `dracula` (T3) |
| **Obsidian Minimal / Flexoki** | Obsidian-native familiars | `flexoki` (T4, optional) |

**Existing Medousa originals (keep):** Obsidian (medousa), Black Lily, Cupertino, Graphite, Midnight.

### Phases (each builds on the last)

#### T1 — Single source of truth ✅ (this PR)

- [x] `src/lib/theme/themeRegistry.ts` — ids, Skeleton names, options, validators, boot script builder
- [x] Vite `transformIndexHtml` injects boot script from registry (no triple-sync with `app.html`)
- [x] `colorThemes.ts` / `themeResolve.ts` re-export from registry

#### T2 — Editor familiars ✅

- [x] Skeleton themes: One Dark, Catppuccin, Tokyo Night, GitHub (dark + light pairs)
- [x] Settings **Room** → new group “Editor familiars”
- [x] Smoke: dark/light toggle per palette, FOUC-free boot

#### T3 — Depth catalog ✅

- [x] Dracula, Nord, Solarized Dark (+ light pairs)
- [x] Token-only — no new `app.postcss` overrides for familiars

#### T4 — Polish & per-workshop theme ✅

- [x] Per-workshop default theme (`clientState.colorThemeId` + apply on switch)
- [x] Settings Room saves palette for active workshop
- [ ] Accent-only picker (Obsidian-style) — stretch / deferred

### Technical notes

- Palettes use `themes/theme-utils.ts` + `surface-scales.ts` ramps.
- New themes register in `tailwind.config.ts` Skeleton `custom` array.
- Boot keys unchanged: `medousa-home-dark-mode`, `medousa-home-color-theme`.

---

## Part B — Multi-daemon (Slack-style workshops)

### Product promise

**One app. Multiple workshops.** Scan a company QR → second engine appears in the switcher. Personal daemon stays local; team daemon is paired remote (LAN + Iroh).

### Architecture (v1)

```
Server registry (persisted)
  └── WorkshopServer { id, label, kind, url, pairing? }

Active pointer: active_workshop_id

One active connection at a time:
  select server → reconnectWorkshop(server) → reload stores
```

- **One workspace SSE** per active server (defer multi-live streams).
- **Profiles** loaded from active server only.
- **Vault** tree is per-server (switch = different library).

### Phases

#### M0 — Design lock ✅

- [x] [ADR-003](../../../docs/architecture/decisions/adr-003-multi-workshop-connections.md): server vs profile, credential storage, one-active-SSE
- [x] `WorkshopRegistry` schema — `src/lib/types/workshopRegistry.ts` + `workshops.schema.json`
- [x] UX wireframes: switcher, add-workshop, mid-turn switch confirm (in ADR-003)

#### M1 — Registry + switcher ✅

- [x] Persist N servers; default “Personal” on first run (`workshop_registry.rs` + migration)
- [x] Reuse `ProfileSwitcherCompact` pattern → `WorkshopSwitcherCompact.svelte`
- [x] `selectWorkshop()` → `workshops_set_active` + `reconnectWorkshop()`
- [x] Tauri: multi-credential store (`pairing_client.rs` per-workshop paths + session tokens)
- [x] Settings → Connection workshops list (`SettingsWorkshopsSection.svelte`)
- [x] Mid-turn / dirty vault switch confirm

#### M2 — QR join ✅

- [x] “Add workshop” → paste / scan pairing link → registry entry (`WorkshopJoinSheet.svelte`)
- [x] “Switch now?” after pair (`pendingSwitchAfterPair` prompt)
- [x] Mobile wizard auto-switches to paired workshop on first setup

#### M3 — Slack polish (partial ✅)

- [x] Connection dot per workshop in switcher list (active workshop health)
- [x] Rename/remove, last connected timestamp in Settings
- [x] Per-workshop last chat session (`clientState.lastSessionId` + restore on switch)
- [x] Status bar shows active workshop when multiple saved
- [ ] Turn tags include `workshop_id` in UI (defer — no cross-workshop turns in v1)

#### M4 — Team / enterprise ✅

- [x] Server branding — icon, accent color, tagline in registry + Settings
- [x] Invite rotation — `POST /qr/rotate` + “Rotate invite” in Phone pairing
- [x] Per-server theme default (shared with T4)

#### M5 — Optional multi-live

- [ ] Background health on inactive servers — defer until needed

### Reuse map

| Existing | Multi-daemon use |
|----------|------------------|
| `reconnectWorkshop()` | Switch target |
| `pairing_client.rs` + QR v2 | Join workshop |
| `ProfileSwitcherCompact` | Workshop switcher shell |
| `SettingsBasementSection` connection card | Manage workshops |
| `workshop_transport` route cache | Invalidate on switch |

---

## Recommended sequence

```
Themes T1 → T2 → T3 ✅
Multi-daemon M0 ADR ✅
Multi-daemon M1 ✅  (registry + switcher)
Multi-daemon M2 ✅  (QR join + switch prompt)
Multi-daemon M3     (polish — mostly done)
Multi-daemon M4 ✅  (branding + invite rotation)
Themes T4 ✅        (per-workshop room theme)
```

---

## References

- `apps/medousa-home/src/lib/theme/themeRegistry.ts`
- `apps/medousa-home/src/lib/types/workshopRegistry.ts`
- `docs/architecture/decisions/adr-003-multi-workshop-connections.md`
- `architecture/iroh-p2p-pairing-plan.md`
- `docs/architecture/decisions/adr-002-user-profiles.md`
