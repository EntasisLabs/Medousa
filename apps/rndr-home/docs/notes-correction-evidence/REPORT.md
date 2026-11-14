# Notes correction evidence — bounded local preview, not acceptance

## Exact provenance and historical-review boundary

- Undertaking baseline: `a4ea4400482a27183340f738f0d961d571b94157`.
- Exact reviewed prior candidate and correction parent:
  `465d1e14d6d634fe8e7602c13fd2f5d1fdec2fa3`.
- Correction implementation: `1d438f27420a1c390cd7a9e1d689bcdff83ee0a4`;
  tree `978147251d1ce1eb205b00949cbe094c3130f20f`.
- This evidence-only child directory seals that source revision. Its final candidate
  OID is the Git commit containing this report; a self-referential hash is not used.
- `../content-evidence/` remains byte-unchanged historical evidence for the old
  implementation/candidate, not corrected verification or renewed independent review.
- `historical-independent-review.json` preserves exact original REPORT.md and
  probe.log bytes (base64 plus SHA256), read from ignored
  `target/notes-independent-review/`, explicitly bound to **465d1e14**. Decode a
  file's `base64` value to reconstruct it. Original review acceptance was evidence
  acceptance only, not implementation acceptance. No corrected independent delta
  review has occurred. Sender retains review, corrections and native integration.

## Corrections and regressions

1. App-local Notes exclusion tags now use Unicode-whitespace `trim()` before
   lowercase matching. Regressions cover exact session, session-prefixed,
   chat-prefixed and profile-prefixed categories with case, ASCII and Unicode
   padding; an ordinary padded tag remains usable. Existing path exclusions and
   ordinary non-default-profile exclusions remain, not an opt-in broadening.
2. Notes identifiers must fit **240 Unicode scalar characters and 512 UTF-8 bytes**.
   Anything over either boundary rejects the **whole first page**, before exclusions,
   without partial success. Existing relative-path/sensitive-component checks and
   duplicate rejection remain. This is deliberately conservative: even an excluded
   row with an overlong path can hide otherwise usable summaries by making the page
   incompatible. No generic model bound was changed, and no identifier/hash suffix,
   sensitive metadata, path opener, markdown renderer or body reader was added.
   ASCII and multibyte regressions cover valid common prefixes differing after 240,
   byte-limit failure, exact 240-character accepted records, distinct records and
   suffix filtering. Notes-only path labels add visual breaks every 24 characters;
   records keep original paths in full. Queries spanning a visual break still match.
   Filtering remains local over projected text only; discarded title suffixes and
   excluded rows are not searched. The existing title display cap remains unchanged.
3. CONTENT_SURFACES.md explicitly states that 100 first-page rows/display caps are
   **not HTTP byte/allocation caps**. The inherited SDK reads full response text and
   deserializes it before app projection, including extra fields and long strings.
   Timeout is not a memory cap. Scope is a trusted local canonical service, not
   hostile/custom-service hardening. No SDK, daemon, transport or framework edits.

The daemon endpoint enumerates its active vault root, **not a per-chat vault or chat
attachments**. No root switching, pagination, new grants or sensitive-field widening.
All five existing chat denials (Send/Restore/New/Recent/Reload), authority/privacy,
state/refresh fences and navigation boundaries remain covered by the existing suites.
No live daemon, providers, push, deploy, integration or self-approval was used.

## Reproduction and checks

Run from the admitted checkout with its existing local rndr paths:

```sh
bash apps/rndr-home/docs/notes-correction-evidence/reproduce.sh
python3 apps/rndr-home/docs/notes-correction-evidence/seal.py
```

`reproduce.sh` records exact commands, uses the existing isolated
`target/rndr-content`, `-j 2`, default debug-disabled/incremental-disabled profiles,
and only offline synthetic exports / ephemeral loopback fixture services.
Existing target reused (~1.9 GiB); correction run output ~8 MiB before JPEG delivery
renditions. No unrelated build targets or prior evidence were cleaned or rewritten.
The same established scoped configurations were rerun after the final source edits:

| Check | Result |
|---|---|
| default tests | 6 library + 93 app, 0 failures; doc tests 0 |
| observe tests | 6 library + 101 app, 0 failures; doc tests 0 |
| default + observe builds | pass |
| scoped default all-targets Clippy `-D warnings` | pass |
| scoped observe all-targets Clippy `-D warnings` | pass |
| `cargo fmt --check` | pass |
| `bash scripts/verify-docs.sh --strict` | pass |
| correction-only `git diff --check` | pass |

Raw logs stay under ignored target. Saved **new** log copies remove extra LF bytes
at EOF only; `log-provenance.json` records raw and saved SHA256 separately. Interior
bytes and all prior integrated/candidate evidence are untouched. Timings, compiler
paths, GPU pixels and binary bytes are host-specific, not deterministic test or
pixel equality promises. `seal.py` deterministically regenerates manifests of the
saved evidence and verifies exact source bytes; it does not assert review acceptance.
Reproducing exports produces fresh target files; saved evidence is not overwritten.

## Synthetic export vs actual pixel inspection

132 synthetic original PNGs exported across three themes and four sizes (480x650,
480x820, 1120x650, 1120x820), including two new long-label list/detail states. They
use the real Notes projection with accepted 240-character ASCII common-prefix paths
and an overlong multibyte title (display-capped). `export-manifest.json` seals all 132.

Pixel signoff is **only these two JPEG-derived delivered renditions**, not a claim
that any or all 132 original PNG files were visually verified. JPEGs were made with
`sips -s format jpeg ORIGINAL --out inspection/NAME.jpg`. Viewer delivered them as
PNG artifacts; original PNG, JPEG and delivered PNG hashes are separate in
`visual-ledger.json`. Repeated viewer receipts are not extra inspected samples.

| Inspected rendition | Original source export | Actual size | Observation |
|---|---|---|---|
| `inspection/minimum.jpg` | `high-contrast-narrow-650-notes-long-list.png` | 480x650 | Path continuation lines remain within content width; suffix and second record are below fold and require scrolling. Existing fixed-height list-title button clips a long title's second line. |
| `inspection/wide.jpg` | `dark-wide-notes-long-detail.png` | 1120x820 | Complete path with x.md suffix visible on continuation lines, no horizontal path overflow. Existing 240-character title cap ends midword; lower metadata requires scrolling. |

This is representative layout evidence, **not font/Unicode, native-window, physical
input/IME, scaling, accessibility or whole-matrix qualification**. Long-title clipping
in the existing fixed-height list button is a residual presentation limitation, not
silently called parity-complete. No visual signoff of offscreen suffixes, other
states/themes/sizes or all original PNGs is implied by export or hashes.

## Remaining risks and independent-review next step

Summary browsing remains first-100-before-exclusions only; no full inventory claim,
cursor following, profile opt-in, root/profile generation awareness or note bodies.
Conservative exclusions and whole-page rejection can hide legitimate notes. Empty
visible results do not prove an empty vault. Full HTTP response allocation remains
an inherited trusted-local-service limitation. Long-title and font presentation,
live canonical-daemon and native-device qualification remain open.

Independent reviewer should review the delta **465d1e14..1d438f27**, rerun the saved
commands and seals, re-check padded tags in every category, UTF-8/common-prefix paths,
whole-page failure and filtering, chat/authority/privacy/state boundaries, and inspect
the two saved renditions (including disclosed title clipping). Historical independent
review is not corrected-candidate acceptance. Sender alone retains acceptance and
native integration; no merge or approval has been performed.
