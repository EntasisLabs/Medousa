# Composer voice presets

> **Status:** Phase 1 shipped (2026-06-07)  
> **Related:** [runtime-collaborator-voice.md](runtime-collaborator-voice.md), [medousa-home-m11-settings-charter-plan.md](medousa-home-m11-settings-charter-plan.md)

## North star

One memory, one identity graph — switch **stance** and **tool strap**, not profile worlds.

Hermes-style profiles (separate homes, SOUL files, isolated memory) do **not** fit Medousa. Voice presets are a lightweight composer control: how she answers **this turn**, layered on shared Locus + Stasis continuity.

**Mantra:** Voice = stance. Depth = reasoning budget. Tools = strap (Phase 2).

---

## What a voice preset is

| Field | Purpose |
|-------|---------|
| `id` | Stable key (`default`, `direct`, or user slug) |
| `name` | Composer + Settings label |
| `description` | Optional one-liner in Settings |
| `voiceAppendix` | Short stance block injected into turn prompt prep |
| `builtin` | Built-ins are code-defined; custom presets live in `tui_defaults.json` |

Voice presets are **not** manuscripts. They do not change tool allowlists, cron lanes, or identity isolation.

---

## Built-in defaults

| ID | Name | Appendix |
|----|------|----------|
| `default` | Default | *(empty — runtime collaborator voice only)* |
| `direct` | Direct | Action-first, lead with answer/next move, skip preamble |

Users may create up to **8 custom** presets (same cap pattern as `favoriteModels`).

---

## Axes (orthogonal)

```
Composer toolbar:  [Model ▾]  [Voice ▾]  [Depth ▾]  …  [Tools ▾ — Phase 2]
                         │           │            │
                         │           │            └── response_depth_mode (existing)
                         │           └── voice_appendix (this plan)
                         └── provider:model (existing)
```

**Depth** (Concise / Standard / Deep) stays in Settings → Voice and composer — controls how much reasoning lands on the page.

**Voice** (Default / Direct / custom) controls tone and stance — expands the existing Settings “Voice” section beyond depth-only.

**Tools strap** (Phase 2): thin manuscript specialty or `tools.allow` kit — separate dropdown, optional `manuscript_id` on turn ticket. Not bundled into voice.

---

## Storage

```json
// tui_defaults.json (camelCase in Home DTO)
{
  "activeVoiceId": "direct",
  "customVoicePresets": [
    { "id": "briefings", "name": "Briefings", "description": "…", "voiceAppendix": "…" }
  ]
}
```

- Built-ins are never persisted; only `activeVoiceId` + `customVoicePresets`.
- Mobile reads active voice via paired Mac `tui_defaults` (same as depth/model defaults).

---

## Turn wiring (Phase 1)

1. Home resolves active preset → `voicePresetId` + `voiceAppendix` (empty for Default).
2. `POST /v1/turns` (`CreateTurnTicketRequest`) carries optional `voice_preset_id`, `voice_appendix`.
3. `prepare_turn_prompt` appends `[MEDOUSA_VOICE]` block after manuscript hints, before identity context.
4. No change to system STTP — voice is per-turn appendix, like manuscript `voice_appendix`.

Prompt block shape:

```
[MEDOUSA_VOICE]
preset=direct
<stance text, budget-truncated>
```

---

## Surfacing

| Surface | Phase 1 |
|---------|---------|
| Mobile composer | `[Voice ▾]` select beside Model + Depth |
| Desktop composer | Voice segment in model picker panel |
| Settings → Voice | Built-ins list, custom CRUD, depth charter (existing) |
| TUI / CLI | Future — read `activeVoiceId` from `tui_defaults` |

Settings Voice hint copy: *How she speaks and how much reasoning lands — not who powers chat.*

---

## Phase 2 — Tool strap kits

- Composer `[Tools ▾]`: None + thin manuscripts (skills-only catalog subset).
- Pass `manuscript_id` on `createTurnTicket` (field already exists on daemon API).
- Voice and tools compose; neither replaces the other.

---

## Out of scope

- Hermes-style profile homes or per-voice memory partitions
- Full manuscript YAML for everyday chat stance
- Replacing Stasis persona / identity graph with voice presets
- STT / dictation settings (stay under Models)

---

## Exit criteria (Phase 1)

- [x] Default + Direct built-ins selectable in composer
- [x] Custom voices CRUD in Settings (cap 8)
- [x] Active voice persists in `tui_defaults.json`
- [x] Turn tickets carry appendix; daemon injects `[MEDOUSA_VOICE]` in prompt prep
- [x] Depth charter unchanged and separate from voice stance
