# Preparation closure — docs-only correction and sender dispatch packets

## State and immutable identities

Both independent deliverables have been assessed and accepted by sender for
candidate **`bdef72eb7fbf8014067603f50bb15cfe0a3745f5`**, per the closure request.
Preparation itself awaits sender's assessment of this bounded closure. This is
not acceptance on sender's behalf, worker dispatch, feature implementation or
native integration. No workers have been launched by this closure.

The docs-only closure commit is the commit containing this document; resolve its
full OID from Git and use the exact OID returned with this handback for dispatch.
Its direct parent/base is `bdef72eb7fbf8014067603f50bb15cfe0a3745f5`.
Do not call closure HEAD the independently executed candidate. Source remains
bound to extraction `08839824203c57588607f3ad66496e1bd81d8cba` plus denial coverage
`b9ee1f1b55fd984231dcb8082fddd745444d5986`; prep baseline remains
`431ccab3affe24725eaff7d39176661b3398a17f`. Historical qualification manifests,
starting-HEAD/source-manifest distinctions and original reports are not rewritten.

### Immutable reports and bounded evidence seal

All paths below are relative to this directory.

| Lane | Original report | SHA-256 |
|---|---|---|
| Correctness/authority | `review-prep-correctness-20261113-1304/REPORT.md` | `d04c7ded00300286d981c3eefc383bf34d6f2de498b345acace63d68c293e4b6` |
| Desktop UX/evidence | `review-prep-ux-20261113-0bdc05e3/REPORT.md` | `4ef4fffc33e9ad19d766a47d90010efefeebc41c6a0ef20a92455026a775bdd3` |

The entire explicitly inventoried correctness review set is sealed: original
report, original SHA256SUMS, source hashes, run/final-state/integrity/comparison
records, bounded logs, three review-only Python reproducibility/audit scripts,
27 synthetic export originals and one native empty PNG/JSON. These are review
receipts under docs, not new application implementation. Their original bytes
(including log/manifests' terminal blank lines) are retained. The original report
says untracked because that was true at review time; sealing does not edit history.
UX produced only its report; it references existing tracked architect originals,
not new UX-generated outputs. No target/cache, thumbnails, contact sheets, new
exports or whole generated tree are added. `closure-evidence/REVIEW_SHA256SUMS`
binds both sets including the original correctness manifest;
`closure-evidence/CHANGED_FILES.txt` enumerates the exact closure paths.

Correctness freshly executed **54 default / 60 observe** tests, both strict clippy
and build variants, fmt/strict docs, 27 byte-identical exports and native empty
capture/cleanup on the candidate, using existing cached artifacts, not a clean
rebuild. Its strengthened source audit passed 23 checks and 150 architect hashes.
UX separately performed read-only source/hash/dimension inspection and opened
**all 27 original final exports plus native original**; it verified 150 architect
hashes and performed **no Cargo/app/export/native execution**. The two methods
remain distinct. Neither lane grants preparation acceptance or feature authority.

This closure runs only bounded documentation/hash/Git diff checks. No Rust tests,
Cargo/build, app, export or native capture are rerun. Candidate test results are
not relabelled as execution on closure HEAD. `closure-evidence/verification.json`
records the allowlisted path/no-code-change checks and evidence integrity.

## Resolved wording, not changed behavior

Automatic retirement of pending recovery requires **matching-generation
Admitted**. Explicit **Restore** into an **empty composer** transfers the last
recovery entry into that composer, removes the entry and clears its pending
pointer **without enqueueing Send**. Restore is not admission, does not duplicate
the entry, and does not clear uncertain execution merely by copying text.
Failure/history reload never implies admission or automatic resend; newer draft
text cannot be overwritten. This clarifies intentional existing controller
behavior only. TEAM_PREPARATION and both review briefs now use this distinction;
independent report contents remain immutable, including the original finding.

## Final P1 / A packet — contract first, for sender dispatch after closure

**Admission:** not launched. Sender must first assess this closure, then issue an
assignment with the exact closure OID (or a newer explicitly reconciled base).
Source/evidence review reference remains bdef72eb above. Do not use a moving branch
name as the assignment base. If other changes intervene, architect must refresh
the packet and review its delta before dispatch.

**Goal:** define stable-session selection and memory-only per-session draft and
recovery semantics before connected shell work. There is currently no session-list
projection or per-session draft map. Prompt/Label is the active editor's canonical
draft; do not introduce a competing live editor store. Reuse the existing SDK
`client.sessions().list(...)` read API (`Recent` currently uses list(1)) and
`Open(stable session_id)`; no daemon endpoint, shared SDK/protocol or framework edit.

**P1a first deliverable (contract-only checkpoint):** A proposes a bounded contract
in `docs/p1-a-contract/CONTRACT.md`, with an exact action/Command/Update table,
proposed Model fields and draft snapshot/reset API, transition table, ownership,
list bound/pagination/error behavior and synthetic test plan. No runtime source
or module registration changes before architect + C review and sender checkpoint.
A must settle explicitly:

1. Stable session ID keys, a separate unsent/new-session key, and how that key
   migrates to an admitted stable ID without prose matching or draft merging.
   Identical prompt text in two sessions is independent data.
2. One live Prompt draft plus inactive memory-only snapshots; exact snapshot,
   switch, restore and text-revision rules. No persistence, private-data logging,
   automatic submit or draft loss on list refresh, failed Open or stale reply.
3. Which navigation is refused while loading/busy; uncertain admission blocks
   Send until explicit history reconciliation. Define selection commit/rollback,
   generation increments and stale reply handling, preserving current catalog/
   unavailable exceptions rather than broadening them incidentally.
4. Session-scoped recovery/pending associations and reconciliation. Automatic
   retirement requires matching-generation Admitted; explicit Restore transfers
   the entry into an empty editor, removes it/its pending pointer without Send.
   Reload alone preserves recovery. Restoration does not certify admission.
5. Bounded session metadata projection from the existing SDK, stable-ID selection,
   offline/disconnected/error/empty-list behavior, and no hidden reconnect/send
   replay. Enumerate action identity and cross-lane handle/API changes for the
   architect to serialize. No credentials/private session reads during tests.

**P1b later implementation boundary:** only after that contract is frozen and
sender dispatch permits implementation. A's exclusive paths under `src/`:
`state.rs`, `controller.rs`, `lib.rs`, `backend.rs`, `backend_tests.rs`, and new
`session_tests.rs` after architect registration. Packet evidence goes only in
`docs/p1-a-evidence/`. A requests shared shape edits; architect serializes them
with file owners. No edits to shell/composer/menu/view/theme files, main wiring,
Cargo files or existing shared UI/interaction/qualification tests by A. No source
implementation is performed or authorized to start by this closure document.

**P1 acceptance for the later implementation:** synthetic switch A→B→A preserves
distinct Unicode/multiline drafts and same-text prompts independently; failed
Open/list error preserves the previous selection/draft; stale generation cannot
replace selected identity or retire another pending recovery; busy/loading
navigation is blocked; uncertain admission never resends; reload retains pending
recovery; explicit restore requires empty composer, moves exactly one entry,
clears its pending association, leaves newer text untouched and emits no Send;
matching Admitted automatically retires only its own pending entry without
clearing newer edits; disconnected actions enqueue no command. Assert editor
identity, scroll reset and no leaked FieldTransactions where reset is involved.
Return contract, exact commit/base/diff, registered-test evidence and remaining
gaps. New tests written but not registered/executed must never be called passing.

## Follow-on ownership and dependency packets

Paths in the table are under `apps/rndr-home/`. These are ready packet boundaries,
not launched assignments. Sender selects exact refreshed OIDs after each gate.

| Packet | Exclusive edits | Dependency and acceptance |
|---|---|---|
| P2 / B shell/navigation | `src/shell.rs`, new `src/shell_tests.rs`, `docs/p2-b-evidence/` | Wait for P1 contract and implemented interface checkpoint; render stable-ID selection and recovery status, no SDK/admission writer. Synthetic empty/start/reload/selection, busy/loading/uncertain gates, no draft overwrite, focus and 480/520/1120pt widths. Coordinate real-trigger popup assertions with C/D, not shared-file edits. |
| P3 / C input/transcript | `src/composer.rs`, `src/conversation.rs`, `src/menus.rs`, new `src/composer_tests.rs`, `docs/p3-c-evidence/` | Uses frozen P1 draft API; may run alongside P2 only after interface freeze. Plain/Shift Enter, later-key ambiguity, model search/Escape, source copy, scroll anchor, six-line cap, same streaming view and no Send on menu/theme/scroll. Close the low active-composition gap with **separate nonempty ime_preedit and ime_commit + Enter cases**, asserting no command, preserved draft/composition outcome and no FieldTransactions leak. Enabled-but-idle IME is not this test and not physical certification. |
| P4 / D theme/evidence | `src/presentation.rs`, `src/fixtures.rs`, `src/observation_tests.rs`, `scripts/capture_native.py`, `docs/p4-d-evidence/` | May plan harness first; new fixtures wait for P2/P3 fixture freeze. With B/C add explicitly reviewed **480pt and narrow popup originals/assertions using each actual model/runtime/behavior/depth/appearance trigger**, including model search, popup scrolling, containment, focus return and no Send. Dark/light/high-contrast, editor identity, marker lifetime, all denial branches and offline-empty cleanup. Do not silently overwrite 27 prior expectations or widen observation authority. |
| Architect shared wiring | `src/main.rs`, `Cargo.toml`, `Cargo.lock`, existing `src/{ui_tests,interaction_tests,qualification_tests}.rs`, shared interfaces/test registration/team docs/indexes | Register lane test modules serially before counting tests. Own atomic shared-shape changes with file owners, resolve failures, freeze/rebase before reopening lanes. Separate independent correctness and UX review after integration evidence; sender alone accepts/dispatches/integrates. |

A remains the only normal runtime transition writer. B projects state; its
synchronous history/live take/restore exception cannot become async state
ownership. C manipulates editor through the approved API, never sends SDK
commands. D's fixture exceptions are synthetic, not runtime authority. Model/Ui/
Handles, row/muted helpers, actions and Command/Update cannot change concurrently.
No two lanes own one file; one new registered test module and evidence namespace
per packet. Four ownership lanes do not require four simultaneous workers.

### Shared build-slot and handback policy for future packets

One active build/test/export/native runner at a time in the existing shared target:
`/Users/theelevators/Library/Application Support/medousa/forge/worktrees/b906e0f57650/.cache/cargo-target`.
Architect/coordinator schedules the slot; workers without it perform reads/docs,
not another build or duplicate target. Max **two jobs**; `CARGO_BUILD_JOBS=2`,
`CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`; offline/locked app-only commands and explicit target.
Sample free disk before, during and after commands and stop the process group
below **8 GiB free**. No cleanup/clean rebuild/cache duplication is implied.
Return actual commands, source hashes, cache class, disk minima, original logs
and exact commit/base. Architect integrates serially and runs default/observe,
clippy/build/fmt plus agreed bounded visuals; two independent reviewers inspect
correctness/authority and UX/original evidence separately before sender acceptance.

## Explicit remaining limits and prohibitions

P3 active IME and P2/P4 480pt/real-trigger popup acceptance above are carried-forward
low pre-existing gaps, **not closed here**. Physical keyboard/IME/clipboard/
accessibility, compositor presentation, live daemon/auth/provider/paid inference,
very-large-message behavior and full desktop parity remain open. Native evidence
is `live-surface-copy-before-present`, `presented_confirmation=false`, never
compositor certification. No private chat, credentials, live daemon, paid inference,
network mutation, SDK/protocol/framework changes, push or deploy. Production
`apps/medousa-home` remains untouched. External rndr remains read-only pinned
`44f5dc7c0c91af56e4f3e29f97283b8e8d1e1139`; this closure does not access or mutate
that checkout and does not claim a fresh external cleanliness attestation.
Sender retains preparation acceptance, dispatch, independent review and native
integration; none is performed by this closure.
