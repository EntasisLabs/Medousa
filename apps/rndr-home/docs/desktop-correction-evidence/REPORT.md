# Bounded desktop preservation correction

## Authority and exact revision boundaries

- Correction baseline/report-only review head:
  `b5b2fa9cb116cd8cd25275d61a28a15b06abeb80`.
- Independently reviewed source/evidence parent:
  `97e0c223bf1d1016b2e8b440e7e924fd9716ce7c`.
- Exact correction source commit/tree and per-file SHA-256 are in `seal.json`.
  The final additive evidence commit is returned in the handback; it necessarily
  cannot contain its own commit hash. Git binds this report/seal to that head.
- Independent review was accepted as a **deliverable**, not implementation
  acceptance. This pass is for independent correction verification, not approval,
  native integration, unrestricted UX signoff or project completion.

## Corrections and preservation policy

### Preferences focus (review P2)

`desktop.rs` captures focus for every dynamic navigation/body descendant before
its deletion, keyed by region plus Button action. It first migrates through the
normal validated focus path to a stable visible control, cancelling old armed
activation; after rebuilding it requests the same semantic control, not the
same deleted entity. This also covers appearance buttons, dataset links,
refresh/back/record controls, and settings sidebar/pager controls. When a sidebar
control has no narrow equivalent it uses the visible Settings rail button, never
the hidden Preferences filter. Data routes use their visible filter fallback.
The normal framework focus-reveal/accessibility path is reused, not replaced.
No editor/IME implementation or framework source is changed.

The three appearance controls retain their action, accessible name, focusability,
activation permission and focused AccessTree node across repeated round-trip
resize to 480x650/1120x650. App synthetic Tab/Shift-Tab/Enter events continue through the
migrated control. A changed sidebar/pager label is allowed where it denotes the
same destination action; an absent equivalent has an explicit stable fallback.
This is not a physical keyboard or assistive-technology qualification claim.

### Dataset scroll (review P2)

`Desktop.begin/settle` invalidate only a route consuming that resource. An
accepted off-route reply still updates its own cache and generation state but
cannot reset visible selection, entities, focus or scroll. Stale fencing remains
unchanged. Visible-resource replies still render fresh content; they are not
ignored to make a scroll assertion pass.

Same-view rebuilds retain their numeric offset. A short Loading placeholder
would clamp away the old offset, so the intended offset is retained separately
until settlement. Layout clamps it against measured new content, including
short/empty/error results. A refreshed larger list remains scrollable and new
results replace old results. This is point-offset preservation, not a stable
record-key anchor across reordered data; no new auto-follow or pagination is
implied. New navigation, changed filter, list/detail transitions and refresh
from a selected detail intentionally reset to the top. The last clears the old
positional selection so it cannot hide new data behind a reused row index.

### Bounded presentation work

Added wide/narrow 650px-height geometry and synthetic export variants, in all
three supported themes. Narrow rail width/font change from 58px/8px to 76px/10px
improves the starting readability budget. The exact 520px chat-center geometry
assertion changes from 414px to 396px to account for that 18px rail increase;
its equality assertion and draft-preservation assertion are retained. This is
not a weakened bound or a claim that all glyphs/clipping have been verified.
No footer truth label is shortened or removed: full INPUT PROOF remains in the
synthetic label. See the honest visual-verification limitation below.

## Regression provenance and checks

`src/desktop_preservation_tests.rs` promotes both reviewer reproductions to
permanent tests. `fail-before.log` captures both failures (exit101) with original
source at the correction baseline and only the test module/include added:
Preferences Focus=None and Runtimes offset -200 -> 0 after Health completion.
`fail-before.exit` records 101; original reproduction bytes remain in the
immutable independent-review directory. Rustfmt changed layout and an equivalent closure block, not the
reproduction assertions. `pass-after.log` records the first two-test pass after
the fix. Final focused/default/observe logs verify the sealed source.

Additional permanent tests cover semantic/accessible focused control identity,
Tab/Shift-Tab/Enter activation, settings/sidebar/pager/link fallback, unrelated begin and
accepted completion during a selected detail, visible list refresh with longer,
shorter and empty results, selected-detail refresh showing new data, navigation
reset, and retained chat draft/session/away-scroll across resized admin activity.
Existing input-gate, forged-intent, fixture contract, privacy/redaction, stale
read and observe tests remain in the full suites. All five chat input-sensitive
actions (Send/Restore/New/Recent/Reload) remain denied in production.

Final totals: focused **9/9**; default **6 library + 71 app**; observe
**6 library + 79 app**; doc tests 0. `results.tsv` records the complete final scoped checks: app fmt; default and
observe tests; strict app-scoped all-target clippy for both feature sets; default
and observe builds; offline GPU export; strict repository docs. No workspace-wide
cleanup or manifest/pin changes. Dependency/root manifest warnings are retained,
not suppressed by app lint allowances. `reproduce.sh` selects two Cargo jobs,
two test threads, offline mode, no debug info/incremental and fresh scratch paths.

Intermediate failures are retained, not omitted:
- `first-default-rail-width.log`: the former exact narrow rail geometry expected
  414px; updated to the correct 396px after the deliberate 18px wider rail.
- `parallel-fixture-timeout.log`: two unchanged loopback contract fixtures timed
  out under full default test concurrency. Bounded test concurrency avoids CPU
  contention without extending fixture deadlines, skipping tests, changing
  service contracts or weakening any assertion. This residual test scheduling
  sensitivity is not misreported as an app endpoint defect or unconditional
  high-parallelism pass.
- `first-clippy-formatting.log`: compact copied reproduction formatting triggered
  suspicious-assignment-formatting; conventional test-module registration plus
  rustfmt fixed whitespace without lint allowances or changed assertions.

## Actual synthetic pixel inspection, not native signoff

Fresh export contains **216 PNGs**: dark/light/high-contrast, wide/narrow at
ordinary and 650px heights, with 18 route/state cases per size/theme. They are
offline synthetic GPU renders, not native-window/input proof. The first direct
image-view attempt returned metadata only. **Actual runtime-attached pixels
subsequently became available**, including the directly viewed six-panel
`minimum-height-contact-sheet.png` (SHA-256
`b7a8beffd6a40839b5344b50667f8f7f85952b11728d43b9030a8c7dabb11ae2`).
This is a 1:1 tile of six unchanged 650px PNGs, composed by the adjacent offline
Swift script without OCR, text overlays or edits to the source renders. Direct
`cognition_image_view` was used, not a parallel wrapper. Metadata alone was never
used as the visual signoff. `visual-inspection.md` identifies samples/findings.

Actual inspection covers wide/narrow dark Preferences, light Accounts/Connection,
and high-contrast MCP/Preferences. Full footer INPUT PROOF is visible in all six
samples, on two lines in narrow views. The wider 10px narrow rail has readable
complete labels including Schedules; sampled titles, controls and pager text do
not overlap. The 650px rail leaves You below the initial fold; the wide settings
sidebar likewise requires scrolling to Packages/MCP/Connection. The light narrow
Connection body extends below its scroll viewport rather than disappearing; its
scroll indicator is visible. Existing synthetic sidebar/pager tests verify
last-section reachability. No new concrete app-level glyph clipping was
established in these inspected samples; this is not an exhaustive 216-image or
physical-device signoff. No OCR is represented as visual approval.

Remaining manual checks: physical wide/narrow windows and scaling; rail/section
scroll and below-fold controls; every long list/detail state; visible focus rings,
Space activation, assistive technology and physical keyboard/IME. Geometry and
pixel evidence do not establish those native behaviors. Dependent input actions
stay denied; these qualifications do not halt unrelated authorized non-chat work.

## Scope, parity and next safe slices

All prior evidence remains byte-identical (exact counts/hashes in `seal.json`).
Source-only whitespace checks pass. Authentic raw Cargo logs retain their EOF
blank lines; full-evidence diff whitespace diagnostics are recorded rather than
editing captured logs. Only app-local preview source/tests,
the parity clarification and fresh additive correction evidence change. No
production Home/framework source/pins/root manifest or lockfile, SDK, daemon,
credential/provider/config, real service or paid call is changed. No effectful
administration, push/deploy, approval or integration occurs. Synthetic export
asserts no chat command sender and no administrative worker sender.

Nine settings routes remain read-only/local-preview settings, not editable
production parity. Chat/Settings/Schedules are the three implemented shell views;
15/18 rail destinations remain deferred. Native pane/tab/workspace fidelity is
follow-on scope. Native/IME repair remains separately scoped and all five
input-sensitive gates stay denied.

Concrete next safe slices after sender-arranged independent verification:
1. Production-derived settings chooser/rail grouping and keyboard/accessibility
   selected-state fidelity, with actual glyph/focus pixels and below-fold checks;
   no native pane/tab/workspace rewrite implied.
2. One bounded read-only Activity/jobs list/detail slice using existing safe
   summaries, explicit loading/error/empty/filter/long-list behavior; no execute,
   retry, cancel, reveal or approvals.
3. Versioned catalogue/worker-capacity wire fixtures, visible pending/cached/fresh
   distinctions and bounded response-size/pagination design; no live providers,
   credentials, configuration writes or operations.

Sender retains independent correction verification and native integration.

Source correction commit: `542fe2dc98582a628e93c7f9aa46cb41cce3e786`.
Source tree: `f10e1070a2d24d8180c7786fc3a41be5ce4a4ca7`.
