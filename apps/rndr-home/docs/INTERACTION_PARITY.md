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
| Transcript | One outer scrollport; right-aligned, literal user bubbles; transparent assistant Markdown; bounded reading lane | No virtualized long history or selection spanning separate messages |
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
link opening, code Copy, and virtualization. These must not be represented as
framework bugs or enabled pretend controls.

**Useful framework improvements to evaluate:**

1. A public inline/measurement-only Markdown document API would remove this
   adapter's knowledge of Markdown's 16-point inset. It currently measures the
   existing Markdown blocks into a Hug message, removes nested scroll behavior,
   and tests both narrow width and scroll growth. It is not virtualization.
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
