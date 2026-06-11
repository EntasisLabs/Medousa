# Medousa Home — Product UX Plan (M7)

> **Status:** **M7 surfaces shipped** — Settings charter = [M11 plan](medousa-home-m11-settings-charter-plan.md) (supersedes [M10](medousa-home-m10-settings-runtime-plan.md))  
> **Date:** 2026-05-30  
> **Related:** [medousa-home-main-workspace-plan.md](medousa-home-main-workspace-plan.md), [medousa-home-plan.md](medousa-home-plan.md)  
> **Trigger:** Hermes comparison — messaging settings, cron, skills, tools

---

## The speech Steve would give us

You built a **Ferrari engine**: Surreal-backed runtime, turn-worker bus, verifier, identity graph, OpenShell sandbox, MCP gateway, recurring delivery, stage routing, continuation ledger.

Hermes built a **rental-car dashboard**: Python scripts, simple cron, channel tokens in forms.

And they're **winning the room** — not because their engineering is better, but because their UI **finishes the sentence**:

- *"Connect Telegram."* — not *"Open `product_config.json`."*
- *"What runs at 8am?"* — not *"Check Runtime → Schedule tab."*
- *"What can I turn on?"* — not *"Expand Registry entry."*

**We are giving a world-class agent a world-average experience.** That stops now.

**Principle (unchanged):** `medousa_daemon` owns truth. Home is the product layer — it reads and writes the same files and APIs as TUI/CLI, but **never asks the operator to leave the app to configure the app**.

---

## Competitive gap (honest)

| Area | Hermes | Medousa Home today | Our engine advantage they lack |
|------|--------|-------------------|-------------------------------|
| **Messaging** | Channel list → detail pane, masked secrets, Connected/Credentials badges | Workshop files → Open path in editor | Multi-channel ingest policy, delivery outbox, heartbeat, identity-aware ingest |
| **Cron** | First-class list: search, 7/11 active, + New, last/next, pause | Buried in Runtime → Schedule; create only from Skills card | Agent-turn recurring, manuscript binding, delivery resolve, scheduler tick in stats |
| **Skills** | Catalog: search, categories, enable toggles, 155 rows | One hero card per skill, Run/Schedule stack | Manuscript catalog, OpenShell sandbox, `agent_turn` execution, skill-import |
| **Tools** | Toolsets tab, descriptions, toggles | Title + collapsed "Registry entry" | Capability manifest, MCP + Grapheme bindings, policy profiles |

**We don't need their stack. We need their shape** — object-first, list → detail, status on the row — wired to **our** durable runtime.

---

## Design rules (M7+)

1. **Nouns, not surfaces** — Cron Job, Channel, Skill, Tool — not "Runtime tab", "Workshop files", "Registry entry".
2. **List → detail** — master pane + inspector; no endless scroll of inset cards.
3. **Status on the row** — Connected, Paused, Enabled, Next run — never buried in `<details>`.
4. **Secrets in-app** — mask, save, keychain/file backends — same as TUI; paths only in Advanced/Diagnostics.
5. **One create path per object** — + New cron, + Connect channel — not "schedule from skill card only".
6. **Shared config** — all writes go to the same on-disk / daemon stores TUI uses (`product_config.json`, `tui_defaults.json`, secrets dir, `/v1/recurring`).
7. **Density** — rows, not hero cards; whispers, not banners (carry forward M6 visual pass).

---

## Navigation target

```text
┌────┬──────────────────┬─────────────────────────────────────────┐
│Icon│ Master list      │ Detail / inspector                      │
│    │                  │                                         │
│ M  │ (context varies) │                                         │
│ ⌂  │                  │                                         │
│ 💬 │                  │                                         │
│ 📖 │                  │                                         │
│ ⚡ │ Skills | Tools   │ Skill detail / Tool bindings            │
│ 📅 │ Cron jobs        │ Job editor (cron, prompt, manuscript)   │
│ ▦  │ Work             │                                         │
│ 📡 │ Runtime          │                                         │
│ 💬*│ Messaging        │ Telegram / Discord / … detail           │
│ ⚙  │ Settings         │ Home-only prefs + link to files (adv)  │
├────┴──────────────────┴─────────────────────────────────────────┤
│ Connected · N cron active · delivery ok · tick · N in motion      │
└───────────────────────────────────────────────────────────────────┘
```

`*` Messaging may live under Settings or as its own nav item — **recommended: own nav item** (Hermes parity, high operator value).

**Status bar additions:** `N cron active` (links to Cron), optional `gateway ready` whisper.

---

## M7a — Messaging & product settings

### Problem

Settings shows **file paths** and "Open" — correct architecture, wrong product. Channels (Telegram, Discord, Slack, WhatsApp) are invisible until you edit JSON.

### North star

Hermes-style **Messaging** surface:

- Left: searchable channel list with icon, name, status dot (Connected / Not configured / Paused).
- Right: channel detail — guided setup, masked credentials, allowed users, enable toggle, Save.

### Backend (already exists)

| Store | Path / API | Contents |
|-------|------------|----------|
| Product config | `~/.local/share/medousa/product_config.json` | `telegram`, `discord`, `slack`, `whatsapp`, `daemon`, `runtime`, `identity` |
| Secrets | `~/.local/share/medousa/secrets/*` + keychain | Bot tokens (same as TUI `session.rs`) |
| Ingest policy | `product_config` + daemon | `ingest_sender_allowed` |

Home already has: `medousa_config_paths`, `openPath`, `load_tui_defaults` / `persist_tui_runtime_prefs` (Tauri).

### Work

| # | Task | Exit |
|---|------|------|
| M7a.1 | Tauri: `load_product_config_summary` + `save_product_config_partial` (channel slices only) | Read/write Telegram/Discord/Slack/WhatsApp fields without clobbering whole file |
| M7a.2 | Tauri: secret helpers — `secret_status`, `save_secret`, `clear_secret` (reuse keychain-first pattern from `session.rs`) | "Credentials set" without exposing token |
| M7a.3 | `MessagingPanel.svelte` — channel list + detail layout | Telegram end-to-end: allowed user IDs, heartbeat flags |
| M7a.4 | Channel health — probe via `GET /v1/health` + channel-specific doctor hints | Row shows Connected / Needs setup |
| M7a.5 | Demote **Workshop files** to Settings → Advanced | Primary settings = objects; files = escape hatch |
| M7a.6 | Settings split: **Home** (appearance, notifications, connection URL) vs **Messaging** nav | No more scroll of everything |

### API gaps (daemon — optional M7a.7)

| Endpoint | Purpose |
|----------|---------|
| `GET /v1/product-config` (redacted) | Channel config without secrets for remote clients |
| `POST /v1/product-config/channel` | Validated partial update |

Prefer Tauri local read/write first (same machine as daemon) — matches current Home deployment.

---

## M7b — Cron workspace

### Problem

Cron is a **side effect** of Skills and a **sub-tab** of Runtime. Operators cannot answer: *what runs, when, is it paused?*

### North star

Hermes-style **Cron jobs** surface (first-class nav or status-bar link):

- Header: `Search cron jobs…`, **`N/M active`**, **+ New cron**
- Rows: title, Scheduled/Paused pill, origin (Skill / Chat / Manual), prompt excerpt, cron expression, **Last / Next**, `…` menu
- Detail / modal: name, cron, timezone, prompt, optional manuscript, model hint, enable, Save

### Backend (already exists)

| API | Today |
|-----|-------|
| `GET /v1/recurring` | List definitions — **shipped M6d** |
| `POST /v1/recurring/prompt` | Register — **shipped** |
| `recurring.svelte.ts` | Store — list + register only |

### Work

| # | Task | Exit |
|---|------|------|
| M7b.1 | `CronPanel.svelte` — list view (replace Runtime → Schedule as primary) | Search, active count, sorted by next run |
| M7b.2 | **+ New cron** flow — not tied to Skills card | Create from prompt + cron + optional manuscript |
| M7b.3 | Row actions: pause/resume, delete (confirm) | Operator controls lifecycle |
| M7b.4 | Status bar: `N cron active` → opens Cron | Always visible accountability |
| M7b.5 | Decouple Skills **Schedule** → opens Cron detail prefilled with manuscript | Skill schedules jobs; Cron owns the list |
| M7b.6 | Human titles — `display_name` on register or prompt-first line as title | Rows scannable like Hermes |

### API gaps (daemon — M7b.7)

| Endpoint | Purpose |
|----------|---------|
| `PATCH /v1/recurring/{id}` | Enable/disable, update cron, prompt, manuscript |
| `DELETE /v1/recurring/{id}` | Remove job |
| Optional `display_name` on `RecurringDefinitionEntry` | List title |

Until PATCH exists: Tauri may call stasis SDK via new daemon routes — **do not** fork store in Home.

---

## M7c — Skills catalog

### Problem

Skills panel is a **work order form** (big card, Run/Schedule buttons, hidden cron in `<details>`). Hermes treats skills as a **browsable library** with search and toggles.

### North star

- Tabs: **Skills** | (Tools → M7d)
- Search + filter chips: All, Runnable, Sandbox, Imported, …
- Dense rows: name, one-line description, badges (sandbox, scripts), actions: **Run**, **Schedule…**, **Open**
- Detail pane (optional): full description, scripts, schedule link → M7b

### Backend (already exists)

| API | Today |
|-----|-------|
| `GET /v1/manuscripts` (catalog) | `catalog_list_manuscripts` — **shipped** |
| `POST /v1/recurring/prompt` | Schedule with `manuscript_id` + `agent_turn` |
| Chat `/skill {id}` | Run via composer — **shipped** |

### Work

| # | Task | Exit |
|---|------|------|
| M7c.1 | Refactor `SkillsPanel` → master list (no hero cards) | 10+ skills scannable without scroll fatigue |
| M7c.2 | Search + `skillsOnly` filter as chips | Find skill in <3s |
| M7c.3 | Row actions: Run → chat draft; Schedule → Cron detail | Actions on row, not vertical button stack |
| M7c.4 | Remove inline cron `<details>` from skill card | Scheduling lives in M7b |
| M7c.5 | Category/group headers from `scope` or manuscript prefix | DATA-SCIENCE-style sections (our metadata) |

### API gaps

None required for browse/run. Optional: `enabled` flag per manuscript if we want Hermes-style toggles (would need daemon manuscript preferences — **defer** unless product demands disable).

---

## M7d — Tools catalog

### Problem

Tools section is four cards with **Registry entry** dropdowns — capability IDs with no operator meaning.

### North star

- **Tools** tab (with Skills): search, grouped by domain or `effect_class`
- Rows: title, description, binding summary (MCP server / Grapheme), read-only or policy badge
- Detail: bindings list, invoke policy, link to `capabilities.toml` in Advanced

### Backend (already exists)

| API | Today |
|-----|-------|
| `GET /v1/capabilities` | List with `title`, `binding_count` |
| `~/.config/medousa/capabilities.toml` | Operator bindings |
| `CapabilityRegistry` | Rich manifest in daemon |

### Work

| # | Task | Exit |
|---|------|------|
| M7d.1 | Extend catalog Tauri/daemon response with `description`, `bindings[]` summary | Rows show more than id |
| M7d.2 | Tools tab — dense list, search | Hermes Toolsets parity (read-only first) |
| M7d.3 | Tool detail — MCP vs Grapheme, allowed lanes, effect class | Operator understands what tool does |
| M7d.4 | "Edit bindings" → open `capabilities.toml` (Advanced) until in-app editor ships | Honest escape hatch |

### API gaps (optional M7d.5)

| Endpoint | Purpose |
|----------|---------|
| `GET /v1/capabilities/{id}` | Full manifest + bindings for detail pane |

---

## M7e — Settings cleanup (carry-over)

| # | Task | Exit |
|---|------|------|
| M7e.1 | Settings = Home-only + Connection + Diagnostics | No duplicate Runtime controls (link to Runtime nav) |
| M7e.2 | Workshop files → Advanced accordion only | Primary UX is in-app objects |
| M7e.3 | `tui_defaults.json` writes remain on Runtime/Settings model apply | Shared with TUI — **shipped** |

---

## Implementation order

```text
M7b Cron list     ──┐  (highest visibility gap; status bar)
M7c Skills catalog ├── parallel after M7b list pattern established
M7d Tools catalog ──┘
M7a Messaging     ──── largest Tauri/config work; ship Telegram first
M7e Settings trim ──── ongoing as surfaces land
```

**Recommended first slice:** **M7b.1 + M7b.4** — Cron nav + list + status bar whisper. Proves object-first pattern before messaging config depth.

---

## Success metrics

1. Operator configures **Telegram allowed users** without opening an editor.
2. Operator sees **all cron jobs** in one list with last/next — never visits Runtime → Schedule for routine work.
3. Operator finds and **runs a skill** from a searchable list in &lt;10 seconds.
4. Operator understands **what a tool does** without reading a registry id.
5. Side-by-side with Hermes: **same mental model**, visibly deeper runtime (agent_turn, delivery, sandbox badges).
6. README / onboarding: Home is the product; TUI is terminal advanced mode (M6f).

---

## Files (anticipated)

| Area | New / major touch |
|------|-------------------|
| Nav | `NavSidebar.svelte`, `ui.ts` — add Cron, Messaging |
| Cron | `CronPanel.svelte`, `cron.svelte.ts` (or extend `recurring.svelte.ts`) |
| Messaging | `MessagingPanel.svelte`, `product_config.rs` (Tauri), `types/product.ts` |
| Skills | `SkillsPanel.svelte` refactor |
| Tools | `ToolsPanel.svelte` or tab in Skills |
| Settings | `SettingsPanel.svelte` slim down |
| Shell | `WorkshopShell.svelte`, `StatusBar.svelte` |
| Daemon | `recurring_handlers.rs` — PATCH/DELETE if needed |
| Plan | `medousa-home-main-workspace-plan.md` — M7 reference |

---

## Document history

| Date | Change |
|------|--------|
| 2026-05-30 | M7 product UX plan — messaging, cron, skills, tools; Hermes competitive closure |
