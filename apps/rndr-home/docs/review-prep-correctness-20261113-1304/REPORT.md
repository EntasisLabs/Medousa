# Independent architect-preparation correctness / authority review

## Verdict: PASS — one low, nonblocking contract-wording finding

Reviewed 2026-11-13 in undertaking `work-7fd193aac6facae5335d43860ce1bcce`.
No blocking correctness, authority, privacy, extraction, or evidence-integrity defect found in this bounded preparation. This is the correctness lane's recommendation only: the independent UX review and sender's decision remain separate gates. No feature fanout, sender acceptance, native merge, project completion, push or deployment is performed or authorized by this report.

Exact reviewed identities:

- Candidate HEAD: `bdef72eb7fbf8014067603f50bb15cfe0a3745f5` (docs/evidence).
- Starting independent-report baseline: `431ccab3affe24725eaff7d39176661b3398a17f`.
- Source split: `08839824203c57588607f3ad66496e1bd81d8cba`.
- Denial coverage: `b9ee1f1b55fd984231dcb8082fddd745444d5986`.
- Read-only rndr: `44f5dc7c0c91af56e4f3e29f97283b8e8d1e1139`.

HEAD was exact and porcelain status empty at entry. No tracked source/docs, index, HEAD, framework, manifests or historical receipts were changed. All new material is in this unique untracked directory. Final identity/status receipt is `final-state.json`.

## Findings, severity order

### L1 — Low / nonblocking: recovery contract omits explicit transfer exception

**Location:** `apps/rndr-home/docs/TEAM_PREPARATION.md:121-123` (Critical gates), contrasted with `apps/rndr-home/src/controller.rs::update`, `"recover"` arm, lines 247-260; also read the narrower wording in correctness brief question 4.

The team contract says “Recovery entries are removed only on matching-generation Admitted.” Actual code also removes one on explicit Restore: `m.recovery.pop()`, clears `pending_recovery`, and calls `reset_prompt` into an empty composer. This is deliberate, pre-existing behavior, not a split regression or automatic retry. The following sentence mentions restoration but does not qualify the exclusive removal claim. Do not turn the sentence into an acceptance assertion that Restore must retain an extra duplicate recovery entry.

**Reproducer:** synthetic `ui_tests::failed_admission_reload_preserves_visible_recovery_and_newer_draft` exercises send → failure → explicit reload → explicit restore. After the restore branch with an empty composer, the sole recovery entry is popped, prompt exists in the same editor entity, and the channel has no Send. The code's `pop()` makes the removal exact; the fresh full matrix executes the existing regression. A focused reproducible command is:

```sh
cargo test --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2 \
  --bin medousa-rndr-home ui_tests::failed_admission_reload_preserves_visible_recovery_and_newer_draft -- --exact
```

Use the environment/target below. **Minimum correction:** documentation only: “Automatic retirement of pending recovery requires matching-generation Admitted; explicit Restore transfers the last entry into an empty composer and clears its pending pointer without enqueueing Send.” Preserve the implemented guards; no candidate-source correction requested. Recommend clarifying this before turning P1 into a worker's acceptance contract. Candidate docs deliberately left untouched per review instructions.

**Blocking/high/medium findings:** none found within the requested scope.

## Complete extraction / ordering assessment

Read the complete correctness brief, team preparation, split verification and preserved independent baseline report; inspected moved bodies and callers, guard/regression assertions, new denial tests, transport/projection boundary, saved runner and capture implementation, source/evidence manifests and fresh results.

`audit_source.py` in this directory strengthens the saved architect audit: it preserves quoted string bytes during formatting normalization, and adds explicit entrypoint, full app wiring/order, SubmitBoundary and fixture-marker comparisons. **23 checks pass**, including reconstructed original shell/composer assembly and unchanged-file hashes. This is a bounded consistency audit plus code inspection, not a Rust semantic proof. It permits only documented formatting/private visibility changes and explicit extraction adaptations; screenshots alone are not the equivalence argument.

- All Model fields and Ui handles remain the same; Handles is a return value, not a second resource. Private sharing uses `pub(super)`, not a new public API. Model/Ui are ordinary mutable ECS resources; ownership is a review/process contract, not a Rust capability boundary.
- Root, navigation, empty tree, transcript, centered column and composer subsection spawn/parent order reconstruct to baseline. Composer returns the seven handles; shell finishes bottom-space/recovery/status, inserts Ui and focuses the same Prompt. Helpers are unchanged; no hidden mount/dispatch cycle was introduced.
- Complete `app()` wiring is equal: WindowConfig/theme/Model setup, UiPlugin then MarkdownPlugin, SubmitBoundary/Menus resources, front-of-Input guard, Startup setup and Update controller. Entry-point flag parsing and worker/observation setup order is unchanged; only the export body is replaced with the extracted helper call.
- Controller still consumes replies before action intents and safe Submitted snapshots, applies actions in order, then shell sync, then observation sync. Shell synchronously takes and restores history/live vectors around borrowed projection; both are restored before menus/observation, with no async/event boundary while taken.
- Backend, library, conversation, presentation and three existing app test modules are byte-identical to baseline; menu changes are explicit imports only. No Cargo manifest/lock, SDK/protocol or production Home change.

## Authority, recovery, input and observation

- Generation rejection retains exactly Models, Modes, Unavailable exceptions. Navigation and successful begin increment the generation; dispatch stamps it. Mode/history/settled/stream/admission replies remain fenced. Session/execution identity and stream sequence checks remain in Chat; no prose identity reconciliation is added.
- Send retains connection/model-selection/loading and Chat begin's busy/uncertain/session/blank checks. UI disablement is not the sole admission guard. Successful channel enqueue is not server admission: pending recovery is stored first, only matching admission automatically retires it. Failure keeps uncertain state and blocks a new begin; reload/history reconciliation does not silently retire recovery. Explicit restore is empty-composer-only and does not dispatch. Cancel remains a request, not a fake settlement. Existing tests exercise stale replies, failure/reload/newer draft, admission, cancel/terminal/history and disconnected-worker paths.
- Backend sends admission once, follows/reconnects streams and retries bounded reads only; there is no automatic Send replay. Non-atomic history bracketing, active-execution assistant filtering and effective server mode remain unchanged. Loopback validation precedes credential load; HTTP disables proxy/redirects, auth remains sensitive, stream paths are checked, shared types remain authoritative. These are source/synthetic checks, not real credential or connected verification.
- `reset_prompt` inserts FieldTransactions, synchronously resets draft, removes the marker and resets scroll; no new path keeps it alive. Guard runs before native Input; ambiguous Enter-with-later-press, IME preedit/commit and legacy mixed input are refused. Plain Enter and Shift+Enter remain native Prompt behavior. Post-dispatch busy/text equality prevents clearing newer text. Framework regression freshly confirms the known end-of-batch snapshot limitation, not an Enter-time snapshot; reset/marker and app regressions also pass.
- `--export`/`--observe-offline` force offline before any worker. Observation installs explicit marker and Deny before enabling; final predicate still requires no commands/history/live/recovery/draft. All four added private-state tests isolate one conjunct, check repeat sync and enable no descriptor/network; fifth test ensures no-marker empty state never promotes. Prior draft-denial test remains. Synchronous vector restoration precedes this predicate. Tests close the baseline coverage gap without extending capture permission.

## Ownership and collision assessment

Four file lanes are practical with the stated architect-only integration discipline; they are not four independently mutable session/draft stores. A is the session/generation/recovery transition owner; B projects and mounts shell/Ui, C owns editor/menu/transcript trees, D owns synthetic evidence/theme. The shell's synchronous take/restore is the explicitly documented projection exception, not another logical state transition writer. Synthetic fixtures/test setup and entrypoint initialization are explicit exceptions to ordinary runtime ownership.

P1 must establish the stable session/draft/recovery contract before P2 connected navigation. P3 can use only frozen draft interfaces; P4 can prepare harnesses but must wait for fixture freeze. There is no current session-list projection or per-session draft store; these are honest future tasks using existing list/Open SDK operations, not claimed implementation.

The listed shared collision points are real: Model fields and Ui/Handles are physically in lane files but their cross-lane shapes remain architect-serialized with the file owner; action strings, Command/Update, row/muted helpers, main schedule/module registration, manifests and large shared tests cannot change concurrently. No additional unassigned shared implementation file was discovered. Pre-register new lane test modules serially when needed for worker-local verification; do not count an unregistered test file as tested merely because a lane wrote it. One active build at a time in the shared target remains mandatory. No worker was launched here.

## Fresh execution and evidence

Executed the copied/inspected saved runner, adjusting ROOT for the shallower new evidence directory and adding the missing after-command floor assertion (the saved runner sampled after but asserted only before/in-flight). Logs are original, unnormalized. Commands and start/end/pin/environment/binary/disk receipts are in `run.json` and per-command logs. Shared target only:

```sh
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/b906e0f57650/.cache/cargo-target'
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
python3 apps/rndr-home/docs/review-prep-correctness-20261113-1304/run_checks.py
```

The runner freshly invokes:

```sh
cargo fmt --manifest-path apps/rndr-home/Cargo.toml -- --check
cargo test --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2
cargo test --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2 --features observe
cargo clippy --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2 --all-targets -- -D warnings
cargo clippy --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2 --all-targets --all-features -- -D warnings
cargo build --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2
cargo build --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2 --features observe
```

Then three binary exports (dark/light/high-contrast), the explicit offline-empty capture script, strict docs and diff whitespace. **All 13 commands exit 0**. Default **6 + 48 = 54 tests**, observe **6 + 54 = 60**, no failures/ignored/filtered. Existing four qualification tests included. This is fresh execution against the exact candidate using existing dependency/build artifacts, mostly cache hits, not a clean rebuild or whole-monorepo/Tauri CI. Pre-existing redundant-homepage manifest warnings persist; strict Rust/clippy gates pass.

Before every command, every 250ms in-flight and after exit, free disk was checked; process-group stop guard at 8 GiB was never triggered. Minimum sampled free bytes across in-flight and after measurements: **16,312,520,704** (~15.19 GiB). Max two jobs, debug/incremental off, no duplicate target/cache or cleanup.

- Final architect source manifest matches candidate; `SOURCE_SHA256.txt` independently records all 18 source/manifest/lock inputs. All **150** architect evidence hashes match. `compare.py` reproduces these checks.
- All **27 fresh offscreen PNGs** match all three architect stage sets byte-for-byte. `comparison.json` preserves hashes and per-stage equalities. Source audit additionally verifies source bodies rather than relying on image equality.
- Fresh native empty fixture: **1120x820 frame 3**, before-present surface copy, `presented_confirmation=false`; normal timed exit and descriptor deletion passed. `native-comparison.json` is empty: metadata equals the architect final metadata in this run. Pixel timing is not an invariant. Fresh PNG opened through image_view; original hashes are in SHA256SUMS, viewer rendition hash differs normally.
- A reviewer audit-script quoting typo initially failed before running any source check; it was corrected solely in this untracked namespace, then all 23 checks passed. No failed build/test is hidden or counted as successful.

## Limits / handback

No compositor/device certification, physical keyboard/IME/clipboard/accessibility test, live daemon/real credential/provider/paid inference, full desktop/Svelte parity or very-large-message guarantee. Absolute local rndr paths and known framework Prompt/reset limitations remain. Synthetic backend tests may use their test-owned loopback server; no live daemon or remote mutation was used. No new workers or native lifecycle actions.

**Handback:** PASS in this correctness lane, with L1 wording clarification recommended before P1 task dispatch. Sender retains review/acceptance/native lifecycle, must incorporate the separate UX review, and must not interpret this result as a feature-fanout or merge operation. Everything new remains untracked under `review-prep-correctness-20261113-1304/`; preserve and attach these receipts as desired without changing the pinned concurrent-review HEAD.
