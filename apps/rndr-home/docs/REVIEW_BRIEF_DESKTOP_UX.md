# Review brief B — desktop UX and evidence integrity

Independent lane, distinct from correctness/authority review. Review the candidate
commits in `SPLIT_VERIFICATION.md`, `TEAM_PREPARATION.md` and the actual image files,
not only this architect's conclusions. Isolated work only; sender owns acceptance
and native integration, and no implementation fanout is admitted before prep
review. Do not edit source or launch workers.

## Questions and gates

- The candidate changes **module ownership**, not intended appearance or features.
  Read before/after source map and data flow. All 27 offscreen exports should be
  byte-identical across fresh before, pure split, final tests and historical
  qualification. Investigate any mismatch; do not replace expected screenshots
  or declare a new appearance acceptable without understanding it.
- Inspect original dark/light/high-contrast files: empty state, wide transcript,
  520pt narrow transcript, six-line-cap draft, model search popup, runtime,
  behavior, response depth and appearance. Check source dimensions/hashes, popup
  containment, reading lane, scroll/copy controls and honest unavailable states.
  The 480pt native minimum is not itself a captured 480pt export.
- Verify native empty fixture separately: same-frame metadata, size, provenance
  `live-surface-copy-before-present`, `presented_confirmation=false`, timed exit
  and descriptor cleanup. Caret timing may change native bytes; it is not an
  offscreen equality oracle. A native surface copy is **not compositor evidence**.
- Read interaction assertions for Enter/Shift+Enter, same-batch ambiguity,
  composition, model search selection/index, Escape/focus return, theme preserving
  draft/message entities, exact-source copy, scroll anchoring and recovery visibility.
  Explicit Restore transfers recovery into an empty composer and removes its
  recovery entry/pending pointer without Send; only matching-generation Admitted
  automatically retires pending recovery. Restore is not admission.
  Tests synthesize framework input; they are not physical-device certification.
- App capabilities not ported must remain visibly unavailable. Permission,
  secret and budget notices do not authorize this port to approve anything.
  No icon redesign, new navigation feature or full Svelte parity is claimed.
- Evaluate team readiness: B's shell and C's composer/transcript must have separate
  files with stable handles; D's theme/evidence lane cannot mutate runtime state
  or silently bless new exports. Architect owns shared wiring, interface changes
  and test registrations; P1 draft/session contract precedes connected navigation.
- Denial tests cover draft/history/live/recovery/commands and absent fixture
  opt-in, not arbitrary future private surfaces. No observation expansion follows
  from closing the low baseline test-coverage finding.

## Reproduction and evidence integrity

`split-evidence/{before,after,final}` contain fresh logs/run records, native PNG
and JSON, and original export PNGs. `comparison.json` and hash manifests are
consistency checks, not signed attestations. Historical
`qualification-44f5dc7c/` remains source-bound to the old candidate. Never relabel
historical source manifests as post-split evidence. Initial failed compile log
is retained separately and not counted as a passing validation.

For a fresh review use a new evidence directory and the saved runner, reusing the
existing shared CARGO_TARGET_DIR, max two jobs, incremental/debug off and the
8 GiB sampled disk-floor guard. Run only one build at a time. No clean rebuild
claim when Cargo reuses artifacts. Inspect actual originals with image viewer,
not a generated thumbnail alone; record which cases you saw. No credentials,
private sessions, live daemon, paid inference, desktop capture or network mutation.
The native script is limited to the explicit synthetic offline empty fixture.

Return an independent PASS / CHANGES REQUIRED with exact commits, inspected
originals, actual regenerated evidence, mismatches/limits and separate device,
compositor, live and parity gates. Preparation review is not approval to integrate
or a declaration that the desktop replica is complete.
