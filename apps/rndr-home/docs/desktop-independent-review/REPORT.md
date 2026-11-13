# Independent non-chat desktop review

## Identity and disposition

- Baseline: `a42c5236cd2239864d27aa9c151c3acd3e8191a0`.
- Implementation: `2cf6b171fdcaf4c5c74fdaccb3ed0299dbc7db82`.
- Exact reviewed source/evidence head: `97e0c223bf1d1016b2e8b440e7e924fd9716ce7c`.
- Independent reviewer, not implementation author. No implementation edits,
  approval, integration, push, deployment, daemon/provider calls, credential/config
  changes, effectful administration or framework repairs performed.
- **Qualified PASS for a bounded read-only/local-appearance non-chat preview;
  FAIL for complete desktop parity and unrestricted UX signoff.** Two reproduced
  P2 preservation defects below should be fixed in a bounded follow-up. They are
  not security blockers for an explicitly limited preview. No new P0/P1 issue
  identified in inspected scope. Native/IME and live-environment qualification
  remain human checks, not a blanket block on unrelated preview navigation.
- Sender retains responsibility and decides fixes/native integration. A later
  report-only commit does not change which implementation this review covers.

## Severity-ranked findings

### P2 — Responsive rebuild drops keyboard focus from Preferences

`src/desktop.rs:305-307` marks the page dirty when crossing 800px;
`:360-373` migrates focus only for record/back/refresh buttons; `:402-403`
deletes both dynamic navigation and body. Appearance buttons and other dynamic
settings/navigation buttons are omitted. Focus on Light, resize 1120 to 480,
settle updates: `Focus.entity` becomes `None` rather than a surviving control.
The existing geometry test (`src/desktop_layout_tests.rs:74-78`) only tests the
focus selected by navigation, not a body control focused before resize.

Independent harness test
`focus_on_preferences_control_survives_responsive_rebuild` fails with
`focus lost/deleted on responsive rebuild: None`. Reproduction module and raw
failure log are adjacent to this report. Restore by logical control identity or
move any descendant focus to a stable, visible, appropriate fallback before
rebuild (Preferences search is hidden). Cover settings-nav/pager and appearance
controls as well as record controls. This is an app-local focus fix, not an IME
boundary change. No secret or inference action is enabled by it.

### P2 — An off-route read completion resets the current dataset scroll

`src/admin_model.rs:201-217` sets global `dirty` on every accepted resource
reply, regardless of visible route. `src/desktop.rs:357-405` then rebuilds the
current view and explicitly zeros its scroll at `:404`. While Runtimes is
scrolled to -200, a valid in-flight Health reply moves Runtimes to 0, even
though its rows/query/selection did not change. This occurs naturally when
moving between resources while older reads finish, not only with stale or
forged replies. Generation fencing correctly accepts this reply for its own
resource; it should not invalidate unrelated visible content.

Independent harness test `unrelated_reply_preserves_current_dataset_scroll`
fails: actual `[0.0, 0.0]`, expected `[0.0, -200.0]`. Keep off-route cache updates
without invalidating the active route, and/or reconcile keyed content and
preserve offset on nonstructural updates. Add the regression without changing
chat/input gates. This does NOT disprove the separate retained-chat-scroll test.

### Coverage limitation — visual/accessibility signoff is incomplete

Three direct image-view requests returned metadata but no inspectable pixel
attachment to this reviewer. Do not treat those receipts as visual verification.
Instead six actual PNGs were independently processed with macOS Vision OCR:
`dark-narrow-runtime`, `light-wide-accounts`,
`high-contrast-narrow-runtime-detail`, `light-narrow-basement`,
`dark-wide-preferences`, `high-contrast-wide-mcp-disconnected`.
`pixel-ocr.txt` records text and normalized bounding boxes from real pixels,
including FIXTURE and SYNTHETIC/NO NETWORK/NOT NATIVE WINDOW / INPUT
prefixes in all six. Full INPUT PROOF was recovered in the wide images; OCR did
not recover the final PROOF word in the three narrow samples. That is an explicit
possible footer-clipping check, not a conclusively diagnosed glyph defect.
Dark/light/high contrast and wide/narrow are represented. This is machine pixel
inspection, NOT a full visual aesthetic, contrast, glyph-overlap or clipping
signoff, and it is not a human/physical interaction test.

Risk areas requiring actual visual/manual inspection: narrow rail uses 8px
text (`src/desktop.rs:321`); min window height is 650 but PNGs are 760/820;
scrollable settings navigation places MCP/Connection below the initial wide
fold; narrow detail explanation reaches the scroll viewport boundary. Geometry
assertions bound major rectangles, not every glyph/control, and cannot establish
absence of text clipping. OCR alone cannot distinguish intentional viewport
clipping from unreachable content. These are risks/checks, not invented defects.

## Actual coverage versus reference UI

Read actual baseline-to-head diff, new models/worker/views/tests/fixtures, both
implementation/parity documents, seal/results/reproduction, and reference
`SettingsPanel.svelte`, `SettingsNav.svelte`, `types/settings.ts`, `NavShell.svelte`,
`navSurfaces.ts`, `lifeRailItems.ts`, plus runtime worker wire/SDK route sources.

All nine settings routes exist in reference order and group: Preferences,
Accounts, Agent, Runtime, Network/Sharing, Connections/External Agents, Packages,
MCP, Basement/Connection. Preferences has three reversible window-only appearance
controls; Packages explains the deferred package/install flows. Other sections
mix useful read snapshots, safe links and explicit deferred-operation copy.
They are NOT nine fully implemented settings panels or editable equivalents.
Narrow previous/next navigation is an adaptation, not the production chooser.

The shell is a source-derived static text rail with a single retained chat panel
and settings/admin page. It is not production's icon/mode-switching rail,
workspace/session/sidebar explorer, draggable tabs/panes, unread indicators or
shell toolbars. Of 18 rail destinations, Chat, Settings and Schedules map to
implemented views; the other 15 have honest deferred landings. This is tangible
progress beyond chat, especially administrative visibility, not full desktop
or life-surface parity. `DESKTOP_PARITY.md` acknowledges these limits and gives
concrete reference owners; it must not be summarized as completed parity.

### Ten read-only datasets and compatibility

1. Health: SDK health compatibility/read; projected status/version/counts.
2. Model catalogue: allowlisted cached HTTP catalogue; model/provider IDs, not provider URLs.
3. External runtimes: SDK runtime rows; auth states stay unknown unless reported.
4. Agent modes: SDK mode ID/label/availability, not system prompts.
5. MCP: SDK status; enabled/connected/tools; no tool invocation or auth URLs.
6. Local engine: typed status; no load/start/download actions.
7. Schedules: SDK recurring list; bounded snapshot, no prompt/delivery targets.
8. Shared workshop mode: typed status; mode/time/profile IDs, no passwords or join action.
9. Worker capacity: allowlisted `/v1/runtime/workers` GET and snake_case
   `max_in_flight` wire shape agree with reference `RuntimeWorkerConfigWire`
   (`apps/medousa-home/src-tauri/src/daemon/runtime.rs:41-47,263-269`), not the
   frontend camelCase DTO.
10. Hosted review providers: SDK `/v1/reviews/providers`: provider ID/label/protocol and problem count.

The rerun ten-resource HTTP fixture test asserts exact expected GET paths and
representative response projections. Typed SDK methods compile against pinned
contracts. This is strong fixture/source compatibility, NOT a successful live
authenticated daemon test, deployment/version matrix or provider qualification.
The dynamic JSON catalogue/capacity projections should acquire additional
versioned daemon-schema fixtures before stronger compatibility claims. Sharing
uses the existing typed status response; review discovery uses the typed SDK.

### Safety, authorization and state

- Existing explicit local origin/data-dir credential boundary is reused;
  offline/export paths create neither worker and discover no credentials.
  No new authority bypass or arbitrary URL/mutation request type exists.
- Read worker exposes a closed Resource enum and GET-only dispatch. Typed safe
  projections exclude config blobs, secrets, provider URLs, prompts, MCP headers,
  local paths and delivery targets; raw transport/serde errors are not rendered.
  Redaction and HTTP contract tests passed. Display labels remain untrusted
  display text: this is field allowlisting, not arbitrary-string DLP.
- Permission/incompatibility/unavailable, offline/not-read, loading, empty,
  filter-empty, list and detail states are distinct. Serial equality prevents
  stale replacement; in-flight duplicate reads are suppressed per resource.
  Reads are serialized with a 12-second service timeout, so queued resources can
  wait longer than 12 seconds. No cancellation/pagination/live subscription;
  list truncation is after receipt, not a network byte-size cap. MCP can actually
  show 501 rows (gateway plus 500 servers), despite the displayed 500 maximum;
  this minor cap/copy inconsistency is not a preview blocker. Fine for bounded
  preview; do not advertise large-scale or continuously live administration.
- Harmless navigation/filter/local appearance remain available without native
  input qualification. Admin filter submissions are not chat sends.
- Composer `native_input_ready` is production false; Send/Restore/New/Recent/
  Reload are still controller-gated, including forged intents. Model/session,
  retained draft and chat transcript/scroll synthetic tests pass. Chat root is
  hidden, not replaced; broader physical IME/selection/scroll behavior remains
  unqualified. No rndr input boundary was changed or repaired in this review.
- No production Home, framework crates/pins, SDK, daemon, root manifest/lock
  changes in baseline-to-reviewed-head diff. All 366 preexisting app doc/evidence
  paths are byte-identical by Git diff; new desktop evidence is additive.
  Implementation-to-evidence-head delta is documentation/evidence only.

## Independent verification (not author-log claims)

Ran the supplied `desktop-evidence/reproduce.sh` with isolated
`OUT=.desktop-review TARGET=.desktop-review-target`, two Cargo jobs, incremental
and debug info disabled. Nine recorded steps all exit 0:
fmt; default test; observe test; scoped strict clippy default/observe; build
default/observe; offline GPU export; strict docs verification.

- Default: 6 library + 62 binary tests passed; doc tests 0.
- Observe: 6 library + 70 binary tests passed; doc tests 0.
- Scoped clippy is `--all-targets -- -D warnings` on the app manifest, not workspace-wide cleanup.
  Existing dependency/root manifest warnings remain, not repaired. Raw captured
  log blank endings are preserved, including the review logs; these are evidence
  whitespace, not implementation whitespace.
- All 108 re-exported PNGs are byte-identical to committed evidence.
- Verified every sealed source/evidence/render SHA-256, all PNG dimensions,
  implementation tree OID, labels in the six OCR samples, and unchanged prior
  evidence. `integrity.json` records audit totals. Hashes authenticate artifacts,
  not physical input or visual correctness.
- Implementation whitespace check passed; author evidence raw Cargo log EOF
  blanks remain the declared full-evidence whitespace caveat.
- Two independent additional regression tests FAIL as described above (exit101).
  Harness copies source under scratch, appends review tests there and edits no
  tracked implementation. Reproduce with `python3` on the adjacent script from
  repository root. It refuses an existing scratch directory and uses offline
  Cargo, two jobs and compact profiles; budget approximately 2GB target space.
- Captured logs are independent reruns, not copies of author logs. OCR has a
  separate source script. No native-window observation/physical input/live
  credentialed service test was run. Build cache is removed after evidence capture.

## Concrete next safe implementation slices

1. Fix the two preservation defects with app-local keyed reconciliation/focus
   fallback and these exact failing regressions. Keep native-input gates closed.
2. Match production settings chooser/rail grouping and add per-control/glyph
   bounds, 650px height coverage, semantic selected-state and keyboard traversal
   tests. Validate with actual wide/narrow pixels; do not require rndr repairs.
3. Add bounded read-only Activity/jobs or Files/Notes explorer using production
   API contracts and safe summaries, no execution/reveal/write actions. Select
   one deferred matrix row with explicit empty/error/detail and long-list tests.
4. Add versioned wire fixtures for raw catalogue/worker-capacity endpoints, response
   size/pagination considerations and visible pending/cached/fresh distinctions.
   No live provider/account changes are needed for these tests.

## Compact final human checks (separate qualification, not blanket preview hold)

- Actual window at 480x650 and 1120x820 in light/dark/high contrast: glyphs,
  focus ring/contrast, settings rail scroll, previous/next reachability, all nine
  sections, list/detail Back, long text, nested scroll, and below-fold controls.
- Keyboard/Tab/Shift-Tab/Enter/Space through resize/filter/refresh/navigation,
  including both reproduced defects after fixes; accessibility names/selection.
- Retain chat draft, selection, transcript offset and newest-follow policy while
  leaving/returning, resizing and receiving synthetic stream updates.
- Physical keyboard/IME composition/cancel/commit/empty across frames: Send,
  Restore, New, Recent and Reload must remain denied until separately proven.
- Only if separately authorized: explicit local test daemon/data-dir, denied auth,
  unavailable/incompatible/malformed and reconnect checks; inspect UI for leaked
  paths/config/secrets. No paid turn, mutation, provider action or real credential
  changes are part of this checklist's current execution authority.
