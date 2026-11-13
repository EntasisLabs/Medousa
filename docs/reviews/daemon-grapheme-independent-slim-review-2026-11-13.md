# Independent slim Grapheme / ownership undertaking review — 2026-11-13

## Verdict and exact boundary

**PASS for sender native integration review, with two nonblocking low-severity
coverage observations below. No blocking production defect found in the reviewed
delta.** This is not native approval or integration. Sender retains acceptance,
integration and any subsequent publication responsibility.

Reviewed HEAD: **`ceb0f82b45bad269271d0598eb3232a58fc6f32c`**.
Direct parent: **`3b8c895ea6b36bcffd6ba0443f58fb67d7268592`**.
Read the full worker report, manifest/guard/LSP source delta, graph/evidence
contracts, relevant route declaration and enforcement code, packaging regression
harness, and prior independent correction/re-review reports.

Acceptance is the principal's revised scope: match the actual root
`embedded-daemon` Grapheme host configuration; optional capability narrowing is
permitted. The host-free iOS probe is not the reference. No old preserve-heavy
capability gate or upstream blocker challenge is reopened.

The combined ownership/TUI-isolation plus slim candidate is fit to return for
sender integration under that scope. This conclusion combines this independently
examined delta and fresh executions with **version-bound prior ownership and
packaging reviews**, not a claim to have re-reviewed every earlier implementation
line. `9026f46d` through the candidate parent has no source/build/packaging changes
under `src`, `scripts`, `.github`, `Dockerfile`, `Cargo.toml` or `Cargo.lock`.
The earlier harness re-review at `6e64e1d6` closes the prior two P2 issues against
`9026f46d`; the present delta does not change those implementations. Fresh current
constructor, drain, identity, contract and recovery executions add corroboration.

## Findings by severity

### Blocking / high / medium: none within the examined scope

No code correction is requested as an integration prerequisite by this review.
Optional narrowing of saved-script capability is intentional and documented,
not itself a defect under the revised principal instruction.

### Low (P3), nonblocking — negative heavy-import assertion is nondiagnostic

Location: `src/grapheme_lsp_bridge.rs:209-218`.
`slim_interpreter_executes_core_and_rejects_heavy_import` checks only `is_err()`
for the heavy query. It neither inspects the error nor demonstrates that the same
input succeeds with the capability enabled. A syntax/selection/IO/runtime error
could therefore satisfy the assertion without proving feature exclusion. The
input references `never-read.csv` and selects `rows`; the cached 0.7.1 data
signature describes `read_csv` output fields `ok`, `op`, `frame`, not `rows`.
Those are additional reasons not to use this input as a diagnostic positive
control. I did **not** execute an extended-profile version of this input or claim
to establish its exact failure mode in that profile.

Follow-up: assert the intended unavailable-module/operation error before IO, and
use a valid hermetic CSV/schema selection for an extended positive control (or
separate compiler/capability assertions). This is **not a dependency-slimming
blocker**: independent locked graphs establish the absence of the data feature,
Polars, LSP, Stage-B container and WASIX branches, and the guard detects their
reintroduction. The successful core half was also independently executed.

### Low (P3), nonblocking — no mounted-LSP admission matrix in new tests

Locations: `src/grapheme_lsp_bridge.rs:189-202`,
`src/grapheme_handlers.rs:266-275`.
The new 501 test invokes the function directly. The nine request-boundary tests
exercise Host/Origin behavior generically, not authenticated/unauthorized requests
to the actual mounted LSP route. The route inventory test checks aggregate
capability counts and the LSP stream class; it is not a full LSP admission test.
Do not report these as a live or route-specific end-to-end authorization test.

Source examination nevertheless supports preservation: the unchanged GET
`/v1/grapheme/lsp` declaration still uses `AdminExecute`, Portal/nonbootstrap,
NativeOnly, 1 KiB body policy and Stream rate class. `DeclaredRouter` still wraps
the method with browser/capability enforcement before dispatch. The slim handler
has no WebSocket extractor, upgrade callback or server creation. Extended cfg
retains the original lease/revocation/socket implementation. Independently ran
the 12 generic route-policy tests, including deny-before-handler and matching-
principal allow, and the actual Grapheme inventory test, all passing.

Follow-up: mount the real surface in an isolated fixture and test insufficient
capability -> denial, authorized -> 501 without upgrade, forbidden Origin ->
denial, and workspace metadata availability. This would strengthen regression
coverage; no bypass was found in the present unchanged route wiring.

## Manifest, graph and capability assertions independently checked

- Shared direct SDK remains defaults-disabled `host`; compiler/runtime remain
  defaults-disabled. Root `embedded-daemon` is the actual host reference, including
  Stasis's host integration, not the host-free portable probe.
- `full-daemon` and default remain native service compositions and binary gates;
  the seven heavy SDK selectors plus LSP move into additive `grapheme-extended`.
  `tui` selects extended. No native service, authority/lifecycle or durable runtime
  implementation is replaced. `Cargo.lock` is byte-identical to the parent.
- All five current locked/offline normal/build trees were independently regenerated
  and matched the archived raw text and complete package/version/feature inventory:

  | Profile | Before | Candidate |
  | --- | ---: | ---: |
  | Native daemon | 917 | 730 |
  | Native + Iroh | 1002 | 830 |
  | Embedded reference | 624 | 624 |
  | TUI | 955 | 955 |
  | Explicit extended | old daemon 917 | 917 |

- Native/Iroh Grapheme package/features exactly match the embedded reference;
  embedded's graph is unchanged; TUI non-root dependencies/features are unchanged;
  extended restores the old daemon dependencies/features. No added dependency
  package is present in the slim native/Iroh comparison. Native removes 187
  package/version pairs; Iroh removes 172 because of unrelated retained branches.
- The standalone guard independently passes default, explicit daemon, Iroh,
  extended/TUI and terminal positive controls. No Ratatui/Crossterm remains in slim
  daemon profiles. These guard name-counts (643 native, 730 Iroh) intentionally
  differ from package/version pair counts above.
- Cargo features still unify when TUI/daemon are built together. Existing separate
  invocations and **early staging**, not merely `--bin`, protect shipped artifacts.
  Reviewed the adversarial packaging harness; reran 9/9 tests, including output
  overwrite, combined-build rejection and runner isolation controls. These are
  simulations/static PowerShell assertions, not Docker/native release execution.

## Independent executions, provenance and commands

Evidence directory: `daemon-ownership-evidence/independent-ceb0f82/` adjacent to
this report. No production source changes, fixes, upstream edits, rebuild/cleanup,
push, deployment, approval or integration were performed. Review report/evidence
are new **uncommitted** files; HEAD remains the exact reviewed candidate.

Commands executed include:

```sh
python3 docs/reviews/daemon-ownership-evidence/slim-grapheme-2026-11-13/verify.py
python3 scripts/ci/test-daemon-packaging.py
python3 scripts/ci/check-daemon-dependencies.py --offline
python3 docs/reviews/daemon-ownership-evidence/independent-ceb0f82/run.py
python3 docs/reviews/daemon-ownership-evidence/independent-ceb0f82/audit.py
git diff --check 3b8c895 ceb0f82
```

`run.py` records exact per-process binary commands and monitor receipts. It
reuses the inspected existing 8 GiB process-group monitor with jobs=1/offline
and hermetic environment. It **does not compile Rust**. Independently hashed the
worker's current cached lib executable:
`3c000dcea20ec36d682d7c06288f881fa376a71187e6aecad5b054a3a55d4693`.
Its recorded Cargo artifact is this checkout's `src/lib.rs`, features exactly
`full-daemon`, `fresh: false` when compiled by the worker. This reviewer freshly
executed that authenticated binary, not an old heavy-profile binary and not a
new independent compilation.

Fresh results: **72 unique Rust tests PASS** = the 59 focused candidate selectors
plus 12 route-policy tests plus 1 Grapheme inventory test. The three constructor
selectors ran separately with the isolated harness's explicit 32 MiB test stack,
normal inherited TMPDIR, exact ignored selectors, one passed/zero ignored each.
Original 47 suites, symlink cleanup, both slim tests and six retained-host tests
all passed with expected nonzero counts. Retained-host tests cover workflow shell,
dropped-worker cancellation, scoped secrets and direct/engine HTTP denials.
Packaging: **9/9 PASS**. Graph guard and auditor: PASS.
Minimum actively sampled free space was **10.7700157166 GiB**, above the 8 GiB stop
and 6 GiB hard floor. No stop was necessary; no cache or worktree was removed.

Source/evidence checks go beyond repeating the worker verifier:

- 1203 checkout source fingerprints bound to the exact candidate Git content;
  53 evidence hashes rechecked; 185 historical files checked against parent Git
  and candidate Git, unchanged.
- The three implementation blocks in `source.patch.gz` equal the exact
  parent-to-candidate `--full-index` diff; the archive also contains documentation
  blocks and is not solely a three-file patch.
- Before native/Iroh/TUI trees match the prior independent review's captured
  trees byte-for-byte. Parent and baseline manifest archives match parent Git.
- All five regenerated current graph texts match the candidate archives exactly.

Initial reviewer audit-development mismatches are retained in `audit-initial.log`
and `audit-scope-mismatch.log`: raw Git hashes differed from required CRLF
PowerShell checkout bytes (`.gitattributes`), then a three-source-file diff was
incorrectly compared to the broader source-plus-doc patch. The corrected audit
uses declared Git checkout filters and compares the implementation blocks with
full object IDs. Final audit passes. These were reviewer assertion-scope errors,
not candidate test/compilation failures; no implementation was changed.

## Shared-index reconciliation and versioned prior evidence

The earlier concurrent collision is documented, not treated as a clean historical
handback. `fa74520a` and `3b8c895e` are both ancestors and documentation-only; their
review report and all `independent-4e85141` evidence remain byte-identical in this
candidate. The later follow-up preserves the unstaged seal/disk corrections.
The earlier review's `--git-only` qualification remains explicitly tied to
`4e85141`; it does not qualify the subsequent slim working bytes. The new worker
report marks the preserve-heavy assessment historical and acknowledges this
boundary without editing the prior independent review. No active competing
executor was observed during this review; only review-specific files were added.

Prior ownership/harness results are used at their stated versions; the historical
upstream blocker is not a present acceptance condition. This report independently
qualifies the new slim delta and current focused behavior, rather than retroactively
relabeling those earlier reviews as review of `ceb0f82`.

## Limits and integration recommendation

No new independent daemon/TUI compilation, full workspace suite/Clippy, release
link/size, startup/RSS/latency measurement, Iroh compilation, live LSP/WebSocket
roundtrip, heavy extended-module execution, Linux/Windows, Docker image execution,
native PowerShell release or production certification was performed. Worker
current-source build logs remain evidence, not reviewer-executed compilation.
The unavailable `rg` shell guard is not newly claimed green. Graph checks are host
normal/build selections, not every cross-platform graph or a performance result.

**Recommendation: sender may proceed with its native integration review of exact
`ceb0f82b45bad269271d0598eb3232a58fc6f32c`**, combining the retained version-bound
ownership/TUI reviews and this new qualification. The two P3 test improvements
are follow-ups, not demands to restore heavy default capabilities. No integration,
project approval, push or deployment is authorized or performed by this handback.
