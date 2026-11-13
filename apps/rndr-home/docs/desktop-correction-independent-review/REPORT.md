# Independent preservation-correction review

## Exact scope and verdict

- Review baseline: `b5b2fa9cb116cd8cd25275d61a28a15b06abeb80`.
- Corrected source: `542fe2dc98582a628e93c7f9aa46cb41cce3e786`.
- Corrected source tree: `f10e1070a2d24d8180c7786fc3a41be5ce4a4ca7`.
- Reviewed worker terminal/report head:
  `4e9a7e72f52e3ce4c7a8b552ec0e05c893f80fef`.
- This additive independent report's commit is returned in the handback. It does
  not amend or replace either source or worker evidence.

**Both original P2 defects are fixed in the bounded app-local preview.** No new
P0/P1/P2 implementation defect was established in this correction. Recommend
bounded local integration **for the sender's decision**, with the P3 evidence
clarification and explicit qualification gaps below. This is not approval,
implementation acceptance, native integration, unrestricted visual signoff or
project completion. The sender retains acceptance and integration responsibility.

## Concrete finding

### P3 — contact-sheet file SHA is confused with image-view rendition SHA

The worker `REPORT.md` and `visual-inspection.md` identify
`minimum-height-contact-sheet.png` as SHA-256
`b7a8beffd6a40839b5344b50667f8f7f85952b11728d43b9030a8c7dabb11ae2`.
That is the image-view output rendition identifier, not the committed PNG hash.
The committed 1600x1950 PNG is SHA-256
`3de83f722c4f235aee16875e7883963bf3a953eb431a2e4e191a7e806eb7c2f2`.
The worker's `seal.json` correctly records the committed file hash, and all its
file hashes match. This is a documentation/provenance clarification, **not a
source-seal failure or proof that pixels were altered**. Correct in a future
additive clarification; this reviewer does not edit immutable worker evidence.

## Independently established correction behavior

### Focus and keyboard/accessibility identity

The dynamic rebuild now captures focus by region plus Button action, moves it
through validated focus migration before deletion, and requests the replacement
through the normal framework path. Region scoping avoids matching a repeated
resource link in the wrong navigation/body region. Missing equivalents use the
visible Settings rail or dataset filter, not the hidden Preferences filter.
The original Preferences resize assertion survives unchanged. Permanent tests
exercise all three appearance controls across repeated 480x650/1120x650 resize,
semantic action, label, focused/focusable/enabled AccessTree node and activation
permission; synthetic Enter, Tab and Shift-Tab; sidebar/pager/link equivalence
and missing-control fallback. This establishes app-local synthetic identity,
not physical keyboard, Space, focus-ring, IME or assistive-technology qualification.

### Dataset selection/entities/focus/scroll and visible refresh

`Desktop.begin`/`settle` invalidate only a view consuming that resource. Accepted
off-route replies still update their own cache and generation state; stale fencing
is unchanged. Both permanent and added independent tests verify that unrelated
begin/completion preserve visible selection/focus; the added long-list test also
compares the actual body child entities and nonzero scroll before and after.

A separate intended offset survives the short Loading placeholder; settlement
rebuilds actual new results and layout clamps against their measured content.
Permanent tests verify refreshed 90/70/2/0-row results, matching visible record
counts, restored -200 offset for long results and zero for short/empty results.
The independent edge probes additionally verify error clamping, changed filter
while Loading, explicit navigation during a pending read, filter reset, entering
and leaving a scrolled long detail, and return-to-list reset. Refresh from selected
detail clears the positional selection and displays new data. This is numeric
position retention, **not stable record-key anchoring through reorder**.

### Preserved chat and denied authority

Draft, session and away-scroll preservation pass through navigation, repeated
responsive resize and unrelated accepted reads. `native_input_ready` remains
unconditionally false outside tests. The unchanged controller rejects all five
Send/Restore/New/Recent/Reload intents even when visual disabling is bypassed.
The independently rerun full suites include the forged-intent/submission test,
empty-draft Restore denial, stale reads, privacy/redaction, wire fixtures and
observation restrictions. Synthetic-editor opt-in remains test-only. No production
input gate or CLI/runtime bypass was introduced.

## Independent commands and accepted results

Full-suite run used fresh scratch storage, offline Cargo, two Cargo jobs, two test
threads, no incremental compilation and no dev/test debug info. No deadlines,
assertions, tests or app contracts were relaxed. `results.tsv` and raw command/log
pairs record the run; `accepted-results.tsv` records count-checked scratch probes.

| Independently rerun check | Result |
|---|---|
| Original reviewer module on exact baseline | 0 passed / **2 expected failures**, exit 101 |
| Byte-identical original module on corrected source | **2 passed**, plus 2 promoted copies in the same filter: 4 total |
| Permanent preservation focused group | **9 passed** |
| Added independent edge cases | **3 passed** |
| Default suite | **6 library + 71 app passed**, doc tests 0 |
| Observe suite | **6 library + 79 app passed**, doc tests 0 |
| App formatting | Pass |
| Strict app-scoped all-target clippy, default and observe | Both pass |
| App builds, default and observe | Both pass |
| Offline GPU export | **216 PNGs**, all 216 byte-identical to sealed worker exports |
| Additional offline pixel probe | 1 test passed, nine representative PNGs generated |
| Strict repository docs | Pass on reviewed head; final additive-report gate logged separately |

Baseline failures independently reproduce Focus=None and Runtimes offset
-200 becoming 0 after Health completion. The exact original reviewer module was
appended to scratch copies, never tracked implementation. All four original
assertion token sequences match their permanent copies after whitespace/trailing
comma normalization; rustfmt's equivalent closure block is not a weaker assertion.
The exact narrow geometry equality changes 414 to 396 solely because the rail
widens by 18px; its equality and draft-preservation assertion remain intact.

`reproduce.py` retains the corrected independent procedure and asserts exits
**and nonzero expected test counts**, using separate caches per source. It was
syntax-checked; the constituent commands and tests above were actually run.

### Failed/rejected observations are retained, not promoted to passes

1. Sharing a target between copied same-name scratch apps reused the baseline
   artifact: the first after log repeated baseline failures and subsequent filters
   selected zero tests. These are `discarded-alias-*` logs, **not app regression
   findings or accepted test passes**. A completely fresh after target produced
   the explicit accepted counts 4/3/9 above. Raw `harness-results.tsv` records both
   attempts; use `accepted-results.tsv` for accepted findings.
2. The reviewer-only pixel probe initially used `&App` where frame needs `&mut App`.
   Its first compile failure is retained; only the scratch harness was corrected,
   and the final probe actually ran one test and produced nine PNGs.
3. Worker-only high-parallelism logs show two unchanged loopback fixtures timing
   out reading request headers (WouldBlock after the existing 3-second deadline).
   Inspection confirms these fixtures use ephemeral 127.0.0.1 ports, a 3-second
   client/read deadline and 8-second accept deadline. Independent two-thread full
   suites pass without changing those assertions or timings. CPU/scheduling
   contention is plausible, **not proven as the exclusive cause**. An unrestricted
   concurrency pass was not independently established. Keep this test-scheduling
   sensitivity visible; do not call it an app endpoint defect or claim an
   unconditional high-parallelism pass.

## Actual attached-pixel scope and limits

**Correction to the earlier broad progress claim:** repeated direct image calls
returned metadata; metadata, hashes and OCR are not pixel inspection. The actual
runtime-attached pixels inspected in this session are the independently generated
`edge-contact.png` (image-view rendition SHA
`686017da2687d448b8a871cf8aacd68151a46a5f8195c4569a21a2c05759a2b8`).
Do not promote the worker's six-panel visual statement to an independent finding
merely because requests for that sheet returned metadata.

The attached edge contact tiles unchanged source renders without overlays or OCR:

- **Dark,480x650:** scrolled 80-row list; rows around 6–10, a visible body scrollbar,
  ordinary viewport clipping of content crossing the body boundary, complete
  two-line INPUT PROOF footer, no sampled label/control overlap.
- **Light,1120x650:** scrolled 30-field detail; body content continues below fold,
  complete footer and no sampled text overlap. The scrolled detail does not
  establish that its offscreen header/Back control is sticky (it is not claimed).
- **Light,1120x650, Connection:** rail scrolled to You and settings sidebar scrolled
  to Packages/MCP/Connection; lower destinations are present/reachable rather
  than missing. These are programmatically positioned synthetic scroll states,
  not a physical wheel/drag or keyboard reachability qualification.
- **High contrast,480x650, Connection:** last-section pager 9/9, scrolled rail/You,
  wrapped Connection content and complete footer visible.
- **Exact 480x90 bottom crop** of the high-contrast 480x650 Preferences source:
  full two-line truth footer including **INPUT PROOF** is visibly present. The
  crop script uses source pixels without overlays/interpolation. The earlier
  sips/resampled attempt is retained but is not the exact-crop evidence.

This covers actual pixels in all three themes and both widths **as the listed
subset**, not all six theme/width combinations. In particular dark-wide,
light-narrow and high-contrast-wide original-sheet pixels were not independently
available as inspectable attachments in this session. Their sealed renders and
all other full-matrix states were regenerated and byte-matched, **not visually
signed off**. The worker's claimed six-panel inspection remains worker-only.
Ordinary-height windows, every long-list/detail state, focus-ring visibility,
contrast ratios, device scaling and physical/native input/AT remain specific
qualification gaps. No new concrete app glyph-clipping defect was established
in the actual attached samples; do not infer absence of clipping elsewhere.
These are not blanket blockers on bounded non-chat local preview integration.

## Seals, immutability and authorization audit

`audit.json` records exact source/report/tree identifiers, the worker seal SHA,
source/evidence path sets and all 216 independent PNG hashes. Independently checked:

- 32 sealed source file hashes match.
- 529 prior document/evidence hashes match; all 530 baseline app-doc files were
  compared to Git. The only intended prior-document change is DESKTOP_PARITY.md.
- 260 sealed correction-evidence hashes match. The terminal worker commit adds
  only its 261 evidence files; the corrected source is its exact parent.
- The eight-file source correction is confined to app-local preview source/tests
  and the parity clarification. All other production Home/framework/pins/root
  manifests/lockfiles/SDK files are unchanged by this correction/review.

No real daemon, provider, credential discovery/configuration, paid inference or
other effectful administration was invoked. Commands used offline exports or
existing loopback fixture tests only. Compilation reads the existing pinned
framework dependencies; this is not framework investigation or modification.
No implementation source was fixed, no worker evidence amended, no approval,
integration, push or deploy performed. Raw log EOF whitespace is retained rather
than altering authentic captured output. Own bounded Cargo/Swift caches and
scratch apps are cleaned after retaining evidence; see cleanup receipt.

## Honest parity and prioritized safe next slices

Nine settings routes are still read-only/local-preview settings, not editable
production parity. Only Chat/Settings/Schedules implement shell destinations;
**15/18 remain deferred**. Native pane/tab/workspace fidelity remains outstanding.
The rndr input dependency remains separate, with all five actions denied.

1. Production-derived settings chooser/rail grouping and selected-state/keyboard/
   accessibility fidelity; fill the explicit pixel combination gaps and physical
   below-fold/focus checks without enabling input-sensitive actions.
2. One bounded read-only Activity/jobs or Files/Notes list/detail slice using safe
   existing summaries, with loading/error/empty/filter/long-list behavior and no
   execute/retry/cancel/reveal/approve operations.
3. Versioned catalogue/capacity wire fixtures, pending/cached/fresh distinctions,
   response-size/pagination bounds. No live providers, credentials or config writes.

Sender retains implementation acceptance and native integration responsibility.
