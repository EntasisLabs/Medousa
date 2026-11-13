# Fresh independent review — Settings/navigation and read-only Activity

## Decision and exact boundary

Recommend **bounded desktop-preview integration**, with the nonblocking documentation
qualification below. No high/medium correctness, privacy, input-admission or scope
regression was reproduced. This is a review recommendation, not implementation
acceptance, project closure, native integration, production qualification or deployment.
The sender retains those responsibilities.

Reviewed HEAD: `aeef3db1dfe768a7c19f327f63c19cbd8a58d319`.
Baseline: `9d51646838cba3571eaec994d5919ef7b2bf6be2`.
Source/test/render revision: `76f624954de8efca5613bc9c0b998e1dbd315a75`.
Both revisions are ancestors of reviewed HEAD; HEAD's immediate parent is the source
revision. The last commit is exclusively `docs/shell-activity-evidence` changes.
The entire non-evidence diff from source revision to HEAD is empty.
App source tree: `5eb19569ce9e6197554c3ef3835a9ff970bf4a37`.
Five undertaking commits and original terminal history are preserved. No commit,
reset, integration, push, deployment, dependency or production Home change by reviewer.
Tracked working tree is clean. `.review-independent/` is deliberately untracked
local review evidence, not part of the original 369-path candidate inventory.

## Severity-ranked findings / qualifications

### Low: modifier-exclusion wording overstates actual chooser policy

`apps/rndr-home/docs/SHELL_ACTIVITY_PARITY.md:38` says modified batches are not
intercepted. `src/menus.rs:391–403,419–427` excludes Ctrl/Alt/Super but allows Shift
for all recognized navigation keys, not only Shift-Tab. Reproduction: open Settings
chooser on Runtime Controls and press Shift-Home: focus moves to Preferences and
the single-key batch is consumed. Shift-End/arrows likewise navigate. This is
harmless chooser navigation, not a native-input gate bypass or submission; the
source and independently passed ordinary/Shift-Tab traversal support integration.
Suggested documentation correction: name Ctrl/Alt/Super exclusion and explicitly
state Shift navigation is supported (or say “except supported Shift navigation”).
No implementation was silently changed.

### Informational: fixture boundedness is per operation, not a hardened global deadline

`src/admin_backend_tests.rs:27–55`: accept polling has a 15-second deadline;
accepted sockets explicitly become blocking and have a 15-second read timeout.
The request loop has no total-byte/elapsed-header budget, and response writes have
no write timeout. A custom peer dripping incomplete headers faster than the read
timeout could exceed 15 seconds; an arbitrarily large unread response could block
writing. This helper is an ephemeral loopback test fixture with its own 15-second
reqwest client and small finite responses, not an exposed server. All fixture tests
completed independently. The readiness-race correction is appropriately scoped;
production worker's existing 12-second read deadline is unchanged. Do not describe
this as an adversarial end-to-end 15-second server guarantee. Optional future test
hardening, not a preview integration blocker or a demand for production work.

### Preserved evidence whitespace caveat

Full `git diff --check BASE..HEAD` exits **2** solely for new EOF blank lines in
`activity-fold.log:81`, `focused.log:85`, `header-before.log:81`,
`test-default.log:170`, `test-observe.log:179`, all under author evidence.
Non-evidence `git diff --check` exits **0**. Logs were preserved, not normalized.

## Inspected source and production references

Every one of the 16 non-evidence changed paths was read, including complete diffs:

- `apps/rndr-home/README.md`
- `apps/rndr-home/docs/SHELL_ACTIVITY_PARITY.md`
- `apps/rndr-home/src/{admin_backend,admin_backend_tests,admin_model,admin_views,desktop,desktop_fixtures,desktop_layout_tests,main,menus,qualification_tests,settings_views,shell_activity_tests}.rs`
- `docs/README.md`, `docs/guides/rndr-native-preview.md`

Followed relevant unchanged callers/contracts: composer/controller, desktop
preservation and existing input/history tests, Record sanitizer and Desktop
begin/settle state machine; production Home settings types/SettingsNav, NavShell,
lifeRailSections, Activity API/data store; SDK workspace query/list call, daemon API
WorkCard/WorkBoardColumn/WorkspaceCardsQuery/Response and workspace card/service
projection. Production source was reference material only, never modified.

### Settings/navigation results

Nine sections and order match production definitions: Preferences, Accounts,
Medousa Agent, Runtime Controls, Sharing, External Agents, Packages, MCP, Connection.
Groups are Medousa then the explicitly preview-labeled This host group; production
machine-group label is blank, not literally This host. That is not exact chrome parity.

App tests and an additional independent probe pass current-section initial focus,
Tab/Shift-Tab cycle including close button, all nine down-arrow positions and wrap,
up-arrow wrap, Home/End, Enter activation, Escape route preservation and focus return.
Existing tests cross the responsive breakpoint with popup open and verify the
stable chooser anchor and focused entry survive. Stable action/region replacement
avoids dangling focus during page rebuilds. Space uses unchanged canonical button
activation; no additional physical-key/Space qualification claim.

Rail primary doors scroll independently of its fixed Activity/Settings/You dock;
minimum-height geometry and dock immobility tests pass. Current-destination and
current-setting names exist in Semantics and selected appearance is rendered.
These are app semantic names, not proof of native selected-state announcements or
screen-reader conformance. Static text shortcuts/automation subdoors remain an
explicit partial-reference preview layout, not a reproduction of production's
dynamic icon rail or Spotlight entry point.

### Activity results

SDK request is exactly GET `/v1/workspace/cards?limit=100&include_terminal=true`;
response requires workspace_revision and typed WorkCards, with both query and
local projection capped at 100. Production workspace service bounds its query and
projects existing work cards; app does not invent `/jobs` or claim a complete ledger.
Loopback fixtures independently exercise exact GET/query order, absent authorization
in fixture transport, malformed/permission/error/empty results and cap behavior.
No real daemon or credential read was performed by this review.

Only generic numbered title, controlled board-state enum, card ID, UTC timestamps,
revision and fixed privacy notice survive projection. Record text strips control
characters and caps fields at 240 characters; no markup/URL execution exists.
Prompt-derived titles, raw status prose, output, unknown JSON payloads, diagnostics,
worker transcript and associations do not reach the rendered Record or semantic
row title. Unknown schema/enum values error rather than become empty success.
This is privacy-safe allowlisted metadata, not a claim that arbitrary remote IDs
are independently trusted or that an ID cannot itself contain sensitive text.

Filtering searches projected fields locally and resets selection/viewport; it does
not send requests. Detail uses the same snapshot, with no secondary detail GET.
Back restores query, viewport and originating row focus; canonical focus reveal
can adjust the viewport to expose that row. Refresh deduplicates Loading requests,
clears positional selection before replacement and fences replies by resource and
serial. Existing stale/unrelated-background tests passed; independent probe also
replaced the dataset while a row was selected and confirmed no stale detail/Back
control survived. Offline/not-loaded/loading/permission/incompatible/unavailable/
disconnected/empty/no-match and fixture/live-read/retained-offline labels are distinct.
“Live read snapshot” expressly disclaims continuous liveness, and Done board state
is not execution-success proof. No effectful job/admin action channel was added.

### Chat, scope and parity

Composer/controller/conversation and their input gate are unchanged. Production
native_input_ready remains false; queued/forged `send/recover/new/recent/reload`
remain rejected, corresponding to Send/Restore/New/Recent/Reload. Existing suites
retain draft/session/transcript/scroll/virtualization/Jump to latest tests, plus the
new chooser/background-reply chat-preservation tests. No runtime bypass was added.

Parity classification is materially honest: functional read-only means GET-wired
but loopback-fixture tested, not live service proven; local preview is reversible
window state; deferred means no functional port. Of 18 shell doors, Chat, Settings,
Schedules and bounded Activity have working preview views; **14/18 deferred** is
correct. Broader Activity feed, editable settings, native chrome and host operations
remain deferred. This does not turn one IME-dependent feature into a blanket blocker.

## Independent commands and outcomes

All commands run from repository root, with independent logs beside this report:

| Command | Result |
|---|---|
| `cargo fmt --manifest-path apps/rndr-home/Cargo.toml --check` | 0 |
| `cargo test --manifest-path apps/rndr-home/Cargo.toml -j 2` | 0; 6 library + 79 app tests, 0 doc tests |
| `cargo test --manifest-path apps/rndr-home/Cargo.toml --features observe -j 2` | 0; 6 library + 87 app tests, 0 doc tests |
| `cargo build --manifest-path apps/rndr-home/Cargo.toml -j 2` | 0 |
| `cargo build --manifest-path apps/rndr-home/Cargo.toml --features observe -j 2` | 0 |
| `cargo clippy --manifest-path apps/rndr-home/Cargo.toml --all-targets -j 2 -- -D warnings` | 0 |
| `cargo clippy --manifest-path apps/rndr-home/Cargo.toml --all-targets --features observe -j 2 -- -D warnings` | 0 |
| `bash scripts/verify-docs.sh --strict` | 0; `verify-docs: OK` |
| `cargo run --manifest-path apps/rndr-home/Cargo.toml --features observe -j 2 -- --export-desktop apps/rndr-home/.review-independent/renders` | 0; 312 GPU PNGs |
| `cargo test --manifest-path apps/rndr-home/Cargo.toml --features observe -j 2 independent_review_full_chooser_cycle_arrows_and_refresh` | 0; 1 independent probe passed |

Probe source is `probes.rs`: temporarily append it to shell_activity_tests.rs to
reproduce, then restore the exact original bytes. Review restored and hash-verified
the original source after execution. No extra tests are silently part of candidate.
Manifest warnings from dependencies are not a strict-Clippy failure. No dependency
installation or upgrade was performed. An optional contact-sheet convenience
attempt could not run because Python Pillow was absent; direct image inspection
was used instead, with no package installation.

## Pixel review: enumerated samples, not 312 inspections

All 312 independently generated originals are byte-for-byte equal to author's
312 committed final PNGs. This establishes reproduction, not exhaustive visual
inspection. The following **17 unique samples** were opened with direct
`cognition_image_view` for pixel inspection (not inferred from receipt metadata).
Names below are relative to `renders/`; each exact full name is in inspected-pixels.json.

| Theme | 480×650 | 480×760 | 1120×650 | 1120×820 |
|---|---|---|---|---|
| Dark | settings-chooser; activity-error | activity-list | activity-list | settings-chooser; activity-empty |
| Light | settings-chooser; activity-list; activity-empty | activity-error | activity-list; activity-error | activity-detail |
| High contrast | activity-detail | activity-empty | settings-chooser | activity-empty |

This includes minimum-size popup/rail/dock, list/detail and error/empty priorities,
all three themes and each of the four dimension pairs. Minimum popup is scrollable;
not every section must be visible simultaneously. Detail's lower metadata requires
scrolling at 650 height; first board-state metadata is visible. Labels disclose
FIXTURE/SYNTHETIC and read-only/board-state limitations. No blocking overlap,
clipped dock or unreadable first-detail-state defect was found in these samples.
No claim that all 312 exports were visually reviewed, or that screenshots prove
pointer/hit testing, physical keyboard, IME or screen-reader behavior.

Original PNG-byte hashes are NOT image-view rendition hashes. E.g. original
`dark-narrow-650-settings-chooser.png` SHA-256 starts `fb887fdb...`, while the viewed
rendition hash is `c0797d5d...`; both are separately recorded. The rendition is a
normalized pixel artifact, not the original compressed file. One early read of
a high-contrast reproduced file raced export completion, failed for missing file,
and was retried successfully once available; no absent image was counted.

## Provenance / seal

Independently verified all 28 source hashes and all 352 evidence hashes in author's
seal, all 312 original-render hashes, source tree identity, binary SHA-256 and the
369-path changed-files inventory. There are 16 non-evidence + 353 evidence paths;
seal.json naturally cannot include its own hash in its evidence map. Review records
its own hash separately. The independent builds still yield the sealed app binary
SHA-256 `1946355a165343164adb8f69edd38a97d2429fa6f8bdddce3f81d2d42f6a2415`.
No claim of signed supply-chain attestation or historical timestamp proof: these
are verified local content/ancestry/reproduction facts. Original failed/intermediate
logs and pre-correction screenshots remain attributed as historical evidence,
not final-source success receipts. Author ledger counts 11 pixel samples, not 312.
See seal-verification.json, reproduction-comparison.json, inspected-pixels.json,
results.tsv, commands/logs and full-non-evidence.diff in this directory.

## Remaining final human checks

Physical device/window resizing/scaling, pointer/scroll, physical keyboard focus
and dismissal, native accessibility announcements/high contrast and IME remain
explicit human checks. Real-workshop identity/locality/order/timestamps, offline,
permission/error/empty responses and privacy require a separately authorized live
check; none occurred here. Keep all five input-sensitive actions denied until their
separate framework/app qualification. These limits are not blanket blockers on
bounded read-only preview integration or unrelated safe desktop work.
