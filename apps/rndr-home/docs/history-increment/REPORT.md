# Independent desktop history increment — implementation handback

## Exact binding and selected scope

Base: `15eedfbc66cf4f2713f1ac148a27b7f80f1080e7` (clean at entry).
Implementation: `995fcd5b85ab67708634623f83251fd1ce363d0d`.
This directory's subsequent evidence-only commit is not another runtime revision.
`implementation.patch` is the complete base-to-implementation diff (11 files,
492 additions / 5 deletions, including registered tests and gate runner).

Selected the existing desktop transcript/scrolling/reading priority from the
preparation and UX direction: **Jump to latest while reading earlier history**.
This is useful with current startup/reattached/synthetic history and streaming;
it needs only app-owned transcript geometry, not a native input signal or a
freeze of the disputed session/draft contract. It is not a new session browser
or a partial implementation of that contract. No contract document was edited.

The control uses the existing canonical pointer, keyboard and semantic action
queue. It appears only away from an overflowing transcript's tail, disappears
at the tail/empty state, keeps the draft unchanged, does not copy message text,
and has no SDK/runtime command authority. It remains available during busy
execution. A small reserved lane prevents visibility changes resizing the
scrollport. If its focused trigger disappears, focus goes to the existing
history scrollbar, never to or through a programmatic composer reset.

The increment also repairs an app-local virtual-layout follow edge exposed by
its regression: intermediate Markdown measurement can clamp an End operation
short of the final bottom. Conversation now remembers the completed layout's
actual offset while following, with two bounded ViewportRows samples per app
sync; it must not overwrite that checkpoint during the next Input-time scroll
refresh. Deliberate pointer drag or keyboard Home cancels follow. No recursive
layout, framework edits, unbounded retries or native API assumptions were added.

## Dependency isolation / safety boundary

Unproven native composition/emptiness is **not a blanket implementation hold**.
It still gates the operations that depend on it. In this increment production
Send, Restore, New, Recent and Reload are disabled in UI and independently
rejected in the controller before mutation/dispatch, including queued/forged
activation and submitted events. Footer explains the restriction. This is an
intentional temporary restriction, not a claim that the old batch guard is a
persistent native detector. Existing user text and retained recovery survive.
Model/appearance/reading surfaces, transcript selection/scrolling and existing
Cancel/authorization behavior are not replaced. Runtime authorization and SDK
contracts are unchanged. No automatic ownership-transfer interface was frozen.

The sole `SyntheticEditor` opt-in exists under `cfg(test)` only, in explicit
mock-editor harnesses. There is no CLI, environment, observe, runtime, or release
build bypass. Legacy transition tests continue to test synthetic state-machine
behavior; they are **not native safety evidence**. The new default-closed tests
do not install the marker. Observer output also remains Deny for unknown native
state, even an empty, opted-in offline fixture. Existing independent privacy
conjunct cases remain covered with explicitly synthetic native state.

The separate rndr investigation remains independent. Its result is neither
assumed nor consumed here. No framework write, dependency/pin change, shared
writer delegation or framework lifecycle claim was made. Focused read-only API
inspection of the current pinned generic interaction/layout/scrollbar code was
used to diagnose this increment; no external source download was performed.

## Exact-implementation verification

All ten final gate receipts in `runs.jsonl` bind to `995fcd5b...`, with captured
commands, exit codes, binary digests, environment, logs, sampled disk minima:

- `cargo fmt --check`: PASS.
- Default `cargo test`: **6 library + 51 app tests**, no failures.
- Observe-feature `cargo test`: **6 library + 58 app tests**, no failures.
- `cargo clippy --all-targets --all-features -- -D warnings`: PASS.
- Default and observe-feature offline/locked builds: PASS.
- Dark, light and high-contrast offscreen export: PASS (12 fixture PNGs each).
- `git diff --check`: PASS.

New registered cases in `history_tests.rs` cover pointer activation at 480, 520
and 1120 points, 1,000-message virtualization, reading position during tail
changes, explicit follow resumption, draft/session/generation preservation,
zero runtime requests, empty-history no-op, keyboard Enter while busy, stable
scrollbar focus, deliberate Home cancellation, repeat semantic activation, and
production-closed visual/controller/submission/Restore admission. The new
observe case independently denies unknown native state across quiet frames.
Existing scrollbar-drag, wrapping/selection/copy, synthetic submission/recovery,
mode/authorization and bounded mock-worker tests also pass.

Tests with HTTP use existing loopback fixtures only, not a daemon/provider.
No paid inference, credentials or private-data capture occurred. Normal
nonfatal pre-existing dependency manifest warnings remain in raw logs. Earlier
failed runs are retained unchanged here: an unsupported test-harness resource
API was corrected; keyboard follow/refinement and Input-time snapshot bugs were
found and fixed; strict lint was corrected. Final successes do not erase them.

## Visual evidence and inspection

All PNGs here are **synthetic, offscreen**, exported from the real app with a
preserved synthetic draft and no command sender. They are not native window
captures or physical interaction evidence. `history-away.png` is 1120×820;
`history-away-narrow.png` and `history-latest.png` are 480×760. New exports assert
that the draft is still present and runtime command authority is absent.

Actual image-view inspection covered dark wide/away, dark narrow/away and
latest, plus light and high-contrast narrow/away. The visible control stays out
of the editor, the safety footer wraps inside the narrow viewport, the draft
remains, and the latest control hides at the tail. Final exports rebind to the
exact source commit; final dark narrow/away and latest were reopened, with
identical normalized image-view artifacts to the first inspection. Original
file SHA-256 values are in SHA256SUMS (image-view may normalize PNG encoding).

- `dark/history-away-narrow.png`
- `dark/history-latest.png`
- `dark/history-away.png`
- `light/history-away-narrow.png`
- `high-contrast/history-away-narrow.png`

## Reproduction and preservation

From the repo root, reuse the admitted target (do not create another target):

```sh
export CARGO_TARGET_DIR="$(cd ../.cache/cargo-target && pwd)"
python3 apps/rndr-home/docs/history-increment/run_checks.py \
  fmt default observe clippy build build-observe \
  export-dark export-light export-contrast diff
python3 apps/rndr-home/docs/history-increment/audit.py
```

The runner forces max 2 Cargo jobs, debug=0, incremental=0, offline/locked;
samples disk every 250ms, terminating its process group at/below 8 GiB. Existing
app dev/test profiles already disable debug/incremental. No concurrent build
was launched. Sampled minimum across all attempts: **14.79 GiB**; exact-source
final sequence minimum **14.89 GiB**. All work used the existing reusable target.
Re-running appends new receipts and may overwrite only this new directory's
synthetic exports; it never runs or rewrites prior evidence scripts.

`preservation.json` / `audit.py` verify the exact 11-path implementation allowlist,
8,200 other baseline tree entries unchanged, all **55 closure-sealed files** and
**52 qualification-sealed entries** matching hashes, and source-file digests.
All prior documentation, evidence, production Home, lockfiles and dependency
pins remain unchanged. No broad cleanup, push, publish, deployment, merge,
framework write, new writer, live private capture, or native integration.

`SHA256SUMS` covers this new evidence directory except itself. Historical seals
are not rewritten to pretend this new implementation belongs to old evidence.

## Residual risk and handback

Physical keyboard/IME/preedit, clipboard, accessibility/AT, compositor/window
behavior, live provider, huge-message performance and full desktop parity are
unverified. Synthetic geometry/interaction is not native input safety proof.
The follow-layout checkpoint is tied to the current pinned framework's bounded
ViewportRows schedule; changes to that schedule need these regressions and
independent review. Input actions stay closed until separately qualified reliable
signals/reset semantics exist. Startup/runtime routing is not rewritten by this
increment. The prior historical docs-verifier discrepancy remains open; no
preserved docs verifier or preserved evidence script was executed.

This is a completed, locally committed bounded implementation increment, **not
project approval, contract freeze, native qualification or integration**. Host
retains independent review, acceptance and native integration. No extra writers
were delegated.
