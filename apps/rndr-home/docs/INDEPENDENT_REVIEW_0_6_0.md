# Independent step-1 review — 2026-11-13

## Recommendation: PASS (offline preview baseline only)

Reviewed candidate **`50c1004b8efb9932b078c7c55a56a2385dfdfb18`** against
baseline **`333a14d823ea09c48fdb3bf6224f7e7960029495`** in Forge
`work-7fd193aac6facae5335d43860ce1bcce`. No blocking correctness, privacy,
or evidence-integrity finding in the exercised step-1 scope. Recommend sender
accept this qualification baseline for native integration. This is not approval
of desktop-replica parity or connected/device behavior. Sender owns acceptance
and integration; this review does not perform either.

## Identity and change audit

- Before review, `git rev-parse HEAD` returned the exact candidate above and
  `git status --porcelain` was empty. Worktree:
  `/Users/theelevators/Library/Application Support/medousa/forge/worktrees/b906e0f57650/medousa-rndr-home-current-main-preview-requalification`.
- Read `QUALIFICATION_0_6_0.md`, the complete four-test module, execution runner,
  run metadata, logs, manifests, normalization account, native metadata and
  capture script, and actual baseline-to-candidate diff. The only existing Rust
  source change is a two-line `#[cfg(test)] mod qualification_tests` registration.
  No production API/implementation change is hidden in the diff.
- `Cargo.lock`: parsed both versions independently. Package count is unchanged;
  **every non-rndr package entry is identical**. Only five local rndr packages
  advance 0.4.0 to 0.6.0. Added edges are core → web-time and render → js-sys,
  objc2-foundation 0.2.2, wasm-bindgen, wasm-bindgen-futures, web-sys, web-time.
  No manifest change, registry upgrade, alternate renderer or SDK change.
- Read-only rndr HEAD was exactly
  `44f5dc7c0c91af56e4f3e29f97283b8e8d1e1139`, with empty porcelain status.
  Production `apps/medousa-home` has no baseline-to-candidate diff.
- All **15 source hashes** and **52 evidence hashes** independently match.
  All **28 PNG records** match original byte sizes, IHDR dimensions and SHA-256.
  Four newline-normalized logs reconstruct to their recorded original hashes
  by appending the documented LF byte counts. No diagnostics were lost.
- Original runner records the pre-commit baseline as HEAD because the tests and
  lockfile were then uncommitted; the explicit source manifest matches this
  candidate. Do not interpret `run.json.medousa_baseline` as a claim that all
  qualification additions were present in the baseline commit.

## Fresh independent execution

Executed from the candidate root on 2026-11-13, not merely inspected prior logs.
Used the **existing** shared target (no second target/cache or cleanup):

```sh
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/b906e0f57650/.cache/cargo-target'
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo test --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2
cargo test --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2 --features observe
cargo clippy --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2 --all-targets -- -D warnings
cargo clippy --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2 --all-targets --all-features -- -D warnings
cargo build --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2
cargo build --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2 --features observe
```

All six commands exited **0**. Default: **6 library + 48 app = 54 passed**;
observe: **6 + 49 = 55 passed**, no failed/ignored/filtered tests. This freshly
executed all four qualification tests, interaction tests, backend SDK/loopback
fixtures and observation denial regression. Cargo reused authenticated local
build artifacts; this is not a clean rebuild or independent compiler audit.
Clippy/build checks were fresh invocations, largely cache hits. Pre-existing
redundant-homepage manifest warnings persist; strict Rust/clippy checks pass.

A Python subprocess guard checked free space before each command, every 250ms
during execution and after exit, terminating the process group if below 8 GiB.
No concurrent build, new build target, or cleanup was used. Fresh Cargo minima:
16,403,644,416 bytes or greater. Review logs/results remain locally in ignored
`.cache/independent-review-50c1004/` (not part of the report-only commit).
SHA-256 of raw fresh logs `0.log` through `5.log`, respectively:

```text
ee561849c5ffc64bb074cda60f0c45ce243b3d9edff49815bda019ddf278f1d6
513541751bdd8b96087273aff1432181f269dde6adaed04fc418e2ffb4be752d
600413ead9d91979639db9189bacb9198cb19186e88167fbefa48a2418c16254
8d3e981fe96ef80593085ad38ee83efae0732465bfb6b81c9de8e8baf2c11a41
30dbe36fbd5753fd7aa7e205df0bfecff15d2f82e494252b017033c198f6a538
184a5c3f07872bd174eb91938fc31e47427fa2605fcd8d15cb6c2149e71c08eb
```

## Correctness and privacy contracts

- Framework-only reproducer genuinely uses bare `App + UiPlugin`, Prompt,
  TextField/Label/Focusable and `[Enter, '!']`. Freshly confirms `Submitted`
  contains `hi!`, not an Enter-time snapshot. Inspected pinned framework
  `rndr_core/src/prompt.rs`: submission is published after folding the batch.
  App installs its guard at the front of Input and discards ambiguous Submitted
  actions in Update. Independent existing regression asserts no queued Send,
  retained `hi!` and explanatory status. Limitation remains, safely mitigated
  in the exercised synthetic case; no claim of physical input ordering.
- Inspected pinned `rndr_core/src/field.rs`: reset requires FieldTransactions.
  New test proves bare reset refusal and app adapter reset/clear/scroll-zero
  with immediate marker removal. Existing Shift+Enter/plain Enter regression
  passes, including IME-enabled without active composition. No framework edit.
- Parley test is real measurement, not a fake engine: width reduction grows
  height; appended Markdown retains the same view and grows height; exact raw
  Copy, Hug/16pt inset and single outer Scroll are asserted. Lucide test checks
  nonzero 16px coverage and scalar round-trip only, not future icon-only UX.
- Offline/export/observe flags set offline before worker/credential setup.
  Observation starts Deny, requires explicit offline fixture opt-in, and gates
  Public on no commands/history/live/recovery/draft. Draft denial regression
  passed. Capture script imports framework tools without writing bytecode,
  uses one scoped descriptor, checks before-present provenance, normal timed
  exit and descriptor deletion. No desktop capture or private-data inspection.
- Existing transport validates loopback origin before credential loading,
  disables proxy/redirects and marks auth sensitive. Source and fresh synthetic
  tests retain generation fencing, stream replay checks, uncertain-admission
  recovery/no auto-resend, server-effective mode, and disabled capability truth.
  No production daemon or real credential was used to exercise these paths.

## UI and evidence integrity

Inspected representative **original** PNGs via image viewer: narrow dark
transcript, light model popup, dark multiline draft, high-contrast appearance,
and native empty fixture. Retained transcript/composer lanes and bounded popup
surfaces are visible. Narrow transcript clipping is outer scrolling, not proof
of missing content. Native empty fixture visibly says offline/no inference and
not-ported capabilities. No claim of exhaustive visual/accessibility review.
Image viewer renditions have their own hashes; the original file hashes were
checked separately against the committed manifests.

Also freshly ran the following against the freshly selected observe binary,
under the same disk guard, using `OUT=$PWD/.cache/independent-review-50c1004`
and `BIN=$CARGO_TARGET_DIR/debug/medousa-rndr-home`:

```sh
"$BIN" --export "$OUT/dark"
"$BIN" --export "$OUT/light" --light
"$BIN" --export "$OUT/high-contrast" --high-contrast
python3 apps/rndr-home/scripts/capture_native.py "$BIN" /Users/theelevators/rndr "$OUT/native-empty"
```

All four exited **0**. All **27 offscreen PNGs are byte-identical** to candidate
originals. Observe binary SHA-256 matches recorded
`c684d5c0533ad6525fad97e163b3660d544bce906f69dfc1738107ee67abc5d9`.
Fresh native capture: frame 3, 1120×820, `live-surface-copy-before-present`,
`presented_confirmation=false`; clean timed exit and capability cleanup passed.
Fresh native PNG SHA-256:
`de4e8fa13452191c8757251da42997d9e81cf71e340140b983f5dee3dad07d0c`.
Native frame pixels need not equal the earlier capture (e.g. caret timing).
Minimum free space across these fresh visual commands: **16,399,265,792 bytes**
(~15.27 GiB), above the 8 GiB floor.

These are three different evidence classes: offscreen exports; native runner
before-present surface copy; actual compositor/device observation. Only the
first two exist here. A native surface copy is **not** proof that the compositor
presented those pixels or that physical keyboard/IME/clipboard/accessibility
interaction works. No compositor, physical device, live-daemon, private-session,
paid inference, credential, network mutation, push or deployment verification.

## Severity findings and remaining gaps

| Severity | Disposition | Finding |
|---|---|---|
| Blocking/high | PASS | None found in the tested offline qualification scope. |
| Low, nonblocking | PASS with coverage gap | Observation regression directly asserts draft denial only. History/live/recovery/commands denial is code-inspected, not separately covered by that test. Expand those negative cases before widening observation scope. |
| Informational | PASS, explicit boundary | Prompt chronological submission and reset opt-in limitations remain reproducible. Preserve app guard/marker lifetime contracts during any later split. |
| Informational | PASS, explicit boundary | Absolute local framework paths, huge-message/virtualization limits, and absent device/live/compositor/parity checks are not solved by this qualification. |

The prior report's build/clippy/fmt/docs logs and runner agree with its claims;
this review freshly reran tests/clippy/builds/exports/native capture, not the
whole monorepo/Tauri suite. Final fresh `cargo fmt --manifest-path
apps/rndr-home/Cargo.toml -- --check`, `bash scripts/verify-docs.sh --strict`
and `git diff --check` also exited 0. All source hashes were rechecked unchanged;
rndr remained clean at the exact pin. Before committing this report, it was the
only untracked/changed file; review outputs are ignored. Recorded disk evidence is sampled minima, not an
archived complete time series. Hash manifests establish consistency, not a
cryptographic attestation of original execution; fresh independent execution
and byte-identical exports supply the additional check here.

No candidate source was altered. Commit only this report. After sender accepts
and integrates the baseline, the next authorized stage belongs to the original
coder conversation as principal architect: smallest behavior-preserving module
split and explicit team contracts/assignment plan before senior-engineer fanout.
No architect split, implementation worker, or reviewer worker was launched here.
