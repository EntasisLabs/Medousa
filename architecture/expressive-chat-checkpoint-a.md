# Expressive Chat Checkpoint A — physical ink validation

> **Status:** Pending physical-device validation
> **Implementation:** Slices 1–2 complete in the commit containing this record
> **App version:** `medousa-home` 0.10.3
> **Prepared:** 2026-09-11

This record is the release gate between Slice 2 and Slice 3 of the
[Expressive Chat epic](expressive-chat-media-ink-liquid-epic.md). Do not start
Slice 3 until every required device class passes, or failures are explicitly
accepted below with named follow-ups.

## Automated evidence

| Gate | Result |
|------|--------|
| Drawing-focused Vitest suite | Pass — 5 files, 22 tests |
| Medousa Svelte/TypeScript check | Pass — 0 errors, 0 warnings |
| Full Medousa Home Vitest suite | Pass — 301 files, 1,565 tests |
| Strict documentation verification | Pass |

Covered automation includes version-1 migration, version-2 round trips,
malformed and oversized payload rejection, pressure/velocity width, retained
outline generation, sparse partial-erase splitting, crossing-lasso selection,
camera round trips, command undo/redo, and coalesced pointer sampling.

## Required devices

| Device class | Device / model | OS | Input | Build | Result |
|--------------|----------------|----|-------|-------|--------|
| iPad | _Record during test_ | _Record_ | Apple Pencil _model_ | _Record_ | ⬜ |
| Android | _Record during test_ | _Record_ | Active stylus _model_ | _Record_ | ⬜ |
| Desktop | _Record during test_ | _Record_ | Mouse / trackpad | _Record_ | ⬜ |
| Touch-only mobile | _Record during test_ | _Record_ | Touch | _Record_ | ⬜ |

Use a real Apple Pencil and a real Android active stylus. Simulator results are
useful diagnostics but do not close those rows.

## Test procedure

For each device, create a full drawing note, run the applicable scenarios, save,
leave the note, reopen it, then record pass/fail and feel notes.

| Scenario | iPad | Android stylus | Desktop | Touch-only | Notes |
|----------|------|----------------|---------|------------|-------|
| Slow/light to fast/heavy sweep visibly changes width | ⬜ | ⬜ | N/A | N/A | |
| Fast curves stay smooth and feel close to the tip | ⬜ | ⬜ | ⬜ | ⬜ | |
| Palm contact creates neither ink nor an unexpected pan | ⬜ | ⬜ | N/A | N/A | |
| Pinch zoom and two-finger pan never create ink | ⬜ | ⬜ | N/A | ⬜ | |
| One-finger touch pans; **Finger draws** enables touch ink | ⬜ | ⬜ | N/A | ⬜ | |
| Pen, eraser, select, and hand switching is predictable | ⬜ | ⬜ | ⬜ | ⬜ | |
| Partial erase preserves the unaffected stroke sections | ⬜ | ⬜ | ⬜ | ⬜ | |
| Lasso move, duplicate, delete, undo, and redo work | ⬜ | ⬜ | ⬜ | ⬜ | |
| Mutations survive save, leave, reopen, and app reload | ⬜ | ⬜ | ⬜ | ⬜ | |
| A version-1 fixture keeps the same visible ink | ⬜ | ⬜ | ⬜ | ⬜ | |
| Background/foreground does not leave a stuck gesture | ⬜ | ⬜ | N/A | ⬜ | |

For pen devices, note approximate perceived latency, pressure range, whether tilt
arrives, whether coalesced samples appear effective on fast curves, and any
WebView-specific limitation. A missing optional tilt signal does not fail the
gate; broken pressure, palm rejection, or core gesture arbitration does.

## Findings and accepted follow-ups

_Complete during physical testing. Every accepted failure needs an owner and a
tracked follow-up before this gate can close._

| Finding | Device | Severity | Owner / follow-up | Accepted? |
|---------|--------|----------|-------------------|-----------|
| _None recorded_ | | | | |

## Sign-off

- Overall result: **⬜ Pending**
- Tested by: _Pending_
- Date: _Pending_
- Notes: _Pending_
