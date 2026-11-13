# Platform lifetime harness corrections — 2026-11-13

## Scope and exact source

Both independent P2 harness defects are corrected in source commit
**c0d47829f11f643044b278a8694c4c9023e867ca**, directly over clean undertaking
**71a149b3c982a828c3ea8bd76967191c40ea9091**. The latter is evidence-only over
f44c405c3e91bbe160ed148b34265beffff25a1b. This handback is for fresh independent
review; it is **not implementation acceptance, approval or integration**.
Sender retains responsibility. No push, publication, deployment or production
state change was performed.

Only three source/harness files changed (55 insertions, 5 deletions):

- `scripts/ci/test-platform-lifetime.sh` sets **RUST_MIN_STACK=33554432** inside
  this isolated harness, even if a caller supplies an undersized or oversized
  value. The 32 MiB debug-constructor requirement is documented beside it.
  Production stacks are unchanged. The exact three Cargo invocations, separate
  test processes, ignored/exact selectors, one test thread, pipefail and nonzero
  one-pass summary guard are preserved. No external stack workaround is needed.
- `src/runtime/platform/lifetime_tests.rs` canonicalizes the **existing owned
  TempDir root**, then uses that canonical root for both scoped data and vault
  override, appending `vault` only afterward. The TempDir owner, field/drop order,
  environment lock/restoration, vault override clearing and lifetime assertions
  remain unchanged. Production no-follow confinement is untouched. The root
  cause was `/var` symlink ancestry, **not missing directory creation**.
- `scripts/ci/test-daemon-packaging.py` now executes the real runner against fake
  Cargo with no inherited stack, a too-small stack and an oversized stack; all
  nine calls must receive exactly 33554432 and the unchanged host TMPDIR. The
  prior exact-selector/zero-match rejection and packaging-overwrite adversaries
  are preserved. These are executable harness/packaging simulations, not native
  binary qualification.

The new ordinary Unix Rust test creates an explicit symlink ancestor, calls the
same canonical-root helper as Fixture, checks the resolved target, confirms that
canonicalization retains ownership, and confirms TempDir cleanup after drop.
It mutates no process-global environment or constructor registry. The three
actual-constructor tests remain isolated/ignored and explicitly invoked by CI;
all original real constructor/drop/cancellation/calendar assertions and cfg(test)
hooks remain intact. Prior Docker/release fixes and all production behavior are
unchanged by this correction.

## Fresh qualification

Toolchain: **Rust/Cargo 1.100.0**, stable aarch64-apple-darwin, Darwin 27 arm64.
`rustc -Vv`, toolchain identity, cache configuration and commands are preserved.
The repository's existing `../.cache/cargo-target` was reused. Rust work was
serialized with **CARGO_BUILD_JOBS=2**, locked/offline dependencies. No cache or
worktree was deleted; no foreign executable was used. The newly compiled lib
executable has SHA-256
`c54a858343716ae93208de944c7ff66cf8019c88c5f4b0c2f04655d43cd7bc03`.

The monitor sampled available bytes every two seconds and would interrupt the
entire process group below **8 GiB**, preserving margin above the **6 GiB hard
floor**. No stop was needed. Minimum observed free space was **14.9215 GiB**;
free space after qualification remained above 16 GiB. This was incremental
compilation using existing cache, not a clean/full rebuild.

### Executed checks

1. `bash scripts/ci/test-platform-lifetime.sh`: **3/3 PASS**, each selector in its
   own Cargo/test process, each summary exactly `1 passed; 0 failed; 0 ignored`.
   The driver explicitly removes externally inherited RUST_MIN_STACK and does
   **not** modify TMPDIR. Actual host TMPDIR was
   `/var/folders/cw/ql6_4ftd3nb2mwwjptzz8j3c0000gn/T/`, resolving through the
   macOS symlink to `/private/var/...`. No canonical TMPDIR workaround was used.
   The initial revised-source build completed in 44.38 seconds. After the source
   commit, the **committed runner was rerun**, again **3/3 PASS** under the same
   normal host temporary environment with no caller stack value. Repeated runs
   do not increase the unique-test count.
2. New exact `fixture_root_resolves_symlink_ancestor_and_retains_owner` selector:
   **1/1 PASS**, default test stack, same fresh binary.
3. Original focused suites, run sequentially from that freshly compiled binary:
   **47/47 PASS**. Counts: drain 4, compatibility identity 2, settings 5, unchanged
   tool contracts 4, spine byte parity 7, host catalog 4, Grapheme runtime 11,
   recovery 1, HTTP/request boundary 9. Exact command vectors and nonzero summary
   checks are preserved. Together these are **51 unique passing Rust tests**,
   not a full lib/workspace suite.
4. `python3 scripts/ci/test-daemon-packaging.py`: **9/9 PASS**, rerun after the
   final source inventory included that regression script. This includes the
   fixed-stack controls, stale zero-match rejection and adversarial overwrite
   packaging controls. Native PowerShell execution remains unqualified.
5. `python3 scripts/ci/check-daemon-dependencies.py --offline`: **PASS**. Default
   and explicit daemon graphs have 803 unique package names; daemon+Iroh has
   881; terminal positive control and Grapheme capability closure pass.
6. Rust formatting (platform including lifetime submodule, event delivery), Bash
   syntax, workflow YAML parsing, strict documentation validation, Python AST
   validation and baseline-to-candidate whitespace checks: **PASS**.
7. The existing runtime ownership guard exits zero despite **missing `rg`**. It
   is **not counted as a passing scan**. A separate equivalent Python scan of
   the same forbidden-symbol regex and the same two roots passes over **781
   Rust files**; both raw observations are preserved. The production CI guard
   was not changed in this bounded correction.

No new standalone/TUI typecheck was necessary or claimed for this cfg(test)-only
Rust correction; the independent report's fresh f44c405 checks remain historical
qualification of that source. Existing homepage/future-incompatibility warnings
and the macOS large unwind-section linker warning remain recorded, not hidden.

## Evidence provenance and historical limits

New evidence and the noncompiling auditor live at:
`docs/reviews/daemon-ownership-evidence/harness-correction-2026-11-13/`.

Run:

```sh
python3 docs/reviews/daemon-ownership-evidence/harness-correction-2026-11-13/verify.py
```

The fresh archive records exact commands, return codes, test summaries, raw
stdout/stderr, toolchain, normal host temp/stack environment, source patch,
executable hash and disk samples. The new source inventory verifies **1083**
current source/build-file fingerprints: the previous 1082 paths plus the Python
packaging regression script, which the old inventory did not include. All three
changed harness files are therefore fingerprinted. Current-source evidence is
bound to the revised source, not the old executable or old green logs.

**All 59 previously tracked evidence/report files are byte-identical to
71a149b3**, including archives and verifiers. The original independent archive
still verifies its 46 internal files and 1082 fingerprints when the two changed
paths in its inventory are resolved against their historical Git version. The
other changed file (the Python regression script) was absent from that old
inventory. The new history checker initially expected it there; this inventory
assumption was corrected, and the failed checker-development receipt plus the
subsequent passing historical check are both preserved.

The old auditor's **current-source** mode now correctly fails on the revised
runner. That expected exit-1 rejection is also saved. Neither old auditor is
rewritten to pretend that historical qualification applies to these sources.
The old correction disclaimer that its Rust tests were uncompiled/unrun describes
that archived pass only; the later independent environment-qualified run also
remains historical. New qualification here is separate, newly executed evidence.
The evidence auditor checks integrity and observations; it does not rerun Rust
or confer independent review/implementation acceptance.

## Remaining limits

No Linux/Windows execution, Docker build/image execution, native PowerShell
release, release linking/size, live daemon/GUI/mobile, full workspace suite or
Clippy, performance measurement, AOT/WASIX roundtrip or production certification
is claimed. Iroh is graph-checked here, not newly compiled. Shared Crossterm via
comfy-table and retained Grapheme implementations/signatures/AOT/WASIX remain;
**effective Grapheme slimming is still outstanding and outside this correction**.
Drain abortion remains asynchronous and is not a stop-all-background-workers
claim. Broader architecture/migration and unrelated refactoring remain excluded.

Implementation source diff:
`git diff 71a149b3c982a828c3ea8bd76967191c40ea9091 c0d47829f11f643044b278a8694c4c9023e867ca`.
An exact lossless patch is also preserved in the fresh archive. The subsequent
handback commit adds only this report and fresh evidence. Fresh independent
review and sender disposition remain outstanding; no approval/integration gate
was invoked.
