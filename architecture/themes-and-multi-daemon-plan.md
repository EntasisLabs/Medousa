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

#### T4 — Polish & optional per-server theme

- [ ] Accent-only picker (Obsidian-style) — stretch
- [ ] Per-workshop default theme (requires Part B M1 registry)

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

#### M0 — Design lock (next epic)

- [ ] ADR: server vs profile, credential storage, one-active-SSE
- [ ] `WorkshopServer` schema + file path (`workshops.json`)
- [ ] UX wireframes: switcher, add-workshop, mid-turn switch confirm

#### M1 — Registry + switcher

- [ ] Persist N servers; default “Personal” on first run
- [ ] Reuse `ProfileSwitcherCompact` pattern → workshop switcher
- [ ] `reconnectWorkshop(serverId)` keyed selection
- [ ] Tauri: multi-credential store (extend `pairing_client.rs`)

#### M2 — QR join (marketing demo)

- [ ] “Add workshop” → scan QR v2 → new registry entry
- [ ] Optional “Switch now?” after pair

#### M3 — Slack polish

- [ ] Connection badge per server, rename/remove, last session per server
- [ ] Turn tags include `workshop_id` in UI

#### M4 — Team / enterprise

- [ ] Server branding, invite rotation, per-server theme default

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
Themes T1 → T2 → T3     (parallel-safe, low risk)
Multi-daemon M0 ADR     (1 session, design lock)
Multi-daemon M1         (registry + switcher)
Themes T4 per-server    (after M1)
Multi-daemon M2 QR join (demo-ready pitch)
```

---

## References

- `apps/medousa-home/src/lib/theme/themeRegistry.ts`
- `architecture/iroh-p2p-pairing-plan.md`
- `docs/architecture/decisions/adr-002-user-profiles.md`
