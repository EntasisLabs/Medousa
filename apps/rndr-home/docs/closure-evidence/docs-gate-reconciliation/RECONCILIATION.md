# Bounded docs-gate evidence reconciliation

## Identities and scope

Reconciliation base: `2cef83fdc3b977bc83b2800a8601f0167bb95a86`.
Original closure direct parent / independently executed review candidate remains
`bdef72eb7fbf8014067603f50bb15cfe0a3745f5`. This reconciliation is documentation
and evidence only, not new candidate execution or P1a work. Original closure's
62-path inventory and other counts describe that historical commit, not this delta.

## Fresh exact observation

Command: `bash scripts/verify-docs.sh --strict`.
Cwd: `/Users/theelevators/Library/Application Support/medousa/forge/worktrees/b906e0f57650/medousa-rndr-home-current-main-preview-requalification`.
Runtime: `cognition_coder_shell_run`, surface `coder_pty`, invoking Python
`subprocess.run` with explicit cwd, separate captured streams, timeout 120 seconds,
no pipeline, no shell flags or verifier modifications. The outer capture completed
exit 0; the verifier's independently recorded exit is **0**.
Resolved Bash: `/opt/homebrew/bin/bash`, GNU Bash 5.3.15(1)-release
(aarch64-apple-darwin25.4.0). Duration: 46.299 seconds.

Exact stdout: `verify-docs: OK\n` (16 bytes). Exact stderr: empty (0 bytes).
Raw streams and complete metadata are `root.stdout`, `root.stderr`, `root-run.json`.
The run executed on the reconciliation base, before these documentation edits.

There is **no** `apps/rndr-home/scripts/verify-docs.sh` in this tree. The actual
root script computes ROOT from its own location and checks repository `docs/`
and selected root architecture/SDK files. It is not an rndr-home-local gate and
does not prove app-doc link or packet completeness. No app-local pass is claimed.

## Historical evidence and runtime limitation

Original verification.json asserted exit 0 / `verify-docs: OK`. Durable ledger
call `call-360d9149-5423-40d2-9e06-cf2a45e7fd05` records the initial action and
call `call-f6808754-46d4-4c9b-aad0-6e5515cad98e` records a later poll with
`verify-docs: OK` in its output. Its saved session was
`3be0294aa2984f4590983dfb16e6089b`. Bounded causal replay reports
`input_payload_available=false`; those ledger summaries alone do not reconstruct
exact historical cwd, full command or process exit. The old assertion is retained
only as a labelled historical claim, not used to dismiss sender's result.

Sender reports the same command from root via `cognition_shell_run` completed
exit 1 without stdout/stderr, and a traced tail rerun also returned 1 without
output. That result remains unresolved, not passing. This turn used the admitted
Coder runtime, not sender's runtime; it did not reproduce or repair that execution
surface. Bash/PATH, runtime or sandbox differences are possible, **not proven**.
No specific environment cause or repository documentation defect is established.
The new success is scoped to the exact recorded Coder invocation only.

## Verdict and preservation

PASS for the freshly observed repository-root strict docs verifier in the recorded
Coder runtime. Sender-runtime root check remains an explicit environment/result
limitation. No app-local gate exists or was substituted. No verifier change or
unrelated baseline investigation was undertaken.

All 55 sealed review files and both original reports must remain byte-identical;
`integrity.json` records the bounded recheck and unchanged full non-doc tree.
No Cargo, app/build/test/export/native execution, external rndr access, worker,
network, feature work, merge, push or production Home changes occurred. Saved rndr
read-only pin remains `44f5dc7c0c91af56e4f3e29f97283b8e8d1e1139`, without a new
external cleanliness claim. Existing cache/build-slot/disk restrictions remain in
force; this reconciliation uses no build slot or cache changes.
