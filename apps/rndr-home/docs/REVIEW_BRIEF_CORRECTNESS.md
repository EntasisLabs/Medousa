# Review brief A — correctness and authority

Independent review requested, not performed by the architect. Review the exact
commits recorded in `SPLIT_VERIFICATION.md`, rooted at report-only clean baseline
`431ccab3affe24725eaff7d39176661b3398a17f`. Do not review the old qualification as
if it certifies the new source. Sender owns acceptance and native integration;
this candidate is isolated and native merge remains deferred. Do not launch workers.

## Bounded ownership

Read the complete split source, `TEAM_PREPARATION.md`, independent baseline
report, source/evidence manifests and fresh execution logs. Focus on state,
request/reply adaptation and authority. Do not edit candidate source; findings
should identify exact file/symbol, severity, reproducer and smallest correction.
Use synthetic channels/fixtures only. No credentials/private chat/live daemon,
paid inference, framework edits, push, deployment or remote mutation.

## Review questions / acceptance gates

1. Is the move genuinely behavior preserving? Compare every old `main.rs`
   responsibility with new modules; check spawn/parent order, helper bodies,
   schedule order and theme/plugin setup, not only passing screenshots.
   Visibility is `pub(super)` for app sharing, not a public SDK API. Model/Ui
   remain mutable ECS resources: flag ownership-contract violations explicitly.
2. Does controller still apply replies before actions, retain exactly the
   catalog/unavailable exceptions to generation rejection, and preserve all
   send/loading/busy/uncertain guards? Are session identity, pending recovery
   and draft canonical stores unambiguous? Confirm that layout synchronization
   restores temporarily borrowed history/live vectors before observation checks.
3. Keep FieldTransactions only inside reset, plain/Shift Enter behavior,
   same-batch ambiguity/IME guards and text-equality protection against clearing
   newer edits. Read qualification and app regression assertions; native Prompt
   still snapshots end-of-batch, not an Enter boundary.
4. Automatic retirement of pending recovery requires matching-generation
   Admitted. Explicit Restore into an empty composer transfers the last recovery
   entry to the composer, removes that entry and clears its pending pointer
   without enqueueing Send; it is not admission. Errors remain uncertain,
   navigation/reload does not imply successful admission, and no automatic Send
   retry occurs. Do not infer execution identity from prose.
   Existing non-atomic history and server-effective mode contracts must remain.
5. Backend/lib should have no behavioral diff. Validate loopback before secret
   loading, no proxy/redirect/fallback, sensitive auth header, stream path and
   shared Medousa types. No protocol or credential boundary relaxation.
6. Observation: explicit marker, Deny before session enable, offline before
   worker setup, final sync predicate unchanged. Fresh isolated tests must reject
   commands/history/live/recovery; old test rejects draft; absent marker cannot
   promote empty app. No descriptor is enabled by the added tests. Passing tests
   is not permission to observe real chat. Check normal native cleanup metadata.
7. Are four ownership lanes and architect-only wiring/test registration practical?
   P1 state contract must precede navigation integration; cross-lane shapes cannot
   change concurrently. Identify any overlooked shared-file collision. No feature
   work is claimed as implemented by the preparation.

## Fresh verification

Use the already selected shared target, max two jobs, incremental/debug off,
8 GiB sampled disk-floor process-group guard. Run the saved final runner from
repository root only into a **new** review evidence directory (its root depth
must match or adjust ROOT explicitly); do not overwrite architect/historical
receipts. Default and observe tests, strict clippy all-targets default/all-features,
both builds and fmt should pass. App-only focused matrix, not monorepo/Tauri CI.
Check new source hashes and 27 fresh exports; cached dependency reuse is expected,
not a clean rebuild. One active build at a time; no duplicate targets/cleanup.

Return PASS / CHANGES REQUIRED with severity-ranked findings, exact reviewed
commit(s), actual commands/cache class and residual gaps. Review completion is
not sender acceptance, project completion or native merge authorization.
