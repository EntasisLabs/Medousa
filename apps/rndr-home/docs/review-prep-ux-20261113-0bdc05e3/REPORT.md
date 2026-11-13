# Independent desktop UX / evidence-integrity preparation review

## Verdict: PASS — behavior-preserving preparation only

Candidate: `bdef72eb7fbf8014067603f50bb15cfe0a3745f5`.
Baseline: `431ccab3affe24725eaff7d39176661b3398a17f`.
Source extraction: `08839824203c57588607f3ad66496e1bd81d8cba`.
Observe coverage addition: `b9ee1f1b55fd984231dcb8082fddd745444d5986`.
Undertaking: `work-7fd193aac6facae5335d43860ce1bcce`, existing isolated worktree.
Reviewer lane: independent desktop UX/evidence integrity, not architect or correctness reviewer.

No blocking/high/medium regression found in this file-ownership extraction. Two
low, pre-existing test/evidence gaps are listed below; neither is a new regression
nor a claim of physical/device qualification. This is not acceptance, integration,
full parity certification or permission to launch feature workers. Sender retains
acceptance/native integration; both prep reviews must be assessed before fanout.

## Method and execution boundary

Read the complete `REVIEW_BRIEF_DESKTOP_UX.md`, `TEAM_PREPARATION.md`,
`SPLIT_VERIFICATION.md`, and `INDEPENDENT_REVIEW_0_6_0.md`. Inspected moved
main/composer/controller/shell/fixtures bodies, menus, conversation renderer,
interaction/UI/qualification/observation assertions, capture script, saved runner,
source manifests, execution records and native metadata. Inspected the extraction
map against source, rather than accepting only the architect narrative.

Resource override followed: **no Cargo, compiler, app, test binary, export or native
capture execution in this lane**. No new images or fresh Rust test results are
claimed. Fresh independent work here consists of read-only hash/dimension/source
inspection and original-image viewing. The correctness reviewer alone owns fresh
matrix/27-export reproduction; their result is separate and not assumed here.
Architect records below remain architect evidence, not this reviewer's execution.

Initial HEAD equaled the exact candidate; tracked diff was empty. A separately
owned untracked `review-prep-correctness-20261113-1304/` directory already existed
and was not modified. This reviewer writes only this unique untracked namespace.
No workers, commits, source/tracked-doc/HEAD changes, merge, acceptance, feature
work, credentials, private chat, daemon, paid inference, network mutation, desktop
capture, push, deploy, SDK/protocol/framework edits. `apps/medousa-home` has no
baseline-to-candidate diff. The app manifests, lockfile, capture script and historical
qualification/report files are byte-unchanged from baseline. The external rndr
checkout was not accessed or mutated by this review; saved records bind it to
`44f5dc7c0c91af56e4f3e29f97283b8e8d1e1139`. This is not a fresh external-checkout
cleanliness attestation.

## Exact visual coverage (original paths opened with cognition_image_view)

**27/27 final offscreen originals**, not a contact sheet or metadata-only review:
`apps/rndr-home/docs/split-evidence/final/{dark,light,high-contrast}/`
with **every** case below opened separately in each theme. Also opened the
committed `final/native-empty.png` original. Viewer renditions are tool-generated;
their encoded bytes/hashes differ from source PNGs. Source values are independently
read from the original files and tabulated below.

| Named case | Dark | Light | High contrast | Visual/evidence assessment |
|---|---|---|---|---|
| empty | opened | opened | opened | Centered empty question, retained composer and explicit offline/no-inference status; unavailable controls not presented as working parity. |
| chat (wide transcript) | opened | opened | opened | Readable bounded assistant lane and differentiated user bubble; code block and assistant Copy control remain in the transcript. |
| narrow transcript | opened | opened | opened | 520×760; wrapped content in outer history scroll, bounded composer/control lane and visible rail. Top clipping at the scrolled viewport is not evidence of missing history. |
| draft | opened | opened | opened | Expanded multiline composer retains space for transcript; finite height consistent with six-visual-line cap. A still image cannot demonstrate internal draft scrolling. |
| models | opened | opened | opened | Search field/provider grouping/current model visible in bounded overlay; selected catalog identity is separately assertion-inspected. No nonempty search query is captured in these fixture images. |
| runtime | opened | opened | opened | Bounded vertically scrollable menu; external runtimes say unavailable/not connected rather than promising operation. Not every offscreen runtime row is simultaneously visible. |
| behavior | opened | opened | opened | Depth/appearance routes distinguished from disabled not-ported controls and local next-turn scope. |
| depth | opened | opened | opened | Standard selected; concise/deep choices and window-local/not-defaults caveat retained. |
| appearance | opened | opened | opened | Light/dark/high-contrast controls and window-local/not-saved disclosure retained; theme inspection is not an accessibility contrast-ratio certification. |

No intended/new visual appearance was accepted by replacing an expected image.
All 27 final cases have exactly the same original bytes as all three comparator
sets, so the visual inspection also applies to their equivalent bytes. I did not
separately open all 81 duplicate comparator files. No 480pt export, narrow-popup
screenshot, populated recovery screenshot, physical interaction or compositor
capture is claimed. Popup fixture images are wide; source assertions add synthetic
520pt containment, not a photographed/device gate.

## Independent consistency checks and source binding

- Recomputed all **150/150** split-evidence SHA256SUMS entries successfully.
- Recomputed **27 four-way** original PNG comparisons: before/after/final plus
  historical `qualification-44f5dc7c`, all equal. The 27 rows below represent
  **108 original files** checked; narrow is 520×760, all other exports 1120×820.
- Compared each recorded SOURCE_SHA256 entry directly to `git show` at its
  designated source commit: before **12/12** at baseline, after **17/17** at
  extraction commit, final **18/18** at coverage commit. Final committed source
  remains unchanged under the candidate's docs/evidence-only sealing commit.
- Independently reran the saved bounded source-body audit with its sole tracked
  output write removed in-memory: **19 checks pass**, including 11 moved/rebuilt
  body checks and eight byte-identical implementation/test files. This executes
  only Python string/source comparisons, not app/build/tests. Whitespace,
  private visibility and formatting trailing commas are normalized by that audit;
  it is not a Rust semantic proof. Source inspection and image equality provide
  complementary checks.
- All six normalized test logs reconstruct their recorded original hashes by
  appending exactly one LF. No missing diagnostics were inferred from truncation.
- Saved records: before and pure split 54 default / 55 observe; final 54 / 60.
  These counts were checked in saved log summaries, **not rerun here**. Every
  successful stage has 13 recorded command exits of zero, reusable target,
  two-job and debug/incremental-off settings. Source-changing rebuilds are
  architect-reported cached-dependency execution, not clean rebuilds.
- Before run HEAD is baseline; after run HEAD still baseline while extracted
  source was uncommitted; final run HEAD is extraction while denial-test additions
  were uncommitted. Explicit source manifests, not starting HEAD alone, bind
  those records to the subsequent source commits. Historical manifests are not
  relabelled. Hashes are consistency checks, not signed execution attestations.
- Architect's sampled disk minima: 16,411,328,512 / 16,381,644,800 /
  16,362,594,304 bytes for before/after/final. The runner samples at 250ms and
  process-group-stops below 8 GiB. This is not a complete disk time series.
  Initial failed Arc-import attempt is retained and excluded from pass counts.

### Source PNG dimensions, bytes and SHA-256

Each row is independently equal across the four sets named above.

| Theme / case | Dimensions | Bytes | Source SHA-256 |
|---|---:|---:|---|
| dark/empty | 1120×820 | 35443 | `3dcf579ffbd0b3243b86a0f629c87ab1a335ba5423bf4e0d522176603a46958e` |
| dark/chat | 1120×820 | 85354 | `12ae7138ab37831a12a357059acb385f7ff5614524a8ccc8685f15a71a283e67` |
| dark/narrow | 520×760 | 75692 | `45dfc76f050a75926c0ff4a043125bafeb412739932d6e2287296c37e1f4569a` |
| dark/draft | 1120×820 | 93821 | `87ba37152f655a62d764e5be5ef689b8886fb7819caa10d1e7b363e0745220b1` |
| dark/models | 1120×820 | 83646 | `861463504c2f004aea02798ecb1f27ea0a72b8032b0b775556caa14811b67086` |
| dark/runtime | 1120×820 | 89379 | `a75068a7bf3bf31d949a340a2275f3dbfbc09c7fe4a483a2c0cb9bbfd3cd5ad5` |
| dark/behavior | 1120×820 | 101313 | `2fad49219d5b505a71cac86b67eeb290a482bc47a5f8994c2146c738fa604a15` |
| dark/depth | 1120×820 | 96555 | `2feb5756a41aa8b434c0ebb9cc55e61dbb5d4b04fa6354a3eccf495b1aa2afe1` |
| dark/appearance | 1120×820 | 98520 | `8a62170ae74e960eb838f710827dcfebf4952be453676538bc9d6ac66300e75b` |
| light/empty | 1120×820 | 33412 | `db31c0c01b8b3a48fb55bb2bdbb6b7b0a2f901dfcb0b9558ddaee123b77fbac4` |
| light/chat | 1120×820 | 81752 | `0de93ef39ce9a8b7bb5827fc5f1abb2b5261b6f7058bebfd534b386fa7fa46b4` |
| light/narrow | 520×760 | 71926 | `e68f02dd7dba27ddd500903c98fccc928b1a6053f10af4fe6a7b139d13fd2a17` |
| light/draft | 1120×820 | 90800 | `bd10160b71de7eddf0e9bbed93885205d5575b4d9a39c94f7648ebe83e7258ab` |
| light/models | 1120×820 | 80026 | `19684aeefaa9b7e8a9b9510f281ff88c8a5a96bd7c19659d0bd76659d2f0edbe` |
| light/runtime | 1120×820 | 86302 | `6bd140b1322862ba57949146248e4b9669f5f192127ac3abc3052fbb8575f741` |
| light/behavior | 1120×820 | 97137 | `6e6778d715f6b24fa92dfb94f74a8a88dd0a95d77da1d95737df3df5c305de50` |
| light/depth | 1120×820 | 92476 | `480c7c3a1e559ef9c1b7fb206253d5fcf7450000ac82f06d1283df78479afd58` |
| light/appearance | 1120×820 | 94578 | `54f58ba7e769aa06803f5f90601a9fecb52fb5c7e838c4a5ac8ef4ff75667c94` |
| high-contrast/empty | 1120×820 | 29142 | `4a702657cd5fab88a577a96e4bdad04d782a49521eb93b645a1b056a495978b7` |
| high-contrast/chat | 1120×820 | 68216 | `150e51f4dfb1b1ef8efda8f3210e45fbe6ce35bf294e130cc4af9a0e040abcb9` |
| high-contrast/narrow | 520×760 | 60073 | `b411f2b7ec333fe67d19158adc8936ec46172e645582009ea8f7225e0edfa994` |
| high-contrast/draft | 1120×820 | 75542 | `5769894e956e83a88e0779a6691b272d3c14c2dffe1aa10964413dd81bc60269` |
| high-contrast/models | 1120×820 | 67452 | `18d0171cb9a091d46693f98b4a60a7d67f4bccbd4f037651501f86aa9a6a2cca` |
| high-contrast/runtime | 1120×820 | 71499 | `add4cb8976b93b9e186a301a0f1501549f55231abca40c8140f01588239d6913` |
| high-contrast/behavior | 1120×820 | 80877 | `d9d1543903f1226d74a95ab181c2bbd9ceb8195b6f380e64a84e7f76e9d8bdd8` |
| high-contrast/depth | 1120×820 | 77292 | `ee56e2dcd5feded88539066cfefe74415164dcf3eaea3fe29efc57f6a270de79` |
| high-contrast/appearance | 1120×820 | 79607 | `203e902220153fa5fff2e975b96a848e8d5eaf989bf40de9eed4a4686fb5e686` |

## Native original, same-frame metadata and capture limitations

Inspected committed final native original separately from exports. Independently
compared complete native JSON across before/after/final: **all fields equal in
these stored records**, including png_bytes=33371 (the architect comparison
permits excluding this timing-sensitive field, but these files do not differ).
All three native original PNGs also happen to have the same SHA-256:
`de4e8fa13452191c8757251da42997d9e81cf71e340140b983f5dee3dad07d0c`.
This equality is not a requirement for future native reproduction and does not
prove independent execution; caret timing can change native bytes.

All are frame revision `3`, 1120×820 at scale 1, logical-point bounds, Parley,
Metal/Apple M4 scene path, Bgra8UnormSrgb, 4 scene samples. Source is exactly
`live-surface-copy-before-present`; `presented_confirmation=false`. Semantic
nodes/entity IDs, theme, bounds, enabled/focused states and empty values are
unchanged; Prompt id 27 focused at [246,631,628,40], empty value. Native metadata
and PNG are one protocol frame, not independently composed metadata and screenshot.
The native empty view retains the explicit no-inference and not-ported disclosures.

Read `scripts/capture_native.py`: explicit `--observe-offline`, no desktop capture,
scoped descriptor observed, before-present assertions, process normal wait,
return-code-zero and descriptor-deletion assertions; finally terminates a lingering
child. Entry point sets offline before worker/credential path and installs the
15-second exit event. All three saved native logs say normal timed exit and
observation cleanup passed, run.json records exit zero. I did **not** recreate a
descriptor/process or directly observe a fresh cleanup. This is source/log
inspection only. A surface copy is **not compositor evidence**, and neither
metadata semantics nor a focused text node is physical accessibility certification.

## Behavioral split and interaction assertions

`main::app` retains UiPlugin then MarkdownPlugin, resources, guard at **front of
Input**, shell Startup and controller Update. No new schedule, deferred callback,
parallel state store, event bus, protocol or runtime abstraction. Setup calls
`composer::setup -> Handles`, preserving spawn/parent order and final focus;
Handles contains the same seven entities, not a second draft. Controller handles
replies then canonical intents then safe Submitted snapshots, synchronizes shell,
and runs observation predicate last. Generation fences and recovery/admission
semantics are retained. `shell::sync` moves and restores canonical vectors only
synchronously; no async/event boundary was introduced during that borrow escape.

Source/assertion inspection (not fresh test execution):

| Contract | Exact source / assertion inspected | What it does and does not establish |
|---|---|---|
| Enter / Shift+Enter | composer::guard_submit_boundary, reset_prompt; ui_tests::shift_enter_multiline_survives_clear_and_plain_enter_sends | Shift+Enter newline/no request; plain Enter sends once even with IME merely enabled; synchronous reset clears scroll and removes FieldTransactions. |
| same-batch ambiguity | ui_tests::ambiguous_queued_submit_is_not_sent_and_preserves_newer_edits; qualification_tests::framework_prompt_submit_is_still_end_of_batch_not_enter_snapshot | Framework [Enter,!] yields hi!, app suppresses Send and preserves hi! with explanatory status. No fabricated Enter-time snapshot. |
| active composition | composer::guard_submit_boundary | Denies nonempty ime_preedit or ime_commit. Dedicated active-composition assertion is missing; see low gap below. |
| menu identity/search/focus | interaction_tests::model_menu_selects_exact_index_and_search_preserves_draft, escape_closes_popup_and_restores_focus_without_sending; menus::{open,model_rows,close} | Sorted/filter rows carry original catalog pick index; Unicode draft retained, focus returns to model trigger, popup hidden, no command. Search test assigns Label rather than physically typing. |
| pointer dismissal | interaction_tests::physical_pointer_opens_menu_selects_model_and_outside_click_dismisses | Test name notwithstanding, it writes framework Input::pointer; not a physical device test. |
| restyle identity | ui_tests::semantic_theme_and_surface_updates_preserve_editor_popup_and_messages, semantic_appearance_actions_use_one_fifo_and_never_send | Retained editor/draft, focus, popup, Markdown entities and layout count; FIFO multiplicity and stale-control generation; no requests on restyle. |
| exact-source copy | conversation::copy_message; interaction_tests copy/fenced-code tests; qualification_tests::parley_markdown_hugs_remeasures_and_retains_streaming_view | Mounted assistant-only current Markdown source into Clipboard; rejects user/unmounted/invalid index. Native OS clipboard is not independently exercised. |
| scrolling/streaming | conversation::{sync_iter,follows}; conversation tests; interaction_tests::transcript_scrollbar_drag_preserves_draft_and_virtualization | One outer scrollport, Hug views, user anchor not dragged to tail, same mounted streaming view, bounded mounted history, scrollbar End/hide/focus repair and preserved draft/no requests. Huge individual messages remain an existing limit. |
| recovery | ui_tests::failed_admission_reload_preserves_visible_recovery_and_newer_draft, confirmed_admission_retires_recovery_without_touching_newer_draft | Unconfirmed prompt remains visible after reload, restoration explicit/empty-editor-only, newer draft untouched, reload/restore never auto-send. Not a populated recovery screenshot or per-session-draft implementation. |
| unavailable controls | menus runtime/behavior branches; shell::sync; interaction runtime test; ui_tests::permissions_are_not_actionable_ui_approvals | External agents and attachments/unsupported controls disabled or marked not ported; permission prose redirects to existing app, no approval control/grant. Secret/budget notices create no new authority. |
| observation | fixtures::sync_observation and observation_tests; ui_tests::offline_observer_denies_any_user_draft | Opt-in marker plus no commands/history/live/recovery/draft. Five new negative tests include repeated denial and absent opt-in; no descriptor or daemon enabled. Existing scope only, not arbitrary future private surfaces. |

## Severity-ranked findings, reproductions and minimal corrections

**Blocking / high / medium: none found for this preparation.** No changed expected
export, source-binding mismatch, split-induced focus/menu/input/scroll regression
or dishonest native/compositor claim identified.

### Low — existing active IME synthetic coverage gap (nonblocking preparation)

Files/symbols: `src/composer.rs::guard_submit_boundary` (lines 38–39),
`src/ui_tests.rs::shift_enter_multiline_survives_clear_and_plain_enter_sends`.
Read-only reproduction: inspect all src Rust references to `ime_preedit`,
`ime_commit`, `ime_enabled`; only the guard checks the first two; the sole test
assignment enables IME without active composition. Current code guards these
branches but no dedicated nonempty preedit/commit assertion exists. This gap
was present in baseline; no new input behavior changed here.
Minimal later correction: C's new architect-registered composer_tests should set
nonempty preedit and commit separately with Enter and assert no command, retained
draft/composition outcome and no leaked FieldTransactions; then separately perform
physical keyboard/IME gate under appropriate authorization. Do not describe the
existing enabled-but-idle test as active IME certification. Not fixed this wave.

### Low — existing minimum-width / popup visual coverage gap (nonblocking preparation)

Files/symbols: `src/fixtures.rs::export` captures narrow transcript only at 520;
`src/main.rs::app` native min 480; `src/ui_tests.rs::popup_content_follows_anchor_width_without_horizontal_overflow`
checks 520/1120 and anchors all kinds on ui.behavior, not each actual trigger;
`semantic_reading_measure_resizes_without_losing_draft` includes 480 but only
measure/draft assertions. No committed 480 export or narrow model/runtime popup
original establishes these visual/device cases. This is an existing evidence
limit, not a newly observed overflow regression.
Minimal later correction: after contract/fixture freeze, D with B/C adds explicitly
approved 480pt/narrow-popup fixtures and assertions using each real trigger,
including model search, popup scroll, focus and no-send. Architect registers tests;
review intentional additions rather than silently accepting changed exports.
Not required to widen this extraction's appearance and not executed this wave.

Informational existing gates: framework end-of-batch Submitted/reset marker
limitations, very-large-message behavior, absolute local rndr paths, clipboard/
physical keyboard/IME/accessibility, compositor presentation, live daemon/auth/
provider operation/paid inference and full Svelte parity all remain open. The new
five observe tests close the baseline's specific negative-branch coverage finding,
not a general privacy-surface certification or an authority expansion.

## Practical B / C / D boundaries and session/draft checkpoint

PASS for useful file ownership, not sandboxed plugins. B exclusively owns shell.rs
(layout/projection/Ui), C owns composer.rs/conversation.rs/menus.rs (Prompt,
transcript/copy/popups), D owns presentation.rs/fixtures.rs/observation_tests.rs/
capture harness plus packet-local evidence. These are actual distinct files and
stable concrete entity handles. Controller/Model/SDK remain A-owned; controller
alone performs normal state transitions/admissions. D must not modify runtime
state or certify its own changed exports by overwriting expectations. Fixtures'
synthetic exceptions do not authorize runtime changes/capture of private views.

Shell row/muted are shared helpers called by composer/menus, while shell invokes
composer mount and menus update. This compile-time dependency is acceptable here,
not asynchronous recursion; signature changes require architect serialization.
Visibility pub(super) is application-private, not an enforcement of single-writer
ownership. Review still must enforce the contract. Synchronous shell vector
move/restore is an explicit projection exception, not license for B to add state
transitions or async work.

Architect alone owns main.rs, manifests/lockfile, shared interface/shape changes,
existing shared test modules and new module/test registrations. Separate new lane
test files and packet evidence namespaces avoid shared-file races. Freeze Model,
Ui, Handles, actions and Command/Update in a parallel wave; serialize any interface
changes first and refresh lane bases. One bounded build slot remains appropriate.

P1 must define stable-session identity, memory-only per-session draft/recovery
separation, uncertain admission and stale-generation behavior before B implements
connected navigation. The current canonical draft lives in Prompt/Label, not
Model; there is no per-session draft map or session-list projection now. Existing
Recent reads SDK sessions().list(1); Open takes stable ID. Reuse that SDK, not a
new endpoint/protocol. A+C contract review and architect shape/test registration
checkpoint precedes P2; P3 uses the frozen draft interface, P4 new fixtures wait
for P2/P3 fixture freeze. Draft restore is not Send or admission. This ordering
prevents B from guessing session/draft state and C/D from becoming transition
writers. Four ownership lanes need not mean four full-time simultaneous workers.

## Result boundary

PASS supports sender assessing this read-only prep review together with the
separate correctness reproduction review. It does not itself open fanout or
native integration. No fresh test/export/native results from this reviewer.
Keep physical/device, compositor, live/provider and full-parity gates open.
