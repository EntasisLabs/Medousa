# Follow-on implementation handback: Settings, rail and read-only Activity

## Authority, exact revisions and review boundary

- Undertaking: `work-eb0d9b4798ae44377db6e5a4400978ca`, fresh admitted coder session.
- Actual clean starting HEAD: **`9d51646838cba3571eaec994d5919ef7b2bf6be2`**.
- Final source / tested / rendered revision: **`76f624954de8efca5613bc9c0b998e1dbd315a75`**.
  The subsequent evidence-only commit is the final handback HEAD reported to the
  sender. A commit cannot include its own hash; `seal.json` anchors source and
  binary provenance, and the final HEAD is returned separately.
- Local implementation commits, in order: `da6f0db4728967450bc0ab15e0ddfbba27081586`
  (feature), `4b28d210326238080f651027691d421c40604e61` (compact pixel refinements),
  `2fe38b0e9134afcc7d4dc36dccf17789b295ca76` (fixture scheduling tolerance),
  `76f624954de8efca5613bc9c0b998e1dbd315a75` (compact Activity and fixture socket mode).
- This is a **worker implementation handback**, not self-acceptance, independent
  review, integration, deployment, push or production readiness. The sender owns
  independent review, corrections and native integration. No terminal undertaking
  record or earlier evidence was rewritten.

## Delivered outcome

1. **Settings chooser:** all nine existing reference sections, Medousa/This host
   grouping, current-view semantic names and hints, last section remembered within
   this window, direct choice at wide/narrow widths. Stable header anchor survives
   resize. Existing popup owns Escape/outside dismissal and trigger focus return.
   App-local ordinary single-key navigation follows section order for Tab/Shift-Tab,
   arrows and Home/End; activation uses existing buttons. No editor/composition or
   mixed batches are consumed by that helper, and no native-input gate is relaxed.
2. **Rail:** separately scrolling primary doors, Focus/Automations grouping, fixed
   Activity/Settings/You dock at minimum height, correct active/current semantic
   destination and return focus. Compact heading uses Auto visually with the full
   Automations semantic name. This is partial static reference grouping, not native
   icon rail/panes/tabs or the production folded Automations explorer.
3. **Activity · jobs snapshot:** real existing SDK workspace-cards GET, request and
   projection capped at 100, including terminal work cards. Generic local titles,
   controlled board-state enum, ID, created/updated UTC and revision only. Detail
   reuses this snapshot, not a second effectful/read endpoint. Prompt-derived titles,
   raw status prose, outputs, diagnostics, payloads and worker transcript are omitted.
   Board state is explicitly not execution-success proof. This is not the entire
   durable job ledger, global Activity feed, assignment browser or live subscription.
4. **Read behavior / isolation:** local filtering, detail/Back/refresh; Back restores
   list viewport and row focus; canonical focus visibility may reveal a row. Detail
   refresh clears positional selection before new data. Explicit fixture/live-read/
   retained-offline source labels, existing loading/error/permission/incompatible/
   disconnected/empty/no-match distinctions, stale-serial rejection and unrelated
   dataset update isolation remain. No simulated successful administration exists.
5. **Chat retained:** draft, session, transcript/scroll/virtualization and Jump to
   latest remain mounted and tested. Send, Restore, New, Recent and Reload remain
   denied. No daemon/provider/credential/config operation, effectful job control,
   production Home change, rndr investigation, framework/pin/root dependency change,
   push or deployment was performed.

[Updated parity matrix](../SHELL_ACTIVITY_PARITY.md) separates functional read-only,
local-preview and deferred routes. Nine settings destinations are still subsets,
not editable panels; **14/18 rail destinations remain deferred** (previously 15/18).
Activity is only a bounded functional read-only subset. Device/live checks are
explicit final human items, not blanket blockers on further safe work.

## Verification: exact final-source results

Reproduce from repo root:

```sh
sh apps/rndr-home/docs/shell-activity-evidence/reproduce.sh
bash scripts/verify-docs.sh --strict
```

`results.tsv`, `*.command.txt` and matching logs hold exact commands and exit codes.
All final reproduction steps returned **0**:

| Check | Final result |
|---|---|
| `cargo fmt --manifest-path apps/rndr-home/Cargo.toml --check` | Pass |
| `cargo test --manifest-path apps/rndr-home/Cargo.toml -j 2` | **6 library + 79 app tests**, 0 failures; 0 doc tests |
| Same with `--features observe` | **6 library + 87 app tests**, 0 failures; 0 doc tests |
| `cargo build` on that manifest, default and observe, `-j 2` | Both pass |
| `cargo clippy` on that manifest, `--all-targets`, default and observe, `-j 2 -- -D warnings` | Both pass |
| Offline `cargo run … --features observe … -- --export-desktop …/renders` | **312 actual GPU PNG exports**, pass |
| `bash scripts/verify-docs.sh --strict` | See `docs-strict.log` and `docs-strict.command.txt` for final receipt |
| Source/docs whitespace and protected-path comparison | See `boundary-checks.txt`; prior evidence whitespace unchanged |

Eight added tests (seven shell/Activity tests plus one dedicated backend cap/schema
case) and the expanded eleven-model GET fixture cover keyboard/focus, responsive
anchor stability, current semantic names, dock visibility, list/detail/filter/Back
scroll and row focus, first useful metadata at 480×650, stale/unrelated updates,
chat state and privacy. All prior chat/input/observation regressions remain active.

### Findings corrected during this pass (do not confuse intermediate failures with final results)

- New chooser test reproduced Tab moving backwards after recycled page entities.
  The app-local section-order helper fixed that; final focus/keyboard tests pass.
- The first detail-scroll assertion captured position **before** canonical focus
  revealed an offscreen row. The test now records the real pre-activation viewport;
  it does not mislabel normal focus visibility as a scrolling defect.
- Actual pixels showed a missing chooser chevron glyph, clipped compact Automations
  label and a crowded wrapped fixture title. Removed the unsupported glyph, used
  Auto/full semantic name and reserved 62px for narrow Settings headers. The first
  Rect-only header check passed even before correction: clamped Rects were not
  sufficient ink evidence. The added policy assertion and actual pixel inspection
  are reported separately; no invented fail-before pixel proof is claimed.
- Actual 480×650 Activity detail pixels showed repeated explanations pushing all
  metadata below the fold. Compact copy/header now expose the first board-state
  value without scrolling; a real Parley/layout regression protects that behavior.
  Later metadata intentionally remains scrollable.
- One intermediate observe run hit `WouldBlock` in the loopback fixture server,
  not a production read. Fixture accepts now explicitly select bounded blocking
  reads; scheduling/read budgets are 15s. Method/path/no-auth/privacy assertions
  stay exact and production 12s read deadlines are unchanged. The failed run is
  preserved as `test-observe-fixture-timeout.log`; both final configurations pass.

Source/docs whitespace checks pass. The **whole staged diff** returns 2 from
`git diff --cached --check` solely for preserved final blank lines in five new
evidence logs: `activity-fold.log`, `focused.log`, `header-before.log`,
`test-default.log`, `test-observe.log`. Diagnostic logs remain untouched; this is
not an implementation whitespace or test failure. Prior evidence is unchanged.

The toolchain emits pre-existing Cargo manifest `redundant_homepage` metadata
warnings. Strict scoped Rust lint succeeds without allowances. No root engine CI,
production Svelte build or workspace-wide lint was run: only app-local preview and
user-facing guide/index changes are present.

## Actual rendered-pixel inspection ledger

Exporter: app's existing `rndr_render::Exporter`, real GPU rendering + Parley text,
synthetic data only, **no network**, no native-window capture. Every sample visibly
says FIXTURE / SYNTHETIC GPU RENDER. Export matrix: **26 cases × 4 sizes × 3 themes**:
1120×820, 480×760, 1120×650, 480×650; dark, light, dark/high contrast.
Every nine-section Settings route, open chooser, existing admin samples and seven
Activity states are exported. Export success is not pixel inspection.

**Eleven final PNGs were actually opened and visually reviewed**, not all 312:

| File under `renders/` | Palette / size | Inspected behavior |
|---|---|---|
| `dark-wide-settings-chooser.png` | Dark, 1120×820 | Popup layout, current section, scrollable groups, fixed dock |
| `dark-narrow-650-settings-chooser.png` | Dark, 480×650 | Two-line header allocation, no missing glyph, compact group, popup bounds |
| `light-narrow-650-settings-chooser.png` | Light, 480×650 | Same compact chooser, current section, dock and readable contrast sample |
| `high-contrast-wide-650-settings-chooser.png` | High contrast, 1120×650 | Popup/current section, sidebar scroll and dock |
| `dark-narrow-activity-list.png` | Dark, 480×760 | Compact bounded summary, two list rows, fixture labels, no effectful controls |
| `light-wide-650-activity-list.png` | Light, 1120×650 | List/refresh/search layout, truthful fixture source, fixed dock |
| `dark-wide-activity-detail.png` | Dark, 1120×820 | Board-state/ID/time metadata and Back, remaining scrollable fields |
| `high-contrast-narrow-650-activity-detail.png` | High contrast, 480×650 | First useful metadata now visible; later fields intentionally scroll |
| `light-narrow-activity-error.png` | Light, 480×760 | Unavailable state and safe error text rather than a fabricated list |
| `high-contrast-wide-activity-empty.png` | High contrast, 1120×820 | Explicit empty fixture snapshot, not an error or liveness assertion |
| `light-wide-preferences.png` | Light, 1120×820 | Grouped sidebar, appearance controls and independent fixed dock |

`inspected-pixels.json` records each original PNG file's SHA-256 and dimensions
**separately** from its viewed image-tool artifact SHA-256. The tool emits a PNG
rendition, so its hash can differ from the original file bytes even at the same
dimensions; those are not interchangeable hashes. The three chooser samples were
reopened after the final export, not accepted on an assumed raw-byte equality.
`render-sha256.txt` inventories all 312 original final exports. Tool rendition
hashes are a separate inspection receipt layer, not replacements for that inventory. Other case/theme/width pixels,
especially offline/loading/permission samples and unlisted Settings routes, are
**exported but not individually visually inspected** in this pass. Synthetic layout
checks cover a wider matrix but are a different evidence layer.

Preserved before-pixels: `pixel-before/` is from `da6f0db4` (three initial samples);
`pixel-before-activity-compact/` is from `2fe38b0e` (one minimum-height detail sample
plus environment record). They are historical findings, not final-source evidence.
No full accessibility/theme certification or blanket all-images signoff is implied.

### Exact provenance / limitations

`environment.txt` records final render source, UTC, rustc/cargo versions and Darwin
arm64. `cargo-metadata.json` records package/manifest/target paths. `seal.json`
records source-tree hashes, binary SHA-256 and evidence hashes. The standalone
manifest/lockfile and root dependency files compare unchanged to admission.
The pre-existing rndr inputs are absolute **local path dependencies**; no external
framework source revision was investigated or independently qualified here. The
exact rendered executable is hash-stamped; GPU adapter identity was not captured.
This bounds provenance honestly instead of claiming an unseen pinned framework
commit or physical device result. Prior qualification remains its own evidence.

## Remaining work / sender and final human checks

- Sender: inspect the exact sealed diff, independently rerun focused checks and
  selected images, arrange corrections if needed, and use native integration gates.
  This worker has not self-accepted or integrated anything.
- Principal on real devices: physical keyboard traversal/dismissal and focus
  visibility, screen-reader announcements/bridge, pointer/scroll, scaling, native
  windows, themes/widths and any uninspected image combinations.
- In a separately authorized live environment: verify actual workshop identity,
  work-card data/order/timestamps, permission/offline/error/empty behavior, refresh
  and locality/privacy. No real-daemon/provider/credential/config checks occurred here.
- Keep all five sensitive chat actions denied pending their separate rndr/native
  input proof. Do not turn that concern into a blocker on unrelated read-only work.
- Broader shell/pane fidelity, folded Automation explorer, complete settings editors,
  host package inventory, unified Activity/live subscriptions and 14 deferred rail
  doors remain future slices. Nothing here claims production readiness.
