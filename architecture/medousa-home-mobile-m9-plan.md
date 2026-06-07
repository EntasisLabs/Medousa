# Medousa Home — Mobile M9 (Product Skin)

> **Status:** Shipped in repo (M9a–M9c)  
> **Date:** 2026-06-07  
> **Builds on:** [medousa-home-mobile-plan.md](medousa-home-mobile-plan.md) (M8a–M8e complete)

## Thesis

M8 built the **choreography** — tabs, stacks, sheets, gestures. M9 builds the **soul** — what each screen is *allowed* to say, show, and feel.

**Jobs test:** Open the app. In two seconds, know whether Medousa is waiting on you, working for you, or quiet — without reading a dashboard.

---

## Principles (subtraction first)

| Rule | M8 (before) | M9 (after) |
|------|-------------|------------|
| **One hero** | Label + title + button + schedule + 3 tiles | One status + one headline + one action |
| **Human voice** | Column names, session IDs, depth mode | Waiting / Running / Quiet / Offline |
| **Chrome budget** | Global top bar + per-tab header | Pulse owns connection; other tabs slim or none |
| **Inventory hidden** | Three zero-tiles always visible | Motion summary only when non-zero |
| **You ≠ Settings.app** | Seven equal chevron rows | Two sections: Stay in touch · Workshop |
| **Alive** | Static green dot | Breathing indicator when workshop has motion |

---

## Screen contracts

### Pulse (home)

Answers: *Is Medousa waiting on me, working for me, or quiet?*

- Top row: workshop status line + activity (no global shell bar)
- Hero priority: **blocked → primary in-motion card → quiet**
- Quiet state: never promote last-note filename as hero
- Optional single motion summary line (not three boxes)
- Next schedule: one whisper line, not a widget

### Work

- No duplicate title bar; one status strip (pull-to-refresh replaces Refresh button)
- Human section labels: Needs you · Running now · Done today
- Empty: “All clear” + human CTA (not architecture copy)

### Chat

- Title: **Medousa** (or phase line while streaming)
- Subtitle: last conversational line — not model/depth/session id
- Empty: “Say one thing” — no daemon config footer

### You

- Hub: two sections max
  - **Stay in touch** — Notes, Skills, Channels
  - **Workshop** — Schedule, Settings, Advanced, Health
- Drill-downs unchanged (M8d stacks)

---

## Milestones

| Phase | Deliverable | Feel test |
|-------|-------------|-----------|
| **M9a** | Pulse rewrite, chrome removal, alive indicator | “I feel the heartbeat” |
| **M9b** | Work + Chat human copy, slim chrome | “Not a settings app” |
| **M9c** | You hub sections, motion polish | “I know where to go” |

M9a–M9c ship together in this pass.

---

## Not M9

- New tabs, new backend APIs, custom fonts, illustration system
- Desktop reskin (mobile-only fork via `layout.isMobile`)

---

## Document history

| Date | Change |
|------|--------|
| 2026-06-07 | M9 plan — product skin from Jobs critique |
| 2026-06-07 | M9 shipped — Pulse rewrite, chrome cuts, You sections, human copy |
| 2026-06-07 | Activity feed polish — timeline cards, tone icons, human summaries |
| 2026-06-07 | Activity enrichment slice 1 — client join via card refs + detail cache |
