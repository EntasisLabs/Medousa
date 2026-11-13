# Independent harness correction re-review — 2026-11-13

## Verdict and source boundary

**PASS: both P2 findings are closed. The bounded ownership/dependency source
candidate is ready for sender acceptance review, not automatically accepted or
integrated.** No new blocking finding was identified within this correction.
The whole undertaking is not complete: effective Grapheme slimming remains
outstanding outside this correction.

Reviewed clean candidate `9026f46d786460c1cad7178fa804876cd6000fb6`, containing
source correction `c0d47829f11f643044b278a8694c4c9023e867ca` directly over the
prior independent evidence commit `71a149b3c982a828c3ea8bd76967191c40ea9091`.
All three source diffs and the complete worker report were inspected. This
review adds only a report and independent evidence. Sender retains final review,
correction orchestration and native integration responsibility. No approval,
integration, push, publication, deployment or production-state change occurred.

## Findings disposition

### P2: default-stack constructor runner — CLOSED

The isolated runner unconditionally exports `RUST_MIN_STACK=33554432`, with an
explanation of the debug constructor requirement. This is local to the harness;
production stack configuration is unchanged. Existing pipefail, separate Cargo
processes, three exact ignored selectors, one test thread and exact nonzero
one-pass summary checks remain unchanged. No catch/skip/zero-match escape was
introduced.

Independently executed the committed runner successfully with no inherited stack
value. The packaging suite also executes the real shell runner against simulated
Cargo under absent, one-byte and oversized inherited stack settings. Each of the
nine invocations must observe exactly 32 MiB and unchanged host TMPDIR. The stale
zero-match simulation must fail. All nine packaging tests passed; the simulations
are not native release/Docker certification.

### P2: fixture root symlink ancestry — CLOSED

The helper canonicalizes only the existing trusted owned TempDir before installing
both the scoped data root and vault override; `vault` is appended afterward. The
TempDir owner remains in Fixture, so canonicalization neither transfers ownership
nor disables cleanup. Production no-follow confinement remains unchanged; the
vault capability path still calls `StoreRoot::open_or_create_nofollow`.

The previous suspicion of missing directory creation was incorrect. The
production capability path creates the vault directory while enforcing no-follow;
the corrected fixture removes the host's `/var` symlink ancestry before reaching
that path. No production policy relaxation is necessary or present.

Fixture acquires the suite environment lock before installing overrides. Its Drop
clears the vault override and restores saved environment values while the lock is
held. Fields then drop in declaration order: scoped data guard, TempDir owner,
previous environment storage, and finally lock. Current-thread tests keep those
thread-local overrides available to constructor/tool polling. This ordering is
unchanged. The fixture is declared before constructor/runtime values, so those
values drop before fixture teardown on ordinary exit/unwind.

The new Unix regression creates an explicit symlink ancestor, invokes the same
canonical-root helper, checks resolution to the real target, proves the directory
still exists with its owner retained, and verifies removal when that owner drops.
It changes no global environment/registry. Independently reran it with an exact
nonzero selector and default stack: 1/1 PASS.

## Real lifetime assertions and scope

The original three constructor tests and production test-hook placement are
unchanged. Hooks and lifetime module remain `cfg(test)`. The retained object is
the real registered agent and sender. The public calendar handler is actually
invoked and its empty events/path result asserted before/after observation loss.
Nonfinal platform drop keeps the receiver live; final drop eventually closes it
while agent Arcs survive. The failure branch first reaches actual handler
registration. The cancellation branch polls the real constructor to that same
checkpoint and drops its pinned owner at the inner scope boundary, rather than
merely dropping an unpolled future. Receiver closure and calendar success are
asserted afterward. This is asynchronous drain-abort behavior, not synchronous
stop-all-workers certification.

The fixed stack does not replace the cancellation assertion or obscure its drop
boundary. Source comparison confirms no weakening of these tests or the earlier
packaging-preservation assertions.

## Independent fresh executions and identity

Rust/Cargo 1.100.0, stable aarch64-apple-darwin, Darwin 27 arm64. Existing configured
`../.cache/cargo-target` reused, locked/offline, jobs=2, serialized execution.
The committed runner was executed under the host's normal TMPDIR:
`/var/folders/cw/ql6_4ftd3nb2mwwjptzz8j3c0000gn/T/`, which resolves under
`/private/var/folders/`. No TMPDIR replacement/canonicalization or external stack
workaround was supplied.

Fresh executions, not merely auditor inspection:

- Committed runner: **3/3 PASS**, each in its own exact ignored-test process,
  with exactly one passed and no failed/ignored tests per process.
- New symlink/owner cleanup regression: **1/1 PASS**.
- Original focused suites: **47/47 PASS**: drain 4, compatibility identity 2,
  settings 5, tool contract 4, spine parity 7, host catalog 4, Grapheme runtime 11,
  recovery 1, request boundary 9.
- Total: **51 unique Rust tests**. No repetition inflates that count.
- Packaging suite: **9/9 PASS**, including inherited stack/temp controls and
  zero-match rejection. PowerShell check remains static only.
- Dependency graph guard: PASS; 803 default/explicit daemon packages, 881 with
  Iroh; terminal positive control and retained Grapheme capability closure pass.
- Equivalent Python ownership scan: PASS over 781 Rust files using the same
  forbidden-symbol regex and roots. `rg` is unavailable on this host: the existing
  shell guard is **not green qualification** despite its misleading exit zero
  when `rg` is missing. This previously disclosed limitation is not fixed here.
- Worker evidence auditor: PASS (39 archive files, 1083 source fingerprints,
  59 unchanged historical files). This integrity check was additional to, not
  a substitute for, the independent Rust and packaging executions.
- Bash syntax and baseline-to-candidate whitespace checks: PASS.

Cargo's machine-readable artifact identifies the current checkout's `src/lib.rs`,
features exactly `full-daemon`, lib test profile, and `fresh: true` (Cargo cache
up-to-date, **not freshly compiled by this reviewer**). The actual executable
path matches the runner's output. Independently hashed that executable:
`c54a858343716ae93208de944c7ff66cf8019c88c5f4b0c2f04655d43cd7bc03`.
This equals the worker's corrected-source build, not the old f44c405 executable.
All 1083 source/build fingerprints were checked before and after the runs.
No foreign artifact or forced rebuild was used. These are fresh executions of
the authenticated current cached binary, not a clean-build reproducibility claim.

The two-second active monitor would interrupt the process group below 8 GiB,
with escalation, preserving the 6 GiB hard floor. Minimum observed free space
was **16.3496 GiB**; no stop was needed. No cache/worktree deletion, full rebuild
or Docker build occurred. Existing homepage/future-incompatibility and large
unwind-section warnings remain in raw logs; they are not suppressed.

## Evidence and history

Independent evidence:
`docs/reviews/daemon-ownership-evidence/independent-9026f46/`.
The lossless archive contains full logs (including Cargo artifact JSON), exact
commands/exit codes, nonzero summaries, environment, executable identity, 1083
fingerprints, disk samples, source patch and this report. The independent driver
reuses the inspected committed monitor helper; that helper's identity is bound
by the immutable candidate-file inventory. The auditor is noncompiling:

```sh
python3 docs/reviews/daemon-ownership-evidence/independent-9026f46/verify.py
```

All candidate-tracked reports/evidence are preserved byte-for-byte. Prior archives
and verifiers were not rewritten. The worker history checker correctly resolves
the two historical fingerprint mismatches against 71a149b3; the old inventory
omitted the packaging Python file. Its saved checker-development failure and
historical current-source rejection are honestly retained, not green Rust gates.
Earlier standalone/TUI typechecks qualify their historical source only. No new
standalone/TUI/Iroh compilation was performed or claimed in this re-review.

## Remaining limits and recommendation

Sender may proceed to its bounded source acceptance review. This report is not
an approval/integration action and does not complete the whole undertaking.

No Linux/Windows execution, Docker image build/execution, native PowerShell
release, release linking/size, full workspace/Clippy qualification, live daemon,
GUI/mobile certification, performance measurement or AOT/WASIX roundtrip is
established. Dependency graph checks do not establish runtime performance or
successful Iroh compilation. Shared Crossterm through comfy-table and retained
Grapheme implementation/signature/AOT/WASIX surfaces remain. Effective Grapheme
slimming and broader architecture/migration work remain outstanding and excluded.
