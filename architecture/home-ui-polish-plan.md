# Home UI polish — streaming, thinking, work hygiene

> **Status:** Shipped (2026-06-13) — A1–D3 + C1/C2 complete; A3 and D4 deferred  
> **Product decisions (locked):**

| Topic | Decision |
|-------|----------|
| Failed / terminal cards | Auto-hide from board after **24h**; auto-wipe archived data after **7 days** — **user-configurable** in Settings |
| Verification badges | **Hidden completely** in Home (not wiped from engine/history) |
| Mobile LLM routing | **Always Mac daemon defaults** — phone is a portal until mobile runs its own daemon |
| Engine telemetry (`◈`, orchestrator=, fallback=) | **One toggle** — hidden by default, **not deleted** from stream/history; power users can show |

---

## Phase A — Reconnect & stream alignment (mobile-first)

**Goal:** Background/kill/resume → chat matches daemon within ~1s.

| ID | Deliverable | Status |
|----|-------------|--------|
| A1 | Tauri/mobile **foreground hook** → refresh health, workspace stream, `tryReattachActiveTurn` | ✅ |
| A2 | **Smart reconcile** on resume: fetch session turns + active tickets; merge with local transcript by `turnId`; de-dupe reattach bubbles | ✅ |
| A3 | Stream ownership map: reattach only non-terminal turns for current session (+ linked worker sessions) | ⬜ |

**Acceptance:** Mid-turn background → return → same bubble continues or shows final; no phantom thread; composer works.

---

## Phase B — Operator thinking UX

**Goal:** Capsules + tool chips only; no TUI log tail in chat.

| ID | Deliverable | Status |
|----|-------------|--------|
| B1 | Filter `status` events (`orchestration`, raw `tool=`) from chat **statusLine** unless “Show engine details” | ✅ |
| B2 | **Hide verification badges** entirely in `ChatMessageList` | ✅ |
| B3 | Friendly `turn_progress` copy only; never mirror telemetry into `content` | ✅ |
| B4 | Settings: **Show engine details in chat** (reuse/extend technical activity toggle pattern) | ✅ |

**Acceptance:** Live turn shows capsule + chips + one human status line — no `orchestrator=` / `fallback=`.

---

## Phase C — Model honesty (mobile portal)

**Goal:** UI never implies GPT when daemon defaults differ.

| ID | Deliverable | Status |
|----|-------------|--------|
| C1 | Mobile **never sends** provider/model/stageRouting on turns (portal to Mac) — document in UI | ✅ |
| C2 | Optional subtle chip: “Using Mac daemon defaults” when on mobile (read from runtime summary) | ✅ |
| C3 | Engine (later): `operator_message` vs `debug_message` on stream events | ⬜ deferred |

---

## Phase D — Work card hygiene & retention

**Goal:** Counts reflect “needs attention now”; datastore bounded.

| ID | Deliverable | Status |
|----|-------------|--------|
| D1 | **User settings:** hide-after (default 24h), wipe-after (default 7d) for terminal work cards | ✅ (`tui_defaults.json` + `/v1/runtime/defaults`) |
| D2 | Engine: apply TTL to **failed/cancelled** turn workers (mirror ask-job stale); archive API for turn_worker | ✅ |
| D3 | UI: **Clear failed** / **Clear stopped** tray actions; badge = actionable blocked only | ✅ |
| D4 | Activity feed: respect technical toggle; optional “Clear viewed” (hide only) | ⬜ deferred |

**Acceptance:** User clears Failed tray; Pulse badge drops; old failures auto-hide/wipe per settings.

---

## Implementation order (PR-sized)

1. **B1 + B2 + B4** — thinking UX + toggle (fast polish)
2. **A1 + A2** — mobile resume reconcile
3. **D1 + D2 + D3** — retention settings + engine TTL + tray clear
4. **C2** — mobile “daemon defaults” affordance

See also: [NEXT.md](NEXT.md) (normie operator surface).
