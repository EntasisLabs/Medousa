# Operator’s Guide — docs epic

> **Status:** Active (2026-07) — **D0–D2 shipped**; next **D3 Reference polish**  
> **Audience:** Operators using Medousa Home (desktop, web, phone companion)  

> **Thesis:** The in-app Operator’s Guide ships as **orientation**. The gap is **operator reference depth** — procedures, state models, failure modes, security implications, and platform differences — so a new operator can run the product without rediscovering UI by accident.  
> **Product content:** [`apps/medousa-home/src/lib/guide/`](../apps/medousa-home/src/lib/guide/) (`catalog.ts` + `pages/*.md`)  
> **Gap audit canvas:** [operators-guide-gap-audit](file:///Users/theelevators/.cursor/projects/Users-theelevators-medousa/canvases/operators-guide-gap-audit.canvas.tsx) (Cursor canvas; open beside chat)

**Related:** [polish-and-package-plan.md](polish-and-package-plan.md) (felt polish F-phases — parallel, not a substitute) · [ROADMAP.md](ROADMAP.md)

---

## Reality check

The guide window, TOC, Spotlight entry, status-bar entry, and Settings “Learn more” links are **shipped**. **Twenty-four** chapters exist after D2 — major surfaces now have operator homes. Remaining gap is mostly **D3** polish: generated catalogs, settings reference matrix, recipes collection, What’s new, a11y depth, and governance.

| Group | Chapters today |
|-------|----------------|
| Start | Welcome, Architecture, Getting started |
| Workspace | Navigation, Chat, Permissions, Work, Browser, Calendar, Profiles/Locus, Themes |
| Craft | Vault, Recovery, Liquid, Views, Automations, Specialist agents |
| System | Workshops, Keyboard, Sharing/phone, Messaging, Runtime, MCP/packages, Troubleshooting |

Integrator / engine docs stay under [`docs/`](../docs/). Do **not** invent a parallel operator-UI tree there.

---

## North star

> A layered documentation system inside Home: concepts → task how-tos → complete UI references → security guidance → troubleshooting → generated catalogs.  
> Microsoft Learn–grade for operators — not a longer welcome pamphlet.

---

## Principles

1. **Structure over length** — Do not lengthen every chapter uniformly. Add chapters and split sets when a topic needs a mini-manual.
2. **Operator-manual voice** — Procedures, controls, state, failure modes, platform differences. Prefer “how / when / what happens if” over metaphor-only orientation.
3. **Truth to the UI** — Document what Home actually exposes (surfaces, settings, commands). When UI and docs drift, fix docs in the same train or flag the epic.
4. **Cross-links** — Use `[Label](guide:chapter-id)` and `#anchors`. Callouts use fenced ` ```callout ` blocks (not Liquid fences).
5. **Generated appendices** — Keyboard, Spotlight, and slash inventories should eventually be generated from `keyboardShortcutsCatalog.ts` and `commands/registry.ts`, not hand-copied forever.
6. **Ship D0 → D1 first** — Foundations and core loop unblock every later chapter. Prefer those packages when context is short.
7. **Update this epic** — When a chapter or package ships, tick checkboxes and add a working-log line in the same change set.

---

## Phase map

```mermaid
flowchart LR
    D0[D0 Foundations]
    D1[D1 Core loop]
    D2[D2 Capability depth]
    D3[D3 Reference polish]

    D0 --> D1 --> D2 --> D3
```

| Phase | Theme | Status |
|-------|--------|--------|
| D0 | Foundations — mental model, first-run, surface inventory, glossary | ✅ |
| D1 | Core loop — chat, permissions/budgets, Work, troubleshooting | ✅ |
| D2 | Capability depth — vault, browser, automations, identity, peers, views, runtime, MCP, Liquid | ✅ |
| D3 | Reference polish — generated catalogs, settings ref, matrix, recipes, governance | ⬜ |

---

## D0 — Foundations

**Goal:** Operators share one mental model and can find every first-class surface.

**Done when:**
- Architecture / terminology chapter exists (workshop vs peer vs phone portal; profile vs specialist; state scopes).
- Getting started matches the real wizard / first-run / reconnect paths (and how to rerun).
- Navigation chapter includes a real surface inventory (incl. Library and Automations modes, panes/tabs/desktops, mobile More hub).
- Glossary covers the terms operators hit in the shell.

### Packages

- [x] **D0.1** Platform mental model and terminology — `guide:architecture` (`01-architecture-terminology.md`)
- [x] **D0.2** Complete first-run and connection guide — expanded `guide:getting-started`
- [x] **D0.3** Navigation and surface inventory rewrite — expanded `guide:navigation-surfaces`

---

## D1 — Core loop

**Goal:** Day-one chat and background work are operable and safe.

**Done when:**
- Chat documents composer, attachments, models/routing, context, sessions/export, offline, long-running turns.
- Permissions, tool posture, budgets (`/budget`), and browser verification have an operator home.
- Work board lifecycle (`/ask`, cancel, blocked, retention) is documented.
- Troubleshooting decision tree covers connection, chat, tools, browser challenge, schedules, vault, pairing.

### Packages

- [x] **D1.1** Chat operating manual — expanded `guide:chat`
- [x] **D1.2** Permissions, budgets, and safe tool use — `guide:permissions-budgets`
- [x] **D1.3** Work and background jobs — `guide:work-jobs`
- [x] **D1.4** Troubleshooting decision tree — `guide:troubleshooting`

---

## D2 — Capability depth

**Goal:** Major missing surfaces and systems have task-based chapters, not one-liners.

**Done when:** Each package below has a dedicated chapter (or mini-set) with procedures and failure modes.

### Packages

- [x] **D2.1** Vault documentation set — expanded `guide:vault-notes`
- [x] **D2.2** Vault recovery and versions — `guide:vault-recovery`
- [x] **D2.3** Browser and web research — `guide:browser`
- [x] **D2.4** Automations set — expanded `guide:grapheme-automations`
- [x] **D2.5** Specialist agents and skills — `guide:specialist-agents`
- [x] **D2.6** Profiles, identity, and Locus — `guide:profiles-locus`
- [x] **D2.7** Phone, peers, and shared mode — expanded `guide:sharing-phone`
- [x] **D2.8** Messaging channels — `guide:messaging-channels`
- [x] **D2.9** Views, canvas, and environments — expanded `guide:views-environments`
- [x] **D2.10** Runtime telemetry — `guide:runtime-telemetry`
- [x] **D2.11** MCP and packages — `guide:mcp-packages`
- [x] **D2.12** Liquid authoring reference — `guide:liquid-reference`
- [x] **D2.13** Calendar and `.ics` — `guide:calendar`

---

## D3 — Reference polish

**Goal:** Drift-resistant catalogs and cross-cutting ops docs.

**Done when:** Appendices are maintainable; settings and platform matrices exist; governance checklist is written.

### Packages

- [ ] **D3.1** Generated keyboard + Spotlight + slash appendix
- [ ] **D3.2** Settings reference (scope / default / platform / restart / risk)
- [ ] **D3.3** Desktop / web / phone capability matrix
- [ ] **D3.4** Data locations, backup, migration, retention
- [ ] **D3.5** Themes, accessibility, reduced motion
- [ ] **D3.6** Operator recipes / runbooks
- [ ] **D3.7** FAQ, known limits, What’s new / compatibility
- [ ] **D3.8** Documentation governance (feature → doc checklist, release gating)

---

## Capability index (D0–D2)

| Capability | Home |
|------------|------|
| Work board | D1.3 ✅ `work-jobs` |
| Calendar | D2.13 ✅ `calendar` |
| Human Browser + agent handoff / CAPTCHA | D2.3 ✅ `browser` |
| You / Profiles + Locus | D2.6 ✅ `profiles-locus` |
| Peers + shared mode / seats | D2.7 ✅ `sharing-phone` |
| Messaging channels | D2.8 ✅ `messaging-channels` |
| Runtime telemetry | D2.10 ✅ `runtime-telemetry` |
| Tool permissions + budgets | D1.2 ✅ `permissions-budgets` |
| Runtime Controls (day-one) | D1.2 ✅; full settings matrix → D3.2 |
| MCP + Packages | D2.11 ✅ `mcp-packages` |
| Specialist skills | D2.5 ✅ `specialist-agents` |
| Vault versioning + trash | D2.2 ✅ `vault-recovery` |
| Welcome wizard | D0.2 ✅; Garage onboarding depth still light |

---

## Thin chapters today (expand in place or split)

| Chapter id | Status |
|------------|--------|
| `navigation-surfaces` | D0.3 ✅ |
| `chat` | D1.1 ✅ |
| `vault-notes` / `vault-recovery` | D2.1–D2.2 ✅ |
| `views-environments` | D2.9 ✅ |
| `grapheme-automations` / `specialist-agents` | D2.4–D2.5 ✅ |
| `sharing-phone` / `messaging-channels` | D2.7–D2.8 ✅ |
| `workshops-connections` | Still thinner than D2 peers — deepen in D3 recipes or a follow-up |
| `keyboard-flow` | D3.1 (generate from catalog) |
| `themes-customization` | D3.5 |

---

## Implementation notes

| Concern | Rule |
|---------|------|
| Content files | `apps/medousa-home/src/lib/guide/pages/*.md` (Vite `?raw`) |
| TOC | Register every chapter in `catalog.ts` (`GUIDE_CHAPTERS` + `GUIDE_GROUPS`) |
| Entry points | Status bar help, Spotlight `open-operators-guide`, Settings Learn more — already wired |
| Guide window | Tauri `guide` / `/popout/guide` |
| Cross-links | `guide:chapter-id` (DOMPurify allowlist) |
| Callouts | Fenced ` ```callout ` — not ` ```liquid ` |
| Integrator docs | Stay in `docs/`; this epic is Home operator wiki only |
| Cursor agents | See `.cursor/rules/operators-guide.mdc` when editing `guide/**` |

---

## Suggested TOC evolution (target shape)

Not a commitment to ship all at once. Prefer phased delivery above.

**Start:** Welcome · Architecture & terminology · Install / first run · Getting started · Connect a workshop · First chat / note / Work card · Platform matrix  

**Workspace:** Navigation · Panes/tabs/desktops · Spotlight · Chat set · Work · Browser · Calendar · Profiles · Locus · Themes/a11y · Keyboard ref  

**Library:** Vault fundamentals · Organization · Live/Build/Preview · Formatting · Wikilinks · Boards/sheets · Charts/slides · Liquid ref · Export/recovery · Versions  

**Custom workspace:** Views · Create/edit · Widgets/tiling · Feeds · Backup/share  

**Automation:** Grapheme · Host modules · Recipes · Flows · Schedules · History/delivery · Specialists · MCP  

**Collaboration:** Phone · LAN pairing · Peers · Shared mode · Channels  

**System:** Workshops · Runtime · Runtime Controls · Packages · Updates · Data lifecycle · Troubleshooting · Limits · Glossary · What’s new · Recipes  

---

## Working log

- **2026-07-26** — Epic created from platform-vs-guide gap audit. Eleven orientation chapters shipped; D0–D3 backlog opened.
- **2026-07-26** — **D0 shipped:** new `architecture` chapter (mental model + glossary seed); rewritten getting-started (wizard/migration/rerun/phone/reconnect); rewritten navigation (surface matrix, Library/Automations modes, desktops, mobile More). Catalog renumbered `00`–`11`.
- **2026-07-26** — **D1 shipped:** expanded `chat`; new `permissions-budgets`, `work-jobs`, `troubleshooting` (`12`–`14`).
- **2026-07-26** — **D2 shipped:** expanded vault/views/automations/sharing; new `vault-recovery`, `browser`, `calendar`, `specialist-agents`, `profiles-locus`, `messaging-channels`, `runtime-telemetry`, `mcp-packages`, `liquid-reference` (`15`–`23`). Catalog at 24 chapters.
