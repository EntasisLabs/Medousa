# Existing preview qualification — rndr 0.6.0, 2026-11-13

## Verdict and scope

**Step 1 passes within the synthetic/offline qualification boundary.** The
existing app builds and its tested interactions work against the pinned current
rndr main. No production app-side Rust API repair was necessary; do not replace
working adapters just to produce a larger compatibility diff. This is not
approval to implement the desktop replica, nor certification of connected or
physical-device behavior. Sender retains final review/native integration.

- Medousa input baseline: `333a14d823ea09c48fdb3bf6224f7e7960029495`.
- Isolated branch: `worktree/medousa-rndr-home-current-main-preview-requalification`.
- Forge work: `work-7fd193aac6facae5335d43860ce1bcce`.
- Framework, read-only: `/Users/theelevators/rndr`,
  `44f5dc7c0c91af56e4f3e29f97283b8e8d1e1139`, package version **0.6.0**.
  HEAD/status were checked before the first compilation and again after native
  capture; the final audit repeats that check. HEAD is unchanged and porcelain
  status is empty. No framework files, refs, caches, or Python bytecode were written.
- Results commit: see the worker's terminal handback/native review receipt;
  this document intentionally does not embed its own circular commit hash.
- The 0.4.0 results in [VERIFICATION.md](VERIFICATION.md) and historical sections
  of [INTERACTION_PARITY.md](INTERACTION_PARITY.md) are **not** fresh evidence.

## Bounded changes

1. `Cargo.lock`: five local rndr packages advance **0.4.0 → 0.6.0**. Newly declared
   edges are `rndr_core → web-time` and `rndr_render → js-sys,
   objc2-foundation 0.2.2, wasm-bindgen, wasm-bindgen-futures, web-sys, web-time`.
   All those package/version entries were already present. No registry package
   upgrades, manifest feature changes, SDK/protocol changes, or substitute
   framework/fallback sources were introduced. The first ordinary offline test
   resolved these local-package metadata changes; subsequent gates use `--locked`.
2. Four focused tests in `src/qualification_tests.rs`, registered only under
   `cfg(test)`. They reproduce two remaining framework contracts, exercise real
   Parley Markdown measurement/resize/update/Copy, and check Lucide atlas data.
   All production behavior, including the conservative input guard, is retained.
3. This report, raw gate logs, provenance, fresh synthetic PNGs, reproduction
   runner, and historical-document pointers. All changes stay in `apps/rndr-home`.

`apps/medousa-home`, engine, SDK, protocol and packaging are unchanged. No other
worker was launched. No push, publication, deployment, production daemon mutation,
private chat inspection, credential loading, or paid inference was performed.

## Commands, cache and disk policy

The already existing shared target was inspected (~14 GiB before this pass) and
reused, not duplicated. No broad cache cleanup was performed. Existing root
`.cargo/config.toml` was inspected; the app has its own standalone workspace and
already disables symbols/incremental in dev/test profiles. Explicit environment:

```sh
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/b906e0f57650/.cache/cargo-target'
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
```

Initial commands included `git status --short`, `git rev-parse HEAD`,
`git -C /Users/theelevators/rndr rev-parse HEAD`,
`git -C /Users/theelevators/rndr status --short`, `df -k .`, cache/config inspection,
and the following first fresh compile:

```sh
cargo test --manifest-path apps/rndr-home/Cargo.toml --offline -j 2
```

That unchanged-source baseline passed **50 tests** (6 library + 44 app/backend).
The first observe run passed **51** (6 + 45). Four new qualification tests then
passed in isolation; no failing compiler/test workaround was needed. Initial
commands/results are retained in `qualification-44f5dc7c/default-baseline.log`,
`focused-initial.log` and this account. The final runner supersedes the initial
observe log with the expanded suite.

The final exact commands, environment, timestamps, exit codes, binary hashes and
250ms disk samples/minima are in
[run.json](qualification-44f5dc7c/run.json). The executed orchestration is
[run_checks.py](qualification-44f5dc7c/run_checks.py). It stops the command's entire
process group at **8 GiB** free and also checks the floor before every command.
All Cargo commands use `--offline --locked -j 2`; no two compilations ran
concurrently. Minimum sampled free space in the guarded final run was about
**15.28 GiB**, safely above the floor; initial free space was about 17 GiB.
No floor stop was needed. Earlier baseline builds were manually disk-monitored.

For a fresh replay, copy `run_checks.py` into a new sibling directory under
`apps/rndr-home/docs/` and execute that copy from the repository root with the
same selected `CARGO_TARGET_DIR`. The copy's directory receives the evidence;
its parent depth must stay the same. Do **not** overwrite this run's provenance:
`capture_native.py` deliberately rejects an existing output prefix. The framework
must still be clean at the pin. Commands are also individually replayable from
`run.json` with a new output prefix/directory. `cargo fmt` was applied before the
final run; `cargo fmt -- --check` passes. A final docs/diff audit is logged separately.

## Fresh gate results

| Gate | Result |
|---|---|
| Default focused app suite | **54 passed**: 6 library + 48 app/backend/renderer |
| Observe-feature focused app suite | **55 passed**: 6 library + 49 app/backend/renderer |
| Default strict clippy, all targets | Pass, `-- -D warnings` |
| Observe/all-features strict clippy, all targets | Pass, `-- -D warnings` |
| Default native dev build | Pass |
| Observe native dev build | Pass |
| Formatting | Pass |
| Dark/light/high-contrast exports | **27 PNGs**, 9 fixture states each |
| Native offline observation | Pass, same-frame metadata + PNG, timed clean exit/capability cleanup |
| Strict repository docs verifier / diff whitespace | Pass |

Cargo emits pre-existing `cargo::redundant_homepage` **manifest** warnings in rndr
and the Medousa root package; these are not rustc/clippy diagnostics and did not
make strict clippy fail. They were not suppressed or cleaned up here. Monorepo
engine/Tauri suites, cross-platform builds and packaging were not run: no source
in those surfaces changed and this assignment is the native preview slice.

Native binary SHA-256 values recorded immediately after each build:

- Default: `5b691fdf44be96752a3ccaaf2bc9dba6133633a11a28bd9bbc83931a4073d217`.
- Observe: `c684d5c0533ad6525fad97e163b3660d544bce906f69dfc1738107ee67abc5d9`.

The observe binary is the one used for all final exports/native capture. Build
hashes are local artifacts, not portable/reproducible-build claims. The source
and evidence manifests identify the files tested/captured; final documentation
edits do not change the tested Rust/lockfile content.

## Interaction qualification and framework contracts

All input below is synthetic ECS input or a loopback HTTP/SSE fixture. None is a
claim of a physical keyboard, IME, clipboard device, accessibility client or
configured daemon being exercised.

- **Streaming/transcript:** existing tests cover V3 sequence/replay rejection,
  Unicode and interleaved segments, pending/admitted/terminal/uncertain state,
  generation fencing, durable-history reconciliation, fatal text retention,
  no automatic resend, stable message/view identity, follow-bottom versus reading
  above, exact raw Markdown Copy, and 1,000-message viewport eviction/remount.
  Backend tests use real SDK requests against synthetic loopback fixtures only.
- **Composer:** plain Enter, Shift+Enter multiline, IME-enabled flag versus active
  preedit, growth/six-visual-line cap/shrink, send-once/reset, failed dispatch and
  recovery, draft preservation across resize/theme/scroll/menu actions all pass.
- **Popup sizing:** existing `popup_content_follows_anchor_width_without_horizontal_overflow`
  uses real Parley at **520 and 1120 logical points** for all popup kinds.
  Search/exact original catalog index, pointer selection, outside dismissal,
  Escape, focus restoration, FIFO typed appearance intent and disabled runtimes
  remain covered. Current sizing is anchor-based with a minimum 240pt panel,
  viewport-capped; the existing Fill-content adapter still fits. Preferred panel
  width/listbox navigation are later app/framework design issues, not fixes here.
- **Markdown measurement:** the new Parley test measures at 1120pt, narrows to
  520pt (height grows), appends Markdown to the same retained view (height grows
  again), and checks exact source Copy and the sole outer transcript Scroll.
  `Size::Hug`, no nested Scroll, and the existing **16pt inset** still apply.
  No public inline-document measurement migration was made. This is a tested
  adapter coupling, not proof of every Markdown construct or huge-message bound.

### Exact minimal reproducers / classification

Run either test with the common bounded environment above:

```sh
cargo test --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2 \
  qualification_tests::framework_prompt_submit_is_still_end_of_batch_not_enter_snapshot -- --exact
cargo test --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2 \
  qualification_tests::framework_prompt_reset_requires_marker_but_app_removes_it_synchronously -- --exact
```

1. **Framework chronological-submit limitation; nonblocking with existing app
   mitigation.** A bare `App + UiPlugin`, focused `Label("hi") + Prompt +
   TextField + Focusable`, caret at end, and one input batch `[Enter, '!']`
   emits `Submitted.text == "hi!"`, not the Enter-boundary `"hi"`. The first
   test is framework-only and documents actual behavior rather than expecting a
   nonexistent API. The app's existing ambiguous-batch guard refuses to send,
   retains `hi!`, and asks for a fresh Send/Enter. Its independent regression
   `ambiguous_queued_submit_is_not_sent_and_preserves_newer_edits` passes.
2. **Framework reset API/opt-in limitation; nonblocking with existing app
   mitigation.** `reset_field_draft` returns false on an ordinary Prompt without
   `FieldTransactions`. The second test then calls the existing app `reset_prompt`
   adapter, proves reset/clear and zero Prompt scroll, and proves the marker is
   immediately removed. Leaving it installed would opt the Prompt into different
   transactional behavior. No framework edit or alternative input system was used.
3. **Markdown intrinsic measurement contract / panel preference:** app-owned
   coupling and future API opportunity, not a current compile/runtime blocker.
   The new measurement test plus existing popup test are the bounded repros.

**No blocking framework incompatibility was observed in this exercised scope.**
These limitations should be explicit contracts in later team work, not silently
patched in the read-only renderer checkout.

## Lucide evaluation (availability, not redesign)

The current checkout includes typed `rndr_core::Icon`, generated scalar/name
mapping starting at U+E100, the ISC Lucide subset baked into `rndr_paint`, public
`icon_atlas_size` / `icon_coverage`, and the framework's icon fallback integration.
`lucide_atlas_has_nonempty_coverage_for_existing_control_candidates` verifies
nonempty **16×16 coverage** and scalar round-trip for Plus, ArrowUp, ChevronDown,
Copy and X. No download, new icon dependency or runtime remote asset is needed
for those candidates. The historical blanket “no native icon system” statement
is therefore no longer current. This pass deliberately retains existing labels,
ASCII `v`, send arrow and controls. It does **not** certify all atlas sizes,
all rendering paths, optical alignment or accessible labels of a future icon-only
UI; those require a separate scoped visual/semantic migration.

## Evidence provenance and visual assessment

[Evidence directory](qualification-44f5dc7c/) contains the original emitted PNGs
and logs. `artifacts.json` records PNG dimensions and SHA-256 digests;
`SOURCE_SHA256.txt` fingerprints the app source, lockfile and capture/runner
scripts; `SHA256SUMS` fingerprints the evidence without a self-referential hash.
Cargo's four test logs ended with redundant blank lines; the staged whitespace
check caught these after the initial audit. Only terminal newlines were trimmed
for the commit, with original/stored hashes and reversible removed-byte counts
in `log-normalization.json`; no command output diagnostics or test results were
removed. The staged whitespace check was rerun successfully.

- **Headless exports:** `dark/`, `light/`, `high-contrast/`, each with `empty`,
  `chat`, `narrow`, `draft`, `models`, `behavior`, `depth`, `runtime`, `appearance`.
  Source is the existing `--export` fixture in `src/main.rs`: synthetic two-message
  transcript, fenced code, literal user prompt, fake model catalog and multiline
  draft. Dimensions are 1120×820, except `narrow` at 520×760. There is no worker,
  real session, credential or inference. Export is offscreen rendering, **not a
  submitted native window or OS screenshot**.
- **Native submitted-surface copy:** `native-empty.png` and `native-empty.json`
  are one scoped offline runner frame, revision **3**, 1120×820 at scale 1.
  Metadata says `source=live-surface-copy-before-present`,
  `presented_confirmation=false`; Metal / Apple M4 / Parley / scene path,
  `Bgra8UnormSrgb`, four scene samples, `AutoVsync`. This verifies an actual
  native runner's surface-copy path, **not compositor presentation**. The runner
  exits normally and removes the observation descriptor; no capability token is
  retained. The existing observer refuses public pixels after draft/history/live
  text appears; its denial regression passes.
- **Actual compositor observation:** **not performed**. No desktop-wide capture
  was taken, avoiding other apps/private data. Native semantics metadata is not
  a screen-reader interaction test.

The image viewer was used on dark chat/narrow/draft, light models,
high-contrast appearance, and native-empty captures. The narrow transcript and
multiline draft show retained reading/composer lanes and clipped outer scrolling
rather than horizontal panel overflow; native empty chrome and disabled Send
render normally. These are bounded synthetic checks, not pixel-perfect Svelte
parity or comprehensive visual regression certification. All 27 files were
emitted successfully; no claim is made that every pixel/appearance/platform was
manually certified. The optional contact-sheet attempt could not run because
system Python lacks Pillow; no dependency was installed and original PNG viewing
remained available.

## Explicit residual gaps / blockers for later stages

- No fresh live daemon/auth/provider, paid inference, real network reconnection,
  actual user-session mutation, physical keyboard/IME/composition ordering,
  native paste/clipboard device, screen reader, trackpad or compositor validation.
- Existing preview is not the production Home replica: external runtimes,
  approvals/tools/artifacts, attachments/voice/skills, navigation and persistent
  preferences/drafts remain unimplemented or visibly disabled as documented.
- Variable-height history still scans O(n) row metadata, estimates offscreen
  heights, loses local selection on eviction, and mounts one huge message whole.
  This pass reruns bounded-mount tests; it makes no new RAM/performance claim.
- Framework path dependencies are machine-local and not a distributable pin.
  This qualification records an exact verified checkout hash; later packaging
  must choose an explicit reproducible source policy without silently changing it.
- No step-1 blocker in the executed matrix; device/live/parity gaps must not be
  interpreted as completed acceptance criteria for subsequent steps.

## Recommended later team boundaries — planning only

Do not launch these lanes until the owner agrees contracts/assignments. Keep
one integrator responsible for `main.rs`, dependency/feature selection and shared
IDs/action routing; having every coder independently rewrite it would collide.

| Suggested ownership lane | Existing seam / contract to preserve |
|---|---|
| Shell/navigation/session/draft owner | Extract orchestration from `main.rs` behind agreed interfaces; session switch generation fencing, unsent draft scope/storage policy, disabled capability truth; production Home unchanged |
| Transcript/rendering owner | `conversation.rs`: canonical borrowed message source, stable IDs versus row indices, measured-height invalidation, single Scroll/follow-bottom, virtualization eviction and exact Copy; no authority from Markdown |
| Composer/controls owner | composer assembly + `menus.rs`: exact submit boundary, marker-scoped reset, draft ownership, exact provider/model identity, popup focus/geometry, typed action FIFO; coordinate with shell on intent routing |
| Transport/session-state owner | `backend.rs` + `lib.rs`: existing SDK only, V3 replay/sequence and generation contracts, durable settlement, unknown admission/recovery/no-resubmit, independent mode errors; no protocol changes hidden inside UI work |
| Shared presentation owner (one, not competing reskins) | `presentation.rs`: UiTheme bridge, retained surface IDs, reading measure, palette/typography and icon semantics; agree tokens with transcript/composer before integration |
| Reviewer A: contracts/safety | Independent review of transport/state, credentials/data locality, observation privacy, authority/approval and admission/recovery boundaries; tests should assert actual failure contracts |
| Reviewer B: UI/behavior/evidence | Independent review of measurement/focus/popup/virtualization, synthetic and later controlled native/device checks, visual provenance and reproducible resource-bounded verification |

First settle message identity (durable history versus stream segment IDs),
session-scoped draft persistence, host-intent namespace, model/runtime preference
ownership, and intrinsic Markdown height/resize contract. Rich cards/attachments
must follow daemon/vault locality and explicit admission, not derive executable
authority from Markdown. Stage shell/session and read-only projection contracts
before parallelizing feature parity; reviewers should inspect sealed slices and
then the integrated behavior. These are recommendations, not new implementation
or delegation authority.
