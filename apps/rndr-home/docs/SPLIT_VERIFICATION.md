# Module-split verification — 2026-11-13

## Candidate identity and disposition

**Ready for independent preparation review; not integrated or project-complete.**

| Role | Exact commit |
|---|---|
| Accepted step-1 candidate (preserved) | `50c1004b8efb9932b078c7c55a56a2385dfdfb18` |
| Independent report / starting clean HEAD (preserved) | `431ccab3affe24725eaff7d39176661b3398a17f` |
| Behavior-preserving module extraction | `08839824203c57588607f3ad66496e1bd81d8cba` |
| Five observation-denial tests, no observation expansion | `b9ee1f1b55fd984231dcb8082fddd745444d5986` |

The subsequent documentation/evidence-only commit contains this report, the team
contracts, two briefs and all new evidence. Its exact OID is returned in the final
handoff (a commit cannot embed its own OID). All changes remain isolated under
`apps/rndr-home`. No native merge, fanout, push, deployment or project-completion
transition was performed. Sender acceptance of the earlier reviewer handoff did
not accept this preparation candidate or integrate the undertaking.

## What changed

`main.rs`: **923 → 176 lines**, with only entry-point admission/resource/schedule
wiring retained. Five new application-private modules:

- `state.rs` (20 lines): existing `Model` fields.
- `controller.rs` (276): existing dispatch, reply loop and action handling.
- `shell.rs` (288): existing Ui handles, shell mount and state-to-view sync.
- `composer.rs` (160): existing Prompt/control mount, guard and reset adapter.
- `fixtures.rs` (102 after test registration): existing export loop and final
  observation predicate. No capability/policy expansion.

`menus.rs` replaces its root glob import with explicit dependencies. Existing
SDK adapter, library Chat projection, conversation renderer, semantic theme,
three existing app test modules, manifest and lockfile are unchanged. No feature
implementation or general-purpose abstraction was added. See the symbol-delimited
map, data flow, file owners, collision rules and concrete dependency-ordered
proposed packets in `TEAM_PREPARATION.md`.

## Fresh execution, not inherited pass claims

All commands below were freshly invoked **three times**: before edits; after the
pure split; and after adding the five observe-only negative tests. Each stage
has its own saved runner, exact commands, exit codes, logs, binary hashes, source
manifest, offscreen originals and native fixture evidence in
`docs/split-evidence/{before,after,final}`.

```sh
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/b906e0f57650/.cache/cargo-target'
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo fmt --manifest-path apps/rndr-home/Cargo.toml -- --check
cargo test --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2
cargo test --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2 --features observe
cargo clippy --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2 --all-targets -- -D warnings
cargo clippy --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2 --all-targets --all-features -- -D warnings
cargo build --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2
cargo build --manifest-path apps/rndr-home/Cargo.toml --offline --locked -j 2 --features observe
```

| Fresh stage | Default | Observe | fmt / strict clippy ×2 / builds ×2 |
|---|---|---|---|
| Before | 6 library + 48 app = **54** | 6 + 49 = **55** | all exit 0 |
| Pure split | **54** | **55** | all exit 0 |
| Final coverage candidate | **54** | 6 + 54 = **60** | all exit 0 |

No failed, ignored or filtered tests in successful full suites. All four prior
qualification tests still run. New tests in `fixtures::observation_tests` isolate
history, live messages, recovery and command sender denial, including repeated
sync, plus empty-without-explicit-opt-in staying Deny. Existing draft-denial test
still passes. Added tests enable no descriptor, credential, worker or daemon.
This closes the low baseline negative-branch coverage finding for the existing
predicate; it is not a grant to observe other surfaces or data.

**Build class:** authenticated existing shared target and dependency reuse. Before
was largely a cache hit; changed application source was recompiled/relinked after
the split and coverage registration. Neither is a clean rebuild/compiler audit.
Pre-existing redundant-homepage manifest warnings persist, with no new Rust or
clippy warning permitted by the strict gates. This is the focused standalone app
matrix, not whole-monorepo or production Tauri CI.

Runner uses a process-group guard: checks free disk before commands, every 250ms
while running and after exit, terminates below the **8 GiB** floor. One build at a
time, max two Cargo jobs, no duplicate target, cleanup or framework mutation.
Sampled minima across all successful recorded commands:

| Stage | Minimum free bytes |
|---|---:|
| Before | 16,411,328,512 |
| Pure split | 16,381,644,800 |
| Final | 16,362,594,304 (over 15.23 GiB) |

These are sampled minima, not a complete archived disk time series. Run records
also retain before/after per-command free space, command line and timestamps.

An initial split compile failed due to test-only `Arc` no longer imported from
root. The failure and partial run record are preserved in `attempt-1/`, not
counted as a pass. Explicit menu imports and moved observation Ui lookup were
also corrected during extraction before the passing pure-split matrix. A source
body audit initially needed to account for rustfmt dropping a call's trailing
comma; audit normalization now explicitly documents that formatting difference.
No changed image was silently accepted to resolve any failure.

## Behavior/evidence comparison

- **27/27 fresh offscreen exports are byte-identical** across before, pure split,
  final and historical qualification: nine cases × dark/light/high-contrast.
  `comparison.json` stores four hashes per case. No expected PNG was replaced.
- Each runner freshly invokes the observe binary's export paths plus
  `scripts/capture_native.py` against the read-only framework, under the same
  disk guard. Native empty fixture succeeds at **1120×820, frame 3** each time,
  with normal timed exit and descriptor cleanup. Native metadata including
  semantic tree/entity IDs, bounds, focus, enablement, theme and provenance is
  identical across all three runs except the PNG byte count; see
  `native-comparison.json`. Native PNG timing is not an offscreen byte oracle.
- Native provenance is `live-surface-copy-before-present`,
  `presented_confirmation=false`. This is **not compositor observation**.
- Opened representative final originals through image_view: dark/narrow,
  light/models and native-empty. Original file hashes/dimensions are separately
  recorded; viewer renditions have their own hashes. The exhaustive invariant
  here is offscreen byte equality, not a claimed exhaustive desktop UX audit.
- `audit_source.py` passes **19 checks**: moved types/helpers, guard/reset,
  dispatch, reply/action order, projection, observation predicate, export loop,
  reconstructed assembly/spawn order; plus byte identity for eight existing
  implementation/test files. It normalizes whitespace, formatting trailing
  commas and private visibility for moved bodies, with explicit adaptations
  for the helper call, borrowed App and Ui lookup. It is a bounded consistency
  check, not a Rust semantic proof. Strings additionally have export comparison
  coverage; no hash manifest is a signed execution attestation.
- Fresh strict documentation and diff checks passed in every runner; final
  documentation/staged whitespace and source-integrity checks run again before
  sealing the evidence commit.

### Provenance rules

Before runner records clean baseline HEAD. Pure-split run records baseline HEAD
because code was not yet committed; its explicit source manifest binds it to
`08839824`. Final run records `08839824` because only the test registration/file
were then uncommitted; its explicit source manifest binds it to `b9ee1f1b`.
Do not treat a run's starting HEAD as proof that uncommitted additions were
already in that commit. The initially unregistered observation test file is not
part of the pure-split compiled source manifest.

`SOURCE_SHA256.txt` in each stage covers app source + manifest/lockfile.
`SHA256SUMS` at the new evidence root covers the new receipts and scripts.
`environment.json` records toolchain/platform/cache class and source commit IDs.
New log newline-normalization metadata, if present, records reversible trailing
LF normalization and original hashes; no diagnostics are removed. Historical
`qualification-44f5dc7c/`, `QUALIFICATION_0_6_0.md` and
`INDEPENDENT_REVIEW_0_6_0.md` are untouched and remain bound to old source.

## Independent review and remaining risk

Dispatch **two separate prep reviews** using `REVIEW_BRIEF_CORRECTNESS.md` and
`REVIEW_BRIEF_DESKTOP_UX.md`. No workers launched here. Four ownership lanes are
proposed, not four required concurrent implementers: session/SDK, shell,
transcript/composer, fractional theme/evidence; architect alone edits shared
wiring and registers tests. P1 session/draft contract precedes connected shell
navigation; stable-shape UI and fixture work can then proceed without file overlap.

Remaining gates are explicit: physical keyboard/IME/clipboard/accessibility,
compositor presentation, live daemon/auth/provider operation, paid inference,
full desktop/Svelte parity, very-large-message limits and absolute local rndr
paths. Framework Prompt still emits end-of-batch Submitted and reset still
requires a transient transaction marker; app safeguards remain intact. No real
credential, private chat, live daemon or paid inference was used; no network
mutation, SDK protocol change, framework edit, push or deploy. Sender alone owns
acceptance and eventual native integration after independent prep review.
