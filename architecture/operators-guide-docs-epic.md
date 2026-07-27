# Operator’s Guide — docs epic

> **Status:** Active (2026-07) — **D0–D3 coverage shipped**; **P0 Product voice / mom path** in progress/complete this pass  
> **Audience:** End users of Medousa Home (product), not contributors  
> **Thesis:** The in-app guide is a **product manual**. Coverage is broad; voice and TOC must stay everyday-first. Project hygiene lives in the repo, not in the reader TOC.  
> **Product content:** [`apps/medousa-home/src/lib/guide/`](../apps/medousa-home/src/lib/guide/)  
> **Project maintenance:** [operators-guide-maintenance.md](operators-guide-maintenance.md) · `.cursor/rules/operators-guide.mdc`

**Related:** [polish-and-package-plan.md](polish-and-package-plan.md) · [ROADMAP.md](ROADMAP.md)

---

## Reality check

In-app guide: **Start / Everyday / Create / Connect / More** (~32 chapters including Find answers; governance removed from TOC).  
Mom lives in **Start + Everyday**. Power topics stay under **More** / advanced Create chapters.

Integrator / engine docs stay under [`docs/`](../docs/).

---

## North star

> Product docs a non-technical reader can use to answer real questions — chat, notes, phone, offline — without seeing repo, npm, epics, or contributor process.

---

## Principles (product)

1. **Everyday first** — Welcome → Find answers → Getting started → Everyday group.
2. **UI labels over internals** — workshop, Library, Allow; not daemon/Tauri/Stasis unless quoting UI.
3. **No project leakage** in chapter bodies — see ban list in maintenance doc / Cursor rule.
4. **Advanced framing** on power chapters.
5. **Generated appendix** — `npm run guide:generate` (project); emitted markdown must stay product-safe.
6. **Update this epic** when docs packages ship.

---

## Mom success test

Using only **Start + Everyday**, a non-technical reader can:

1. Send a chat and find Voice / Stance under the box.
2. Create or open a Library note.
3. Pair a phone (or know QR is on the desktop).
4. Recover from Offline via Workshop status.
5. Understand Allow vs Deny without Runtime Controls.

---

## Phase map

| Phase | Theme | Status |
|-------|--------|--------|
| D0–D3 | Coverage (concepts → reference) | ✅ |
| **P0** | Product voice / mom path / leak scrub | ✅ this pass |

### P0 packages

- [x] Remove in-app governance; move to [operators-guide-maintenance.md](operators-guide-maintenance.md)
- [x] Find answers + Welcome front door
- [x] TOC groups Start / Everyday / Create / Connect / More
- [x] Soften Getting started + Architecture glossary
- [x] Productize `guide:generate` output
- [x] Scrub project leaks; advanced framing on power chapters
- [x] Product What’s new / FAQ; Cursor rule + epic ban list

---

## D0–D3 (coverage — done)

See working log. Chapter homes remain; titles/groups may change for product clarity.

Maintenance checklist (feature → doc, generate, tests): [operators-guide-maintenance.md](operators-guide-maintenance.md).

---

## Implementation notes

| Concern | Rule |
|---------|------|
| Product pages | `apps/medousa-home/src/lib/guide/pages/*.md` |
| TOC | `catalog.ts` — groups start/everyday/create/connect/more |
| Generate | `npm run guide:generate` — project only; product-safe emit |
| Callouts | ` ```callout ` |
| Never in-app | Epic status, npm, PR checklists, source paths, D0–D3 |

---

## Working log

- **2026-07-26** — Epic created; D0–D3 coverage shipped (orientation → reference).
- **2026-07-26** — **P0 Product voice:** Find answers; TOC regroup; governance → architecture only; generator product copy; leak scrub; mom success test; Cursor rule ban list.
