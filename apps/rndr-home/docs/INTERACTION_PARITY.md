# rndr chat interaction parity — 2026-11-07

This follow-on compares the native preview with the current Svelte/Tauri chat,
using the two screenshots supplied in the UI-polish request as visual reference.
It changes only `apps/rndr-home` and its guide; the production Home UI, daemon,
SDK, and shared rndr checkout are not modified. Forge work:
`work-6e887b1af060b1fd2607c7ab089054e6`.

## Reference code inspected

- `apps/medousa-home/src/lib/components/chat/ChatComposerBar.svelte`:
  compact composer and footer control placement.
- `ChatModelPicker.svelte`: explicit model choice, provider grouping and search.
- `ChatRuntimePicker.svelte`, `utils/sessionAgentRuntime.ts`, and
  `utils/externalAgentCatalog.ts`: native versus external-agent/provider flows.
- `ChatAgentModePicker.svelte`: registry availability, effective mode, and
  session-scoped changes (not a per-turn authority override).
- `ComposerTurnControls.svelte`, `stores/runtime.svelte.ts`,
  `stores/voicePresets.svelte.ts`: response-depth choices, style/reasoning/
  narration subviews, and the production app's persisted defaults.

## Implemented and bounded differences

| Surface | Native preview now | Difference from Svelte |
|---|---|---|
| Transcript | One outer scrollport; right-aligned, literal user bubbles; transparent assistant Markdown; bounded reading lane; viewport-mounted message subtrees | No selection spanning separate messages; offscreen heights are estimated until measured |
| Streaming | Stable message/view entities, chronological segment updates, follows bottom unless user scrolls up | No jump-to-bottom control or rich tool-event timeline |
| Copy | Assistant message Copy writes raw Markdown through native clipboard | Per-code-block Copy and link opening not wired |
| Composer | Native wrapped Hug measurement, six-visual-line cap, internal draft scrolling, grows and shrinks on edit/clear | Attachments, microphone, skills menu, IME/accessibility device certification remain out of scope |
| Model | Anchored searchable catalog, provider groups, exact provider/model index, explicit checkmark, empty/no-result states | No provider setup, favorites, plan/account metadata, reasoning-capability panel or catalog refresh while menu is open |
| Runtime | Working native-runtime menu; external choices visibly disabled | Does not execute or switch ACP/provider-conversation runtimes. Svelte preferences/bindings are workshop-local browser storage, not shared SDK session settings. This preview always submits native turns and does **not** import that runtime preference. Use the production app for external conversations |
| Mode | SDK registry and effective session mode loaded with history; available choices mutate session scope, wait for server effective value | No local mode fallback or optimistic authority grant. Server task override may keep another effective mode; mode errors are visible/redacted and do not poison history |
| Behavior | Main menu and response-depth subview with Back; Concise/Standard/Deep reach the next turn's request | Window-local next-turn override, not shared persisted defaults. Style, reasoning and narration explicitly disabled/not ported |
| Menu interaction | Pointer activation, search field, keyboard button activation/focus, Escape, close, outside click, focus restoration | Canonical rndr popup Tab policy exits/dismisses rather than reproducing a web focus trap; full arrow-key listbox navigation is not implemented |

History roles/content come from typed history entries and known stream boundaries,
not by splitting rendered `### You`/`### Medousa` headings. Heading-like user prose
cannot fabricate a second message. Pending text, recoverable failed admissions,
uncertain stream settlement, cancel behavior, generation fencing and observation
privacy checks are preserved. Normal UI does not parse an additional hidden
whole-transcript Markdown document.

## What belongs in the app versus rndr

**Already available in rndr; no framework fix needed:** Prompt Hug sizing and
wrapped-line cap, anchored popups, dismissal/focus scope, native clipboard,
Markdown rendering, single retained scrollport. The prototype's fixed composer
height and blank spacer were app layout choices, not rndr limitations.

**App work still needed:** external session lifecycle and bindings; persisted
behavior defaults/style/reasoning capability loading; approvals, attachments,
stickies, voice, skills, full chat navigation, tool timeline, jump-to-bottom,
link opening and code Copy. These must not be represented as
framework bugs or enabled pretend controls.

**Useful framework improvements to evaluate:**

1. A public inline/measurement-only Markdown document API would remove this
   adapter's knowledge of Markdown's 16-point inset. It currently measures the
   existing Markdown blocks into a Hug message, removes nested scroll behavior,
   and tests both narrow width and scroll growth. History virtualization is now
   app-owned around those views, not a framework change.
2. `AnchoredPopup` intentionally sizes to `max(anchor width, 240)` rather than
   honoring a separate preferred panel width. The app constrains its model
   trigger to 340 points; small footer triggers get 240-point panels. A preferred
   panel width would make Svelte's independently sized panels easier to match.
3. The bundled fallback did not render the dropdown triangle `▾` in the exported
   frame. The preview uses an ASCII `v` until a vector icon/font strategy lands.
   This is a visual parity gap, not a functioning native icon system claim.
4. The previously recorded Prompt chronological-submit and public-reset API
   requests still apply. Existing app-level conservative submission guarding
   and reset mitigation remain; no shared framework source was patched here.

## Evidence and reproduction

Run from the repository root (local rndr checkout required):

```sh
cargo test --manifest-path apps/rndr-home/Cargo.toml -j 2
cargo test --manifest-path apps/rndr-home/Cargo.toml --features observe -j 2
cargo clippy --manifest-path apps/rndr-home/Cargo.toml --all-targets --features observe -j 2 -- -D warnings
cargo run --manifest-path apps/rndr-home/Cargo.toml -j 2 -- --export /tmp/rndr-chat-parity
```

The observation-feature suite passed **42 tests** (4 library + 38 app/backend/
renderer), including real pointer-event model selection/outside dismissal,
search/exact selection with preserved draft, Escape/focus restoration, depth wire
request, unavailable runtime rejection, server-effective session mode mutation,
stale reply rejection, structured-role safety, stable streaming rows, scroll
following, narrow bubbles, six-line composer growth/shrink, and clipboard actions.
The backend uses real SDK calls against synthetic loopback HTTP/SSE fixtures.
No test contacts a configured daemon/provider or submits paid inference.

`docs/screenshots/parity/{empty,chat,narrow,draft,models,behavior,depth,runtime}.png`
are headless exports using synthetic catalog/transcript/draft content, not live
provider data or screenshots of the user's chat. `native-empty.png` and the
same-frame `native-empty.json` come from the actual offline desktop runner's
scoped observation surface. That native image proves rendering, not OS-compositor
confirmation or physical-device input/accessibility certification. The observation
launcher never loads credentials, and denies public pixels after a user draft,
structured history, or live transcript exists.

Strict app clippy and native build passed. Full monorepo engine/Tauri CI was not
run because production sources were unchanged. Live connected mode/provider
operation and physical keyboard/IME/clipboard behavior remain an owner smoke
test; do not equate synthetic fixture success with live inference certification.

## Memory/scaling follow-on — 2026-11-07

The UI pass above was committed as `9303d731`. The reported 1.5 GB process was
not reproduced, so this change addresses measured app inefficiencies, not a
proven cause of that observation. No shared rndr, engine, SDK or Svelte source
was changed.

- Removed `Chat.transcript`, its retained prefix, backend aggregate Markdown
  formatting, and duplicate per-segment body strings. Canonical history is one
  structured vector; pending prose lives in `live_messages`. Stream segment
  IDs/byte lengths locate in-place edits, preserving Unicode and interleaving.
- History state and structured messages arrive in one fenced reply rather than
  separate `History` / `Messages` events. Fatal/reconciliation failures retain
  live text; generation fencing and no-automatic-resubmit behavior remain.
- UI sync borrows canonical strings instead of cloning history and live text
  every update. Mounted assistant source is owned by `MarkdownView`, not a
  second conversation cache. Unchanged user prompts are not escaped/measured
  again every frame.
- Each historical row keeps a lightweight height placeholder. Only messages
  intersecting the viewport plus overscan retain Markdown/layout descendants;
  evicted subtrees are despawned, not hidden or cached indefinitely. Measured
  heights replace estimates, with offset correction for rows above the reader.
  Bottom following, reading while the tail streams, remount Copy, resize,
  role changes, truncation and cleanup have regression coverage.

### Matched before/after fixture

`examples/memory_fixture.rs` is a permanent offline-only harness: 1,000 assistant
messages, **1,062,890 source bytes** of repeated Markdown, 1120×820 window,
Parley, debug profile without symbols. Baseline used the exact conversation
renderer from `9303d731`, plus an adapter cloning the history vector as its
original UI did. Current uses the borrowed viewport renderer. Both are minimal
conversation windows, **not the full chat app or the earlier 1,030,945-byte
fixture**. Repeated text benefits from cache reuse.

| Measurement | Baseline | Current |
|---|---:|---:|
| Mounted Markdown views | 1,000 | 3 |
| Layout entities, including placeholders | 18,002 | 1,053 |
| Native physical footprint at ~8 seconds (`vmmap -summary`) | 159.7 MiB | 47.6 MiB |
| Native high-water footprint reported at that sample | 159.9 MiB | 138.4 MiB |
| Headless 12 updates including initial construction, one run | 7,944 ms | 175 ms |

Both native runners exited normally at 15 seconds. This is a scoped, single-run
comparison, not a general memory ceiling, renderer benchmark, full-chat startup
latency, or evidence that the original 1.5 GB incident is fixed. GPU startup can
still dominate peaks; the improvement is clearest in retained history work.
Local raw logs/summaries are in ignored `evidence/perf-{before,after}*`.
The temporary baseline example was removed; the production fixture remains.

Reproduce current counts/timing or a native memory sample:

```sh
cargo run --manifest-path apps/rndr-home/Cargo.toml -j 2 --example memory_fixture -- 1000
cargo build --manifest-path apps/rndr-home/Cargo.toml -j 2 --example memory_fixture
# With this checkout's configured target directory; adjust if yours differs:
../.cache/cargo-target/debug/examples/memory_fixture 1000 --native &
pid=$!
sleep 8
vmmap -summary "$pid"
wait "$pid"
```

Verification: **44 default / 45 observation-feature tests** passed (6 library +
38/39 app/backend/renderer), strict all-target/all-feature clippy, native build,
formatting and docs checks. Added stream Unicode/interleaving/empty-segment and
in-place allocation regression cases; 1,000-message tests repeatedly visit
remote scroll windows and assert bounded Markdown/layout counts, eviction,
Copy, tail updates while reading above, resize and total cleanup. Offline exports
and a fresh native empty-surface capture completed with normal shutdown; this
is not OS-compositor or physical scroll/IME/accessibility certification.

Remaining scaling limits: full canonical source and one small row entity per
message are still retained; each sync scans row metadata. Heights for unseen or
resized offscreen rows are approximate until visited, so the scrollbar extent
can refine. Selection survives streaming while mounted, but eviction drops
that message's local selection. One extraordinarily large message still mounts
as a whole. Paging daemon history, intra-message virtualization and a measured
text-anchor navigation API are separate work, not silently claimed here.

## Updated rndr main evaluation — 2026-11-08

The owner pulled the separate rndr checkout. Its local HEAD names `main` and
that branch ref is `e3c1fd66fefbf1645f7849bd8f5720dae2ee5ede`. The app's
existing absolute path dependencies already consume this source; no dependency
URL/pin switch, shared renderer edit, Git integration or fetch was needed here.
This records the installed revision, not independent proof of remote freshness.
The existing 44-test suite passed against it before further changes.

### Adopted now

- Core `Scrollbar` replaces the decorative `ScrollIndicator` for history.
  A separate fixed 24-point hit lane does not move with virtualized rows;
  pointer dragging preserves composer focus, and explicit Tab/focus enables
  Home/End/Page/arrow scrolling through `FormNavigation`'s installed handler.
  The track hides when there is no overflow; hiding does not change line wrap.
  The entire frame hides with empty history. The canonical outer `Scroll`
  remains the only transcript scrollport; no second scroll model is introduced.
- `rndr_paint/highlight` enables fenced-code token colors (and forwards the
  core feature). No custom highlighter or retained source copy. Message Copy
  remains exact raw Markdown, including code fences and language identifiers.
- Added full-app 1,000-message pointer-drag/keyboard-End coverage with draft,
  focus, bounded mounted-view count, tail streaming and empty-reload checks;
  added highlighted-code parsing plus source-Copy regression coverage.

### Next candidates, in priority order

| Public capability | App opportunity | Boundary / reason not to swap blindly |
|---|---|---|
| `rndr_ui::ListView::mount_variable` / `measure_height` / `VariableAnchor` | Replace our O(n) row scan/placeholders with keyed, viewport-bounded realization and integer item anchors | Needs persistent message IDs, measured Markdown heights, streaming/follow-bottom integration and remount tests. Host still supplies heights and canonical text; not automatic Markdown or remote-history paging. Do not use uniform `mount_virtual` for arbitrary-height chat |
| `rndr_ui` themes, controls and collection policies | Replace hand-styled model/menu rows with shared semantic controls and deliberate keyboard navigation | Namespace/input/theme migration; preserve exact model identity, disabled runtimes, daemon-owned mode and exclusive nested actions. Not a broad visual reskin in this pass |
| `mount_markdown_slot` | Trusted app-owned rich cards and controls within assistant documents | Replaces a whole parsed block, not an inline-document API. Host must validate intents, own state outside evicted views, and allocate nonreused view-local slot IDs. Markdown must never become executable UI/approval authority |
| `CloseGuard` and bounded starter preferences | Protect unsent drafts and persist selected app preferences | Guard alone saves nothing; requires explicit storage/privacy/session scope and failure/cancel/discard UI. Never serialize credentials, authority or unconfirmed admissions as successful state |
| Native file drop/dialog and optional `rndr_browser` | Attachment intake and embedded browsing when those app workflows are implemented | These do not supply daemon upload/attachment admission, URL policy, auth or download permission. Browser platform support is bounded; not a replacement for the chat renderer |
| Component capture / startup tracing | More targeted visual regression and startup diagnosis | Observation remains explicitly opted in and private content denied. Hidden-window asynchronous GPU startup is not evidence of lower memory or faster visible-window launch |

Correction to the earlier conversational scan: at this exact revision,
`prompt.rs` still has no public Prompt-specific reset/chronological submit API,
and `markdown.rs` has no public inline-document measurement constructor.
The existing conservative submit guard, synchronous field-reset mitigation,
Hug/no-inner-scroll adapter and 16-point Markdown inset therefore remain.
Host slots do not remove those limitations. The older framework requests above
are historical findings, not claims that every API is still absent elsewhere.

Verification commands are the same scoped test/clippy/build/export commands
above. New synthetic exports live under ignored `evidence/rndr-main-upgrade/`;
they do not replace the historical parity captures or contain live chat data.
No configured daemon/provider calls, inference, persistence migration, shared
rndr edits or automatic commits are part of this upgrade pass.

Final scoped verification: **46 default / 47 observation-feature tests** pass,
strict all-target/all-feature clippy, native binary build and synthetic export
complete. The same 1,000-message / 1,062,890-byte headless fixture mounts **3
Markdown views / 1,056 layout entities** (three more layout entities for the
scroll frame, reserved lane and track than the earlier 1,053 count). One
12-update run took 241 ms; this is a single headless timing, not a native
memory benchmark or a measured improvement over the older renderer revision.
Fresh physical scrollbar/IME/screen-reader validation remains unclaimed.

## Semantic styling migration — rndr main `4c9836ad` (2026-11-08)

Owner updated the separate renderer checkout again. This pass uses its local
main ref `4c9836ad2be670d649b023d6e7fe860808da6076`; no renderer files or Git
refs were changed. Baseline compatibility was 46 tests. Previous app memory,
scrollbar, highlighting, replay and submission protections are retained.

### Adopted

- Added the matching `rndr_ui` dependency and `UiPlugin` in place of installing
  `FormNavigation` separately. One canonical core input/focus/action FIFO remains.
- `presentation.rs` uses `UiTheme`, `UiStyle`, palette/typography tokens and the
  explicit `apply_theme_bridge`. Core chrome and Markdown use the same theme.
  Markdown's paragraph surface remains transparent; code/link accents stay
  themed. Token recipes own canvas, button, overlay and user-bubble colors;
  authored transparent quiet-control overrides retain rndr's precedence.
- The composer uses retained `Surface::Raised` with `UiId(1)` and a borrowed
  content slot. Repeated same-key mounts retain the Prompt and its editor state.
  Chrome geometry stays app-owned where intentional; this is not a wholesale
  migration of every core primitive or every typography override.
- Composer and transcript use the public 75-character reading-measure helper,
  bounded by available width, rather than independent 670/760-point caps.
- **Behavior → Window appearance** uses semantic `controls::Button` instances
  carrying opaque `HostIntent::Command` tokens. The existing queue is drained
  once; host validation translates only recognized commands to local appearance
  changes. Order/multiplicity are preserved, stale popup requests are discarded.
  Dark/light and high contrast are window-local, unsaved and never inference.
  Explicit launch flags are `--light` and `--high-contrast`; default stays dark.
- Popup content now uses Fill width. `AnchoredPopup` chooses its actual width
  from its anchor (at least 240 points, capped by viewport); a fixed 340-point
  content width caused horizontal overflow. A Parley-measured regression covers
  all popup kinds at 520/1120-point viewports.

### Deliberate non-migrations

- Public variable-height `ListView` is real, but is not an intrinsic Markdown
  measurer. It requires stable unique ItemIds and one positive height per item;
  rows recycle/destroy their subtrees. A safe chat port still needs stable daemon
  identities, intrinsic-height/resize invalidation, streaming view retention,
  bottom-follow policy and batched metadata updates. Per-row `measure_height`
  rebuilds prefix metadata; remounting all metadata every frame is not an idle
  performance upgrade. Keep the tested bounded app virtualizer for now.
- There is still no dedicated public Prompt-reset or atomic submission API in
  the inspected source. Preserve the temporary transaction-marker reset adapter,
  ambiguous Enter/IME-batch guard and admission/recovery/no-resubmit safeguards.
- Markdown host slots, structured editors, attachment intake and external
  runtimes are not auto-enabled by this styling migration. Host data/admission
  boundaries still apply. Native placeholder and scroll-track paint remain core
  chrome; high contrast is not a full accessibility certification.

### Verification

Final scoped gates: **50 default / 51 observation-feature tests**, strict
all-target/all-feature clippy, formatting, native binary builds, diff and strict
docs checks. New tests cover repeated retained-surface/theme updates with draft,
focus, popup, message identity and exact Copy preservation; reading-measure
resize; typed appearance FIFO multiplicity/no-send/stale generation handling;
and real-Parley popup width/overflow.

Synthetic GPU exports (nine fixtures per appearance, no live data) are under
ignored `apps/rndr-home/target/migration-4c9836ad/{dark,light,high-contrast}/`.
Reproduce with the scoped app `cargo run` command plus `--export DIRECTORY`,
optionally `--light` or `--high-contrast`. Visual inspection covers dark chat and
narrow layout, light chat/appearance controls, high-contrast controls and a
multiline draft. Physical pointer/trackpad, IME and screen-reader certification
remains unclaimed.

The updated standalone memory fixture installs the same UiPlugin/theme bridge.
For 1,000 messages / 1,062,890 source bytes, 12 headless updates mount **2 Markdown
views / 1,039 layout entities**. The narrower reading width changes wrapping and
viewport occupancy, so this is evidence of bounded mounting, not a matched RAM
or speed improvement. One run took 401 ms, not a performance benchmark. No
inference, shared renderer changes, persistence migration or commit was made.
