# Expressive Chat Checkpoint A — physical ink validation

> **Status:** Round 1 failed; remediation implemented, physical retest pending
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
| Drawing-focused Vitest suite | Pass — 5 files, 25 tests |
| Medousa Svelte/TypeScript check | Pass — 0 errors, 0 warnings |
| Full Medousa Home Vitest suite | Pass — 301 files, 1,568 tests |
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

## Round 1 — 2026-09-10 TestFlight findings

User-supplied iPad and iPhone captures confirmed that pressure input reached the
app, but Checkpoint A did not pass. Device and OS build numbers were not recorded
for this round and must be captured during the retest.

- The wide control ribbon wrapped or required horizontal scrolling, separating
  the active tool from its options and forcing users to hunt for controls.
- The fixed landscape SVG was letterboxed inside a portrait interaction surface.
  At low zoom, old ink and the grid collapsed into a small nested canvas while
  new pointer input mapped across the much larger visible area.
- A pinch could end after one finger lifted while the remaining touch still
  belonged to the old gesture, making drawing appear stuck.
- The visible grid and paper boundary competed with the ink and made the surface
  feel like an embedded object instead of a drawing canvas.
- Native WebView long-press selection/callout behavior interrupted pressure
  holds and slow strokes.

The remediation replaces the ribbon with four persistent tool buttons and a
grouped options sheet, uses the real rendered viewport, fits existing ink,
preserves the viewed center on resize, makes brush width stable at the current
zoom, keeps pinch ownership until every participating touch ends, removes the
grid/paper boundary, and suppresses native selection, drag, and context-menu
gestures on the canvas.

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
| Scrolling/wrapped controls obscure drawing options | iPad + iPhone | High | Remediated with compact toolbar and options sheet; retest | No |
| Fixed landscape viewport diverges from portrait pointer space | iPhone | Critical | Remediated with measured viewport and intentional fit; retest | No |
| Pinch can strand the remaining touch and appear stuck | iPad + iPhone | Critical | Remediated by retaining pinch ownership until all touches end; retest | No |
| Grid and paper boundary make low zoom visually nested | iPad + iPhone | High | Removed; retest blank canvas | No |
| Long press invokes native WebView selection/callout behavior | iPad + iPhone | Critical | Canvas is no longer a button; native selection/callout/drag/context menu suppressed; retest | No |

## Sign-off

- Overall result: **🔄 Retest required**
- Tested by: _Pending_
- Date: _Pending_
- Notes: _Pending_
