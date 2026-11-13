# rndr desktop team preparation — behavior-preserving seam candidate

Status: **prep candidate, not desktop parity or integration approval**. No workers
launched. Original architect owns technical continuity; sender orchestrates,
accepts independent reviews and alone owns native integration. Native merge is
deferred until this preparation and its two review lanes pass. Later feature
packets below are proposals, not work executed by this candidate.

## Baseline and deliberately small change

Starting clean HEAD: `431ccab3affe24725eaff7d39176661b3398a17f`, the independent
report on accepted step-1 candidate `50c1004b8efb9932b078c7c55a56a2385dfdfb18`.
Read `INDEPENDENT_REVIEW_0_6_0.md` first. Preserve both commits and that report.
rndr remains read-only at `44f5dc7c0c91af56e4f3e29f97283b8e8d1e1139`.
Production `apps/medousa-home` is not changed.

The real collision point was `src/main.rs` (923 lines). Existing `backend.rs`,
`conversation.rs`, `menus.rs` and `presentation.rs` already provided useful seams;
keep them. No new framework, parallel protocol, generic store, action bus,
trait hierarchy, persistence or feature is introduced. Extract existing bodies,
keep entity creation and scheduling order, use application-private visibility.
`Model` and `Ui` remain concrete ECS resources; this is file ownership, not an
attempt to make mutually untrusted plugins. Rust visibility does not enforce the
single-writer rule; reviewers must enforce the ownership contract below.

### Exact before → after map

Symbols refer to the starting commit's `src/main.rs`; source spans below
are delimited by unique symbols/statements, not shifting line numbers.

| Before | After | Meaning |
|---|---|---|
| `FixtureObservation` | `fixtures.rs::FixtureObservation` | explicit offline fixture marker |
| `SubmitBoundary` | `composer.rs::SubmitBoundary` | input batch safety |
| `Model` | `state.rs::Model` | same fields, no new state |
| `Ui` | `shell.rs::Ui` | same entity handles |
| `main`: arguments through observation timer setup | `main.rs::main` | authority boundary stays in one entry point |
| `main`: `if let Some(dir) = export` body | `fixtures.rs::export(&mut App, PathBuf)` | same nine synthetic cases and updates |
| `main`: native run fallback; all of `app()` | `main.rs` | sole schedule/plugin/resource wiring |
| `row`, `muted` | `shell.rs::{row,muted}` | stable private layout helpers used by menus/composer |
| `setup` | `shell.rs::setup`, with composer subsection in `composer.rs::setup` | same spawn/parent order and final focus |
| within `setup`: sticky note through behavior controls | `composer.rs::setup(&mut World, center) -> Handles` | seven entity handles returned, not a new runtime model |
| `reset_prompt` | `composer.rs::reset_prompt` | synchronous marker lifetime unchanged |
| `dispatch` | `controller.rs::dispatch` | generation-stamped enqueue/failure handling |
| `guard_submit_boundary` | `composer.rs::guard_submit_boundary` | front-of-Input admission safety |
| `update`: reply loop then actions | `controller.rs::update` | canonical state transitions, no new events |
| `update`: status projection through send style | `shell.rs::sync` | view sync after all actions |
| `update`: observation predicate | `fixtures.rs::sync_observation` | final sync, same five conjuncts |
| existing adapter/render/theme/test files | unchanged behavior, `menus.rs` imports explicit dependencies | no SDK or framework change |

For exact extraction boundaries see `split-evidence/extract.py` and
`extract_composer.py`: these assert the source SHA before moving text. Import
cleanup and rustfmt follow extraction; these are one-shot provenance scripts,
not a migration command to rerun against the candidate. `git diff` is authoritative.

## Data flow and interfaces

1. `main::app()` installs UiPlugin then MarkdownPlugin, resources, the composer
   guard **at the front of Input**, shell Startup and controller Update. Preserve
   that order. `main` parses offline/export/observe flags before any worker or
   credential path. Connected startup validates loopback before worker admission.
2. Shell Startup mounts root/navigation/empty state/transcript; calls
   `composer::setup(world, center) -> Handles { composer, model, send, cancel,
   runtime, mode, behavior }`; inserts `Ui`; focuses the same Prompt entity.
   `Handles` is assembly-only, not a second resource or duplicated draft.
3. Native core input → `SubmitBoundary.safe` and native `Submitted` snapshots;
   canonical buttons/host appearance intents → existing opaque action strings.
   Controller consumes replies first, then action intents, then safe submissions.
4. `backend::Request { generation, command: Command }` passes through an
   unbounded channel to the existing worker. Commands remain New, Recent,
   Open(id), Send(shared InteractiveTurnRequest), Cancel(session), SetMode.
   No app-owned alternate wire shape. `Reply { generation, update: Update }`
   returns via the rndr feed. Library `Chat` applies chronological stream events.
5. Controller stamps generations on navigation/send; rejects stale replies except
   catalog Models/Modes and Unavailable, exactly as before. `History`/settlement
   replace canonical projection; mode always uses server-effective metadata.
   Existing `Command::Recent` already calls `client.sessions().list(1)` and then
   `history` on the first stable `session_id`; `Command::Open(id)` already
   reconciles an explicit session. There is no app session-list projection or
   list-selection action yet. Future list UI should reuse that SDK API, not
   invent a daemon protocol.
6. `shell::sync(&mut World)` projects state, borrows canonical history/live strings
   by temporarily moving and restoring vectors synchronously, invokes
   `conversation::sync_iter`, updates menus, labels, bounds and enabled states.
   No event/async boundary may be inserted while vectors are moved out.
7. `fixtures::sync_observation` runs last, only under `observe`. It may promote
   to Public only with the explicit fixture marker and no commands, history,
   live messages, recovery or composer text. It is not general chat capture.

The `row`/`muted` helpers live in shell for minimum diff. Composer and menus
therefore depend on those stable helpers; shell calls their mount/update APIs.
This is a deliberate compile-time module dependency, not a second schedule or
runtime callback cycle. Changing helper signatures is an architect-serialized
interface change. Do not duplicate helpers or relocate them per lane casually.

## Ownership and invariants (one editor per file)

| Owner / lane | Exclusive files | Reads / contract |
|---|---|---|
| A — session/authority + SDK senior | `state.rs`, `controller.rs`, `lib.rs`, `backend.rs`, `backend_tests.rs`; future `session_tests.rs` after architect registration | sole runtime transition writer; adapter never writes ECS state directly |
| B — shell/navigation senior | `shell.rs`; future `shell_tests.rs` | reads Model, owns Ui handles, root/navigation/empty layout and projection; no admissions |
| C — conversation/input senior | `conversation.rs`, `composer.rs`, `menus.rs`; future `composer_tests.rs` | owns transcript virtualization/copy and native Prompt/menu trees; emits existing intents, never sends SDK commands |
| D — appearance/evidence senior (fractional initially) | `presentation.rs`, `fixtures.rs`, `observation_tests.rs`, `scripts/capture_native.py`; new evidence subdirectory per packet | semantic theme/fixture truth, no auth/live capture; changes observation predicate require authority review too |
| Principal architect — shared wiring only | `main.rs`, `Cargo.toml`, `Cargo.lock`, existing `ui_tests.rs`, `interaction_tests.rs`, `qualification_tests.rs`, team docs and evidence index | alone registers new modules/systems/test files, changes shared handles/types/interfaces or dependencies; coordinates atomic cross-lane edits with file owner |

A owns session identity (`Chat.session`), generation/loading, catalog selection,
mode/depth, pending admission/recovery and durable/live projection contracts.
The editor draft is canonical in the Prompt/Label ECS entity, not `Model`.
C owns draft manipulation through `reset_prompt`; only controller-approved send
or explicit empty-composer recovery calls it during normal operation. Export
fixtures/tests are explicit synthetic exceptions. No per-session draft map or
pending-action abstraction is added here; introducing either belongs to A with
C's contract review, not a speculative pre-extraction.

Critical gates:

- Never retain `FieldTransactions` after synchronous reset, including any new
  early-return path. Reset scroll to zero; keep editor identity on theme changes.
- Refuse Enter batches with later pressed keys or IME commit/preedit ambiguity.
  Do not convert end-of-batch Submitted into a claimed Enter-time snapshot.
- A successful local enqueue is not server admission. Recovery entries are
  removed only on matching-generation Admitted. Failures remain uncertain;
  never auto-resend. Restore only by explicit action into an empty composer.
- Never reconcile by prose equality. Keep session/execution authority, stream
  turn identity/sequence, generation fences and non-atomic-history notices.
- Validate loopback before loading credentials; no automatic discovery/provision,
  proxies, redirects, remote fallback or credential forwarding. Keep sensitive
  header marking and shared SDK/types contracts.
- Observation defaults Deny and remains offline fixture-only. Negative tests
  now isolate history/live/recovery/commands and absent opt-in; existing test
  covers draft. Passing these does **not** widen observation authority.
- Shell disabling and controller admission guards are both required; disabled
  appearance must not be treated as security. Unsupported controls stay honest.

## Collision rules and proposed team allocation

Use **four ownership lanes, not four mandatory full-time implementers**: A and C
have substantial independent work; B is the next shell slice; D can be a bounded
appearance/evidence specialist after fixture freeze. The architect is the only
integration editor; two independent reviewers are separate from implementers.
Do not launch any lane before preparation is independently accepted by sender.
Do not divide state, drafts and recovery among unrelated simultaneous writers.

No two assignments own the same file. A task specifies exact base commit and
paths. Read other modules freely; request interface changes as a small proposed
patch/spec, do not edit another lane's files or shared wiring. Freeze `Model`,
`Ui`, `Handles`, action strings and Command/Update during a parallel wave.
Architect serializes required shape changes first, rebases lane bases, then
reopens the wave. Each lane uses its own new test file and evidence directory;
architect alone adds module registrations. No simultaneous edits to current
large shared test files, manifests, lockfile or evidence indexes. One bounded
Cargo build at a time in this shared target; no parallel builds on it.

### Dependency-ordered first task packets (proposed, not implemented)

**Gate 0 — both independent prep reviews.** Exact candidate commits, all tests,
source maps and export comparisons. Sender decides acceptance; native integration
still requires its own lifecycle. No feature fanout before this gate.

**P1 / A — session navigation and memory-only per-session draft contract.**
Before UI implementation, define how explicit session selection preserves drafts
and recovery separately by stable session ID, and how busy/loading blocks navigation while uncertain admission preserves
recovery and blocks new Send until explicit history reconciliation. Existing
Recent uses `client.sessions().list(1)` and Open accepts a stable session ID;
define a bounded app-local list projection using that same SDK read API, with
architect-serialized Command/Update and action changes if needed. Do not add a
daemon endpoint or SDK protocol. Files: A-owned files + new `session_tests.rs` only.
Acceptance: synthetic two-session switch/back retains distinct Unicode/multiline
drafts, identical prompts remain independent, stale generation reply cannot
replace selected session, uncertain admission never auto-resends, pending recovery
survives reload, no connection means no command. Deliver the exact action/data
contract to architect. Depends Gate 0; blocks P2 connected navigation wiring.

**P2 / B — shell/session navigation vertical slice.** After P1 contract is frozen
and architect adds any necessary handles/intents, implement a bounded session
navigation surface within `shell.rs` + `shell_tests.rs`, using synthetic session
metadata first. No new SDK code. Acceptance: existing empty/start/reload paths,
480/520/1120 widths, keyboard focus, busy/loading disabled navigation, selected
identity and recoverable prompt visibility, no draft overwrite. Compare approved
before/after fixture exports and enumerate intentional visual changes. Depends
P1/interface checkpoint; may run concurrently with P3 after that checkpoint.

**P3 / C — composer/transcript interaction parity slice.** Bound the first feature
packet to keyboard/focus and scroll/copy regression closure before attachments or
approval UI. Files: C-owned files + `composer_tests.rs`. Acceptance: plain Enter,
Shift+Enter, [Enter, later-key], active IME composition/commit, menu search and
Escape, exact source copy, history anchor, multiline six-line cap, same-view
stream append, no Send on scroll/menu/theme. Synthetic tests first; physical
keyboard/IME/clipboard/accessibility remains a separate device gate. Uses frozen
P1 draft interface if changed; does not mutate session/recovery state itself.

**P4 / D — appearance and reproducible evidence packet.** Freeze new P2/P3
fixtures with the lane owners; add only required semantic theme/fixture coverage
and bounded export manifests in a new evidence namespace. Acceptance: dark/light/
high-contrast, narrow popup bounds, draft/editor identity retained, no marker
leak, every observation denial branch, Public only empty opted-in offline fixture,
normal descriptor cleanup. Can prepare harness while P1 runs; fixture additions
wait for P2/P3 contract freeze. No claim of compositor presentation from a surface
copy. No device/live capture without separate authorization.

**Gate 1 — integration/evidence and dual review.** Architect serially registers
lane tests/wiring, runs both feature matrices, clippy/build/fmt and bounded
comparisons, resolves failures and refreshes manifests. Correctness reviewer
checks A and cross-lane invariants; UX reviewer checks B/C/D original images,
input evidence and claim limits. Sender owns acceptance/native merge. Later
approvals/tools/attachments/settings/external-runtime parity must be separately
scoped; no placeholder control becomes an implicit grant.

## Evidence and remaining gates

See `SPLIT_VERIFICATION.md`, `REVIEW_BRIEF_CORRECTNESS.md` and
`REVIEW_BRIEF_DESKTOP_UX.md`. Historical qualification manifests remain bound to
the old source and are not rewritten to pretend they cover the new layout.
Fresh split evidence has separate before/after source/evidence hashes.

This preparation cannot establish compositor/device/live-daemon/paid-inference
or full Svelte parity. Large-message/virtualization limits, local absolute framework
paths and framework Prompt/reset limitations remain. No credential/private chat,
network mutation, push, deploy, SDK protocol or framework edit is authorized.
