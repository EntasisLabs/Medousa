# Medousa Home — M11 Plan (Settings Charter)

> **Status:** **M11a–M11d shipped** in repo (2026-06-07)  
> **Date:** 2026-06-07  
> **Epic:** **M11 — Settings as workshop charter**  
> **Supersedes:** M10 boundary rules (Settings ≠ integrations hub, Settings ≠ connection hero)  
> **Builds on:** [M10 plan](medousa-home-m10-settings-runtime-plan.md), shipped M7 surfaces  
> **Related:** [medousa-home-product-ux-plan.md](medousa-home-product-ux-plan.md)

## North star

Settings is where you **author the relationship** with Medousa — not where you navigate the app.

**Mantra:** Charter here. Nouns elsewhere. Pulse in Runtime. Ambient status in the bar.

**Steve test (epic exit):** Operator opens Settings and immediately sees *how she remembers*, *how she speaks*, and *how she interrupts their life* — without seeing cron counts, connection pills, or chevrons to Messaging.

---

## The critique M11 answers

| Steve says | M10 (wrong jurisdiction) | M11 response |
|------------|----------------------------|--------------|
| Am I in control? | Summary strip: Connected · theme · cron | Charter line: *Remembers 8 turns hot · answers deep · qwen2.5* |
| Where’s memory? | Buried in Runtime → Workshop → Memory tab | **Settings → Memory** with human sliders |
| Why Connection in Settings? | Primary nav section | **Basement only** — status bar owns ambient link |
| Why Integrations hub? | Pointers to Messaging/Cron/Runtime | **Deleted** — Messaging/Cron are own nav identities |
| Messaging in Settings? | Hub row | **No** — comms surface stays nav; expands to email/phone later |
| Runtime Workshop tab? | Primary edit surface for defaults | **Terminal mirror** — day-to-day charter in Settings |

---

## Three territories (jurisdiction)

| Territory | Question | Owns |
|-----------|----------|------|
| **Settings** | *How should my workshop feel and behave toward me?* | Room, Rhythm, Memory, Voice, Reach, Basement |
| **Surfaces** | *What nouns exist in my life?* | Chat, Work, Library, Skills, **Messaging**, **Cron** |
| **Runtime** | *What's the engine doing right now?* | Now, Jobs, Delivery, Routing telemetry; Workshop = full `tui_defaults` mirror |

**Status bar** = ambient pulse (connected, cron whisper, in motion). Not Settings.

**Messaging** = own entity forever — quick reach changes without opening Settings; future comms hub (email, phone).

---

## Settings nav (M11)

| Section | ID | Human job | Backing |
|---------|-----|-----------|---------|
| **Room** | `room` | Theme, dark mode | `settings` store |
| **Rhythm** | `rhythm` | Notifications, activity feed noise | `settings` store |
| **Memory** | `memory` | Hot/cold windows, long-session threshold | `tui_defaults` via `workshopDefaults` |
| **Voice** | `voice` | Provider, model, response depth | `tui_defaults` via `workshopDefaults` |
| **Reach** | `reach` | Allowed tools, web search, tool posture | `tui_defaults` (M11c) |
| **Basement** | `basement` | Connection URL, files, diagnostics, verifier hell | connection + advanced |

Header copy: **Settings** — *Your charter with the workshop.*

Charter one-liner (derived): e.g. *Remembers 8 turns hot · answers deep · qwen2.5:7b*

---

## Epic overview

| Phase | Name | Ship |
|-------|------|------|
| **M11a** | Strip facade | Kill Connection, Integrations, telemetry strip; new nav; Basement absorbs connection |
| **M11b** | Memory + Voice | Human controls wired to `workshopDefaults.save()` |
| **M11c** | Reach + Routes | Allowed tools, web search, simplified routing posture |
| **M11d** | Runtime diet | Controls tab → “quick override”; Workshop tab = terminal mirror only |
| **M11e** | Context view | Separate nav — read-only STTP + identity (not Settings) |

**Next epic after M11:** Context view + Messaging polish pass.

---

## M11a — Strip facade

### Remove from Settings

- `SettingsConnectionSection` as primary nav → move into Basement
- `SettingsIntegrationsSection` → delete
- Summary strip with connected/cron/theme → charter one-liner
- Props: `onOpenMessaging`, `onOpenCron`, `onOpenRuntime`, cron counts

### Touch

| File | Change |
|------|--------|
| `types/settings.ts` | New section IDs + labels |
| `SettingsPanel.svelte` | Charter header, section router, load defaults on open |
| `SettingsBasementSection.svelte` | Advanced + connection (from M10 advanced + connection) |
| `SettingsRoomSection.svelte` | Renamed appearance |
| `SettingsRhythmSection.svelte` | Renamed general |

---

## M11b — Memory + Voice

### Memory (human copy)

| Field | Label | Meaning |
|-------|-------|---------|
| `sliceHotWindowTurns` | Recent turns kept vivid | How much of the chat stays “hot” |
| `sliceColdWindowTurns` | How far back she can recall | Cold window in a long thread |
| `activationLongSessionTurnThreshold` | When a chat becomes long | Turn count before long-session rules |
| `activationLongSessionMaxPromptChars` | Extra context for long chats | Advanced but visible here |

### Voice

| Field | Label |
|-------|-------|
| `responseDepthMode` | Concise / Standard / Deep (with hints) |
| `provider` | Who answers |
| `model` | Which model |

Save → `workshopDefaults.save()` → `tui_defaults.json` (Mac desktop). Mobile: read-only snapshot + copy pointing to Mac Settings.

### Touch

| File | Role |
|------|------|
| `SettingsMemorySection.svelte` | Memory charter |
| `SettingsVoiceSection.svelte` | Voice charter |
| `SettingsCharterSaveBar.svelte` | Shared save + status |

---

## M11c — Reach (shipped)

- Allowed tools (textarea — chips later)
- Web search provider + fallbacks
- Tool call mode, host turn bus — human labels
- Tool rounds per turn (single limit)
- Routing posture table (read-only summary; specialists in Runtime → Workshop)

### Touch

| File | Role |
|------|------|
| `SettingsReachSection.svelte` | Reach charter |
| `types/settings.ts` | Reach nav + human option labels |

---

## M11d — Runtime diet (shipped)

| Tab | After M11d |
|-----|------------|
| Controls | Quick override for *this session* — copy points to Settings → Voice |
| Workshop | Full `WorkshopDefaultsPanel` — terminal mirror, not primary |
| Routing | Read-only telemetry of configured routes |

---

## Messaging boundary (explicit)

**Messaging stays its own nav surface.** Not in Settings. Not linked from Settings.

Rationale: operators change *where Medousa can reach them* frequently; comms expands (Telegram today, email/phone tomorrow). Settings owns *how she behaves*; Messaging owns *who can talk to her and where she replies*.

---

## Verification

```bash
cd Medousa/apps/medousa-home && npm run check && npm run build
```

Manual:

1. Settings → no Connection/Integrations in left nav
2. Memory + Voice edit and save on Mac → `tui_defaults.json` updates
3. Header shows charter line, not cron/connected
4. Messaging/Cron only via nav (or You hub on mobile)
5. Basement has connection + diagnostics
6. Runtime → Controls copy points to Settings
