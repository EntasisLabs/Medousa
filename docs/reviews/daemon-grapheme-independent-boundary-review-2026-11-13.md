# Independent Grapheme effective-boundary review

Date: 2026-11-13. Candidate: `4e85141b32d6c74a25427e7506758b38c8683739`,
directly over prior independent review `6e64e1d6ac359ea223baf6754d219ca2b2148a82`.
Same authorized ownership/dependency undertaking; review/evidence additions only.

## Verdict and integration decision

**PASS as an evidence-only blocker handback. ZERO effective Grapheme slimming.**
No new implementation defect or materially false qualification claim was found
in this candidate. The published feature boundaries independently reproduce the
reported obstruction. I did not find a safe in-repository effective reduction
within the existing supported capability contract and the stated no-upstream-
edit/no-vendor-patch/no-functionality-change constraints. This is a bounded
engineering conclusion about these manifests/callers, not a theorem that no
future architectural optimization exists.

**The previously independently reviewed ownership/TUI-isolation slice can proceed
to sender-controlled sealed native review/integration, with Grapheme explicitly
deferred as blocked.** There is no new source change here invalidating that
slice's prior review. This is a recommendation, not approval or integration.
The sender must retain the prior qualification limits and explicitly record the
Grapheme deferral rather than marking the entire original outcome complete.
If whole-undertaking acceptance requires effective Grapheme slimming, that
acceptance criterion remains unmet; split/defer scope explicitly through the
native owner lifecycle rather than silently treating it as satisfied.

## Findings, severity and action

### B1 — Medium, upstream scope blocker: data still forces terminal support

Independently inspected locked registry manifests and fresh standalone inverse
graphs confirm:

```text
Medousa/full-daemon -> SDK/data -> stdlib/data
stdlib -> Polars 0.46 [csv,fmt], default-features=false
Polars/fmt -> polars-core/fmt -> comfy-table/tty -> Crossterm 0.29
```

Polars `fmt_no_tty` selects `polars-core/fmt_no_tty -> comfy-table`, without
selecting its TTY feature. Polars-core's comfy-table dependency already disables
default features. However, features are additive: adding `fmt_no_tty` downstream
cannot subtract stdlib's explicit `fmt`. The existing SDK `slim` feature is an
empty additive feature, not a negation mechanism. Dropping SDK/data would
remove the retained CSV/data-frame implementation; signatures alone do not
preserve executable capability. This is not a defect introduced by this report.

**Action:** separately authorize a narrow upstream stdlib formatting investigation
and patch release, then separately authorize a reviewed downstream lock update.
Retain CSV, implementations, signatures, output/error contracts and policy
checks. Reprove absence of the daemon TTY branch, not just changed selector text.
Do not weaken today's dependency guard to hide the retained edge. A graph delta
is not a speed, RSS or linked-size result.

### B2 — Medium, separate optional upstream boundary: native-unused WASM frontend

Stasis 0.13 `grapheme-host -> grapheme` selects SDK, compiler **and** the optional
`grapheme-wasm` dependency. Its native SDK adapter is gated by `grapheme` and
`grapheme-host`; its WASM adapter is gated by `grapheme` plus wasm32 or absence of
`grapheme-host` (`src/infrastructure/runtime/mod.rs:5-15`). The latter is disabled
on this native profile, but that does not unselect a Cargo dependency. Fresh
normal/build graphs still include the frontend. Removing Medousa's optional
direct WASM dependency activation would not remove Stasis's forcing edge.
Dropping all of Stasis/grapheme would lose its retained native adapter, actually
used by Medousa (`src/grapheme_medousa_bridge.rs:26,121-130`).

**Action:** if pursued separately, split upstream core and frontend adapter
features while preserving a compatible aggregate and both native/browser
consumer matrices. This is not needed for B1 and is not evidence of a second
resident runtime or a large incremental runtime cost. Shared compiler/runtime/
stdlib dependencies would remain.

### N1 — Low, follow-up parity caveat; not a candidate correction request

The proposed `fmt_no_tty` change is plausible, not already parity-qualified.
I extended source binding to Polars-core `src/fmt.rs` and comfy-table
`src/table.rs`. Polars handles most formatting under `any(fmt,fmt_no_tty)`, but
`fmt.rs:759-772` deliberately differs in fallback-width/TTY detection.
Comfy-table `table.rs:269-286` changes terminal-size lookup to stored width.
Consequently **global terminal-table presentation parity is not guaranteed**.
The stdlib data conversion at `src/data/frame.rs:118-172` returns columns/rows,
formats dtype and fallback scalar values, and does not render a whole DataFrame
as a terminal table. This supports investigating the narrow data API, not
asserting all external Polars table behavior is identical.

**Action:** qualify the supported Grapheme output/error contract: null/empty,
bool/numeric/string, dtype and fallback values, CSV malformed inputs and
roundtrips, with relevant formatting environment settings. Check diagnostics
and error formatting too. If upstream promises arbitrary table display parity,
resolve that contract explicitly before changing it. Preserve TUI terminal
support and upstream supported targets. No parity switch or test is implemented
or claimed in this review.

## Challenge of other possible local reductions

- Removing the direct SDK/host selection does not remove host: Stasis/dashboard
  selects grapheme-host, and explicit grapheme-host also remains. SDK/host
  selects stdlib's transforms/HTTP/web/net/email/SQL/Surreal bundle. Both the
  configured allowlist's empty-is-unrestricted behavior and actual SDK fallback
  stdlib dispatch matter (`grapheme_workshop.rs:33-49`, SDK `lib.rs:1893-1913`).
  A discovery catalog or current allowlist is not a safe compile-time product
  contract. Stripping the bundle or replacing dispatch would change capability.
- Stage A check/AOT authoring is directly used (`grapheme_workshop.rs:120-180`);
  that call alone does not demonstrate Stage B execution. Nevertheless SDK/
  wasix-runtime implies stage-b, and LSP's unconditional compiler/full also
  selects compiler Stage B. Removing only the explicit Stage B selector cannot
  subtract that graph. Runtime disabled-feature branches are explicit errors,
  not an equivalent interpreter fallback.
- WASIX/MIR generation activation and default hotload configuration are real
  retained surfaces (`grapheme_workshop.rs:214-263`, Medousa bridge `81-89`).
  Removing WASIX changes supported module execution. This review does not
  infer new live WASIX/AOT certification from source inspection.
- The in-process editor calls `grapheme_lsp::run_server` at
  `grapheme_lsp_bridge.rs:46`. LSP's compiler and signatures dependencies
  request `full` unconditionally. Disabling only LSP defaults cannot reduce
  them. Compiler/signature pruning without implementation/authoring agreement
  is not capability preservation.
- PDF/image/plot/media/data are explicitly retained execution modules, not just
  catalog rows. SDK wires each through stdlib, runtime, compiler and signatures.
  Removing them or substituting another engine is outside this review's contract.
  No authority, cancellation, durability or recovery guard was bypassed.

## Independent observations and evidence integrity

New evidence: `docs/reviews/daemon-ownership-evidence/independent-4e85141/`.
`review.py` records exact commands and two-second disk samples; commands are
serialized, offline/locked where relevant, jobs capped at 2, active interrupt
threshold 8 GiB protecting the 6 GiB floor. Minimum sampled free space across
all recorded commands: **14.9760 GiB** (during the documentation scan; initial
graph/provenance checks stayed above 16.3317 GiB). No disk stop occurred. The
documentation script does not compile; the shared-volume free-space change is
not attributed to this review or used as a performance measurement.

1. Verified exact clean entry HEAD. Candidate diff over prior review is **69
   additions only**, exclusively the worker report/evidence. Independent Git
   archive comparison binds **all 7,811 candidate tracked working files**,
   including historical reports/archives/verifiers and declared checkout EOLs.
   Rechecked the same bytes after observations. No baseline file was altered.
2. Ran the worker sealed auditor using `/opt/homebrew/bin/python3.11`: PASS.
   System Python lacks `tomllib`; no dependency install was attempted. The
   auditor's checks are evidence audits, not additional application test runs.
3. Fresh standalone normal/build graphs for daemon, TUI, daemon+Iroh are
   byte-identical to worker snapshots: **917 / 955 / 1,002 package-version pairs**.
   Fresh inverse feature graphs independently reproduce Crossterm, frontend,
   compiler and WASIX forcing paths. These are resolved package/feature graphs,
   not rustc compilation-unit graphs or linked-artifact measurements.
4. Independently compared the **341** recorded upstream manifest/source files
   against actual registry sources and cached `.crate` contents for 12 packages.
   The `.crate` hashes bind to unchanged `Cargo.lock` checksums. Also bound and
   archived the two formatting source files discussed above. This checks actual
   cache contents, not merely self-consistency of a saved checksum table.
5. Independently rehashed the unchanged test binary:
   `c54a858343716ae93208de944c7ff66cf8019c88c5f4b0c2f04655d43cd7bc03`.
   Worker Cargo artifact reports `fresh=true`, full-daemon lib-test at this
   checkout. Worker receipts/logs record **51 Rust + 9 packaging tests** freshly
   executed there, with nonzero selectors. The 51 count includes 3 constructor,
   1 fixture and original 47 tests. **This reviewer did not rebuild or reexecute
   Rust tests in this pass.** Source/executable hashes and prior independent
   execution establish continuity, not a fresh compiler-to-binary proof here.
6. Fresh independent packaging execution: **9/9 PASS**. Python equivalent
   ownership scan: PASS (781 Rust files). `rg` remains unavailable; the original
   shell guard is **not counted green**. Worker cached daemon/TUI checks remain
   worker evidence, not fresh independent successful compilation in this pass.

The worker report correctly calls its result a blocker and zero reduction,
separates cached validation from rebuilds, discloses failed collection/probe/
whitespace attempts, and does not relabel historical fingerprints as a new
functional candidate. Its “compiled-source” discussion is read as unchanged
source plus resolved cfg/feature evidence, **not** new rustc-unit or end-to-end
heavy-capability proof. No report correction is required on that reading.

## Residual scope and handback

No fresh build, release link/size, performance measurement, full workspace/
Clippy, Docker/image execution, Linux/Windows/wasm qualification, native GUI,
live daemon/media/AOT/WASIX roundtrip, Iroh compilation or platform certification
was performed. Previous slice limitations still apply. No upstream edit,
vendor patch, functionality/format change, migration, Forge reorganization,
push, deployment, publication, spend, approval or integration occurred.

**Outstanding:** effective Grapheme slimming and upstream parity qualification.
**May proceed separately:** previously reviewed ownership/TUI isolation through
sender review and native lifecycle with explicit blocked Grapheme deferral.
The whole original requested outcome is not complete. Sender retains final
assessment, correction orchestration and native integration responsibility.

Sealed evidence can be checked without compilation:

```sh
/opt/homebrew/bin/python3.11 docs/reviews/daemon-ownership-evidence/independent-4e85141/verify.py
```

Strict documentation validation passed independently; the exact monitored
command and output are retained. New review/evidence files also pass staged
whitespace validation. These are review-deliverable gates, not extra product tests.

### Concurrent handback qualification

After the successful immutable-source recheck and initial evidence sealing,
a separate native worker started the next slim-daemon assignment in this same
checkout. It created `slim-grapheme-2026-11-13` evidence and modified `Cargo.toml`.
Runtime observation identifies that worker's manifest write at 01:38:39 UTC.
The final scoped staging attempt encountered an existing Git index lock; the
subsequent commit captured the previously staged review at `fa74520a`, leaving
the last three review/disk-minimum seal corrections unstaged. A follow-up
review-only commit preserves those corrections and this qualification.

**There is no globally clean checkout handback while that worker is active.**
No other worker's files were reset, deleted or committed by this reviewer.
The 7,811-file immutable checks and graph/provenance observations precede that
later source edit; the review applies only to exact `4e85141`, not the new slim
candidate. The sender has been notified of the collision. The default auditor
intentionally rejects current source drift; `--git-only` audits the preserved
candidate Git binding without claiming current working bytes still match.
The next candidate needs its own review and qualification. Do not apply this
review's integration recommendation to unreviewed follow-on changes.
