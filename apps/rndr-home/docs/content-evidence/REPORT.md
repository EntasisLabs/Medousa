# Notes content-surface increment — implementation evidence

**Outcome:** bounded, read-only **Notes summary browsing**, not full Notes/Library
completion. Sender retains independent review, corrections and native integration.
This executor has not self-approved, merged, pushed, published or deployed anything.

## Exact source and scope

- Undertaking: `work-09765685d470952709f6814b15d073c0`, Forge-owned isolated checkout.
- Admission baseline: `a4ea4400482a27183340f738f0d961d571b94157` on the requested
  `medousa/runtime-ui-fixes` target. Admission `git status --short` was empty;
  prior shell, nine-section Settings and Activity sources/tests/docs were present.
- Implementation commit: **`8b51ed79f569dc6bf35587c41749ab8125c3e22e`**
  (`feat(rndr-home): add bounded read-only Notes summary browsing`).
- Source diff from baseline: **15 files, 877 insertions, 26 deletions**; predominantly
  focused tests, scope/guide documentation and a small app-local projection/route.
  No Cargo manifest/lock, daemon, SDK, production Home or rndr source changes.
- The separate unused default-main scaffold was not used. No child worker was
  spawned. This source commit is an executor deliverable, **not integration**.

[Current 18-destination matrix and contract audit](../CONTENT_SURFACES.md).
[User guide](../../../../docs/guides/rndr-native-preview.md).

## What works, and what does not

Notes now uses the existing SDK GET `/v1/vault/notes?limit=100` contract with the
existing closed read worker. It offers local filtering, list → summary detail →
Back, explicit refresh, serial-fenced replies, preserved Back viewport/row focus,
responsive focus restoration and distinct offline/loading/empty/no-match/error/
permission/unsupported states. No input-safety dependency was enabled.

Only title, inert vault-relative Markdown path and modified UTC are projected.
Tags, bodies, raw kind, backlinks, extra fields and cursor strings are withheld.
Unsafe or duplicate paths and `reset_required` pages fail closed without echoing
input. HTML/script/link-looking titles stay literal native labels, never Markdown
views or external link actions. Hidden/system/developer, Ask/link-test and
chat/session/profile-tagged notes are excluded, with no opt-in or tag-based grant.
The source audit confirmed profile/session tags can be automatically added to
ordinary authored notes: **this conservative view can omit legitimate notes,
especially on non-default profiles**. It is not a generic secret detector.

This is always the **first page only, up to 100 summaries before exclusions**,
even when old adapters omit pagination metadata. Empty visible page does not mean
empty vault. No cursor continuation, root switching, note-body reading, attachments,
creation, editing, deletion, filesystem opening or execution controls were added.
Files and Artifacts remain deferred for the explicit adapter/isolation work in
the matrix, not because the entire backend lacks read support.

Five of the original 18 doors now have usable **bounded** previews (Chat,
Notes, Activity, Schedules, Settings); **13 remain deferred**. None is asserted fully
complete or production-ready. Shell/native workspace parity is still partial.

## Checks actually run

All commands ran in this admitted checkout with a worktree-local
`CARGO_TARGET_DIR="$PWD/target/rndr-content"` and `-j 2` for bounded build use.
There was no dependency installation, paid service, external provider, credential
lookup, real-daemon operation or native-device test. Service tests use ephemeral
unauthenticated loopback fixture servers; synthetic exports force offline mode.

| Command (manifest `apps/rndr-home/Cargo.toml`) | Result | Receipt |
|---|---|---|
| `cargo test --manifest-path … -j 2` | **PASS: 6 library + 90 app**, 0 doc tests | [default](test-default.log) |
| `cargo test --manifest-path … --features observe -j 2` | **PASS: 6 library + 98 app**, 0 doc tests | [observe](test-observe.log) |
| `cargo build --manifest-path … -j 2` | PASS | [default build](build-default.log) |
| `cargo build --manifest-path … --features observe -j 2` | PASS | [observe build](build-observe.log) |
| `cargo clippy --manifest-path … --all-targets -j 2 -- -D warnings` | PASS | [default lint](clippy-default.log) |
| Same clippy with `--features observe` | PASS | [observe lint](clippy-observe.log) |
| `cargo fmt --manifest-path … --check` | PASS | [format](fmt.log) (empty on success) |
| `bash scripts/verify-docs.sh --strict` | PASS, `verify-docs: OK` on final indexed guide changes | [strict docs](docs-strict.log) |
| Scoped source/current-doc `git diff --check` | PASS | Recorded in native tool receipt and source commit admission |
| Updated guide/matrix/report local Markdown links | PASS | [scoped links](scoped-docs-links.log) |
| `…/debug/medousa-rndr-home --export-notes …/renders` | PASS: **108 PNGs** | [export](export.log) (empty on success), [frames](renders/) |

New nonempty `.log` receipts have redundant final LF bytes removed, retaining one
final LF; no nonblank output was changed. [Normalization provenance](log-normalization.json)
records original/normalized SHA-256 values. Existing integrated evidence was not
rewritten. [Source manifest](source-manifest.json) pins all 15 source/doc files;
[artifact seals](artifact-seals.json) pin the final evidence and all 108 PNGs.

These are **scoped app checks**, not a monorepo CI or production qualification.
Existing upstream rndr Cargo-manifest warnings (`redundant_homepage`) still appear;
strict app code lint passes. No upstream changes were made to silence them.

The final default binary used for exports has SHA-256:
`3d81989b8f7c2448bd659ff37e612de3db6c2192731d202c8742c75a8cfba5e5`.
The binary is retained under the ignored local target directory, not committed.
[Reproduction commands](reproduce.sh) write fresh receipts into ignored
`target/rndr-content-evidence` rather than overwriting committed evidence.

### Focused regressions and preserved behavior

**11 new regressions** plus the extended all-resource exact-contract test cover:

- Exact GET method and fixed head-page query, no authorization header in fixtures,
  malformed schema, 401/403/404/500, empty success vs errors, reset-required pages.
- Traversal/absolute/URL/drive/encoded/query/fragment/control/bidi identifiers,
  duplicates, allowed Unicode paths, bounded labels/rows and old adapter behavior.
- Hidden/system/developer/Ask/link-test and chat/session/profile exclusions;
  bodies/tags/kind/cursor/extra-field privacy; raw failures never projected.
- Literal script/HTML/link-looking content in native labels, never Markdown/link
  actions; case-insensitive filtering of local projected fields.
- Enter activation, detail → Back focus and same-size scroll, filter retention,
  no-match, responsive traversal/focus, first path visible at 480×650 with real
  Parley text measurement, refresh-selection reset and stale-reply rejection.
- Distinct loading/offline/empty/unavailable/permission/incompatible/disconnected
  states, no row activation in empty/failure states, offline Refresh disabled.
- Cross-resource reply/navigation isolation, fresh-window snapshot isolation,
  unsent Unicode draft/session preservation and unchanged native-input denial.

Existing chooser/Settings/Activity/appearance, transcript, Jump to latest,
virtualization, observation/privacy and input-admission suites pass in both
configurations. All **five denied chat actions** (Send, Restore, New, Recent,
Reload) remain denied; no input-gate code was changed.

## Pixel evidence — deliberately narrow signoff

108 synthetic GPU exports = 9 Notes states × 3 themes × 4 sizes:
480×760, 1120×820, 480×650 and 1120×650. States are list/detail/no-match/empty/
offline/loading/error/permission/unsupported. These are not live data, native
window captures or physical input observations. All retain explicit fixture labels.

**Actual delivered pixels inspected and accepted for this implementation signoff
are limited to these four PNGs** (not all exports):

| Exact relative path under this directory | Size | Inspected result |
|---|---|---|
| `renders/light-narrow-notes-no-match.png` | 480×760 | Filter text and no-match state visible; wrapped bounds copy and fixed dock fit |
| `renders/dark-wide-650-notes-list.png` | 1120×650 | Grouped rail, current Notes route, first-page limits and real row buttons fit without overlap |
| `renders/high-contrast-narrow-notes-list.png` | 480×760 | High-contrast first-page list, two rows, fixture marker and reachable dock fit |
| `renders/light-wide-notes-detail.png` | 1120×820 | Back, title, inert relative path, UTC and coverage fields legible; all summary fields visible |

The original on-disk PNG hashes and the image tool's delivered-artifact hashes
are **different byte identities** and are recorded separately in the seals. The
viewer supplied its own image artifact; no byte-equality or raw-source-image hash
attribution is assumed from an image-open receipt. Inspection refers to those
delivered pixels for the named source paths, not a separate byte-level comparison
of image encodings.

These span all three themes and both widths, **not every theme/width/state
combination**. Other image-open requests/receipts were made during exploration;
receipt metadata alone is not visual verification and does not expand this narrow
signoff. **104 exports are not claimed visually verified.** In particular the
480×650 minimum-height exports and several failure/loading/offline/permission
combinations retain visual-review gaps, notwithstanding layout/state unit tests.
No sample is an actual user-device or service screenshot.

An optional Python contact-sheet attempt could not run because Pillow was absent;
no sheet was produced and no package was installed. Inspection used direct image
attachments instead. No contact-sheet or all-export visual qualification is claimed.

## Remaining qualification and prioritized next surfaces

- Authorized live-service checks: actual vault identity/root, ordinary imported
  versus profile/session-tagged notes, first-page ordering/timestamps, reset,
  permissions and offline retention. No real daemon was contacted here.
- Physical devices/native windows, OS scaling, pointer/keyboard/scroll ergonomics,
  assistive technology bridge/announcements and native accessibility remain open.
- Physical IME/composition/empty-across-frame proof remains separate; all five
  sensitive chat actions stay denied. No rndr repair was attempted.
- No full Library folder/group tree, complete inventory, pagination, multi-root or
  full note reader/editor; no Files or Artifacts completion claim.
- **Next:** generation/root/profile-aware Notes pagination; then a reviewed bounded
  plain-text note reader. Alternatively audit an explicit session-scoped Artifacts
  metadata adapter (existing POST envelope) without raw HTML. Calendar or Scripts
  definition browsing follows audited read contracts; Files needs the inventory/
  locality bridge before safe generic browsing. No execution or content writes.

Sender must independently review the exact candidate, assess limitations, request
corrections as needed and decide native integration. This report cannot waive that
review or approve the whole desktop app.
