# Historical qualification archive — candidate 4998468

The original files in this directory (all pre-existing `.log`, `.json`, `.txt`,
`.gz` and `run-*-tests.py` files) belong to implementation candidate
`49984680858d50da8ec424709b943c62a1bfe589`, over baseline
`effb18c2836df4ce2e5e88cd46876c685b0bb318`. The original report is
`../daemon-ownership-and-dependencies-2026-11-12.md`.

**The recorded 47 test passes and Rust check/typecheck logs do not qualify the
corrected sources.** The independent review reran the old tests against the old
candidate; that does not execute the newly added platform lifetime tests. The
original `source-fingerprints.json` is intentionally historical, not a current
source verifier. Do not rerun `run-focused-tests.py` / `run-existing-tests.py`
into this archive: they overwrite original evidence.

The correction pass preserves every original archive file byte-for-byte. Its
separate records live in `correction-2026-11-13/`, including:

- `archive-fingerprints.json`: immutable original-file hashes and exact old source
  candidate identity, checked against Git objects for that candidate.
- `source-fingerprints.json`: final corrected source/config/document hashes.
- `checks.json` and logs: checks actually run on corrected sources, noncompiling.
- `unrun-rust-tests.json`: exact three actual-platform regression selectors and
  isolated commands, explicitly **not compiled or run** in this correction pass.
- `corrections.patch.gz`: exact correction source/config diff, not a test result.

See `../daemon-ownership-corrections-2026-11-13.md` for the correction contract,
limitations, inherited Docker defects, and final sender-review boundary. No old
executable, receipt or old green result is relabelled as fresh Rust qualification.
