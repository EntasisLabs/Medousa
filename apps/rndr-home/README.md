# Medousa — rndr native chat preview

A **second desktop app**, not a replacement for `apps/medousa-home`. Native
rndr ECS/layout/input/Parley/AccessKit/Markdown rendering; no Tauri, browser,
webview, JavaScript server, renderer fork, or alternate inference service.
This is a developer preview, not the home-first production installer.

## Local dependency and launch

Requirements: Rust/Cargo, macOS development tools, and the owner-managed rndr
checkout at `/Users/theelevators/rndr`. The Cargo path dependencies intentionally
reference that checkout; adjust the four `rndr_*` paths in `Cargo.toml` if yours
is elsewhere. No rndr source is copied or modified. The tested framework HEAD
was `d7ed6d798b7680ee09ce6ee0ac15c9ae85bc1513` (v0.4.0).

This package has its own workspace and lockfile, like a desktop shell. Always
use its manifest explicitly; it is not added to the engine's default build.
The existing root Cargo configuration may choose a Forge cache target directory.
Check disk before building. App profiles disable debug information/incremental
output; two build jobs are enough.

From the Medousa repository root:

```sh
# Safe UI-only launch: no credential loader, daemon traffic, or inference.
cargo run --manifest-path apps/rndr-home/Cargo.toml -j 2 -- --offline

# Use the origin and data directory of the SAME existing local workshop.
# These are paths/origins, NOT tokens. Do not put secrets on the command line.
export MEDOUSA_DAEMON_ORIGIN='http://127.0.0.1:9777' # replace with actual origin
export MEDOUSA_DATA_DIR='/absolute/path/to/that/workshop/data'
cargo run --manifest-path apps/rndr-home/Cargo.toml -j 2 -- \
  --daemon "$MEDOUSA_DAEMON_ORIGIN" --data-dir "$MEDOUSA_DATA_DIR"

# Open a specific existing chat, or reload/follow its active turn.
cargo run --manifest-path apps/rndr-home/Cargo.toml -j 2 -- \
  --daemon "$MEDOUSA_DAEMON_ORIGIN" --data-dir "$MEDOUSA_DATA_DIR" \
  --session 'YOUR_EXISTING_SESSION_ID'
```

Start/configure the daemon using the existing Medousa app/workshop. The port
above is an example/default for this preview, **not discovery**. The daemon must
serve the current SDK session/interactive routes and v3 turn SSE contract, with
its existing first-party local credential available for that exact data
directory. Configure provider credentials in the existing app. This preview
neither starts a daemon nor provisions/rotates credentials. macOS Keychain may
require the normal OS consent for the existing first-party credential loader.
There is no token input, credential-copy step, plaintext secret store, remote
workshop selection, or auth bypass. A data directory is required for a connected
launch; offline/export launches never load it.

## Use

- **+** creates a real single-user session; it does not run inference.
- **Continue where we left off** opens the most recent daemon session. There is
  no full chat browser yet; use `--session` for a specific older chat.
- **Today / Reload history** reloads the current session. Its full ID appears
  in the bottom status strip. An active turn reattaches using its daemon-issued
  stream URL and replays v3 events, without submitting another turn. History is
  bracketed by active-turn reads (up to three attempts), not an atomic snapshot.
  The active execution's persisted assistant entries are excluded by `caused_by`
  identity while its complete replay owns that prose; persisted user entries stay.
  A terminal stream triggers canonical history reconciliation before another send.
  Ticket cleanup gets at most three read attempts (25/50 ms delays); persistent
  activity/handoffs or read failure remain visibly uncertain and require Reload.
  A newly persisted user prompt may appear only after terminal reconciliation.
- Click **Select model** to cycle the read-only `/v1/models/catalog` choices.
  Provider and model are displayed together. Nothing is preselected; a selection
  is required. This intentionally small picker is not the production search UI.
  Catalog entries may require provider setup in the existing app.
- Compose with native text editing, clipboard, IME, selection, and Tab traversal.
  **Enter sends; Shift+Enter inserts a newline.** Send ↑ also sends. Sending can
  incur inference charges. No prompt is sent automatically on launch/resume.
- Ambiguous Enter batches (later keys in the same batch, or IME composition)
  are explicitly deferred, preserving the draft; press Send or submit again
  after composition finishes. See the framework repros below.
- Streamed text uses native Markdown (headings, emphasis, lists, inline/fenced
  code, etc.). V3 segment order/sequence IDs are preserved; aggregate terminal
  text does not overwrite the chronological projection.
- **Stop** requests daemon cancellation, not local pretend completion. The
  daemon's terminal event settles the UI. After stream/admission failure the
  outcome is unknown: sending is blocked until **Reload history** reconciles it.
  There is never an automatic admission retry. Stop is also available in an
  unknown state, but disabled for an idle settled chat. Reload is explicit,
  not a second inference request. On fatal settlement, history may be incomplete:
  live in-memory text is retained instead of silently replaced; copy/inspect it
  before explicitly reloading.
- An unconfirmed prompt is retained **in memory only**, separately from history.
  After admission failure/unknown outcome it remains visibly labeled **Recoverable
  prompt**, even after Reload or session navigation. **Restore last unconfirmed
  prompt** moves the exact text into an **empty** composer; it never overwrites
  newer edits or submits. Check history before manually sending it again. Confirmed
  admission retires its recovery copy. Closing the app loses these memory-only
  copies; there is no draft/secret/plaintext persistence or automatic retry.
- Permission, budget, and secret requests direct you to the existing Medousa
  app. This preview cannot approve/fulfill them. Never paste secrets into chat.

## Architecture / audited reuse

`src/backend.rs` uses `medousa-sdk` / `medousa-types` directly:

- `sessions().create/list/history/active_turn/cancel_active_turn`;
- `interactive().start_turn/stream_reconnecting_v3` (bounded reconnect with
  replay and duplicate suppression from the existing SDK);
- `http().get("/v1/models/catalog")` for model/provider choices;
- `medousa-local-credential::load_home_local_secret`, the same first-party local
  auth boundary used by Tauri's `workshop_transport.rs`.

The reqwest client is loopback-only, has no redirect/proxy forwarding, marks its
Authorization header sensitive, and refuses absolute/foreign stream URLs. The
worker never returns raw server error bodies to the UI. No secret values are
written to disk or included in application diagnostics. UI request generations
reject stale session/stream replies; `src/lib.rs` owns only a visible projection,
not session persistence or execution authority. Uncertain send outcomes require
history reconciliation. Stage routing uses the existing default matrix for the
explicit provider/model; this does not grant code-project setup authority or
add permission-approval routes.

The Tauri audit covered its SDK wrapper, workshop transport/local auth,
chronological stream/session contracts, and model catalog integration. Tauri's
remote pairing, credential UI, broad settings/studio surfaces, sidecar lifecycle,
and frontend stores are intentionally **not** duplicated in this app.

## Focused verification

```sh
cargo fmt --manifest-path apps/rndr-home/Cargo.toml -- --check
cargo test --manifest-path apps/rndr-home/Cargo.toml -j 2 --all-features
cargo clippy --manifest-path apps/rndr-home/Cargo.toml -j 2 \
  --all-targets --all-features -- -D warnings
```

The tests use test-only loopback HTTP fixtures, not a daemon/provider, keychain,
or inference API. They exercise create/history, strict response identity, turn
admission, v3 Accept/replay/dedup/terminal handling, cancellation, unauthorized
admission, state/error guards, UI send-once/draft reset/resize/Tab/multiline,
stale responses, startup auth failure, and ambiguous queued submissions. Follow-on
regressions cover failed admission → newer edits → Reload → explicit restore,
confirmed admission, active reattachment/full replay, completion between reads,
bounded activity/ticket cleanup, fatal unsaved prose, catalog wire shape and all
stage-routing targets. UI tests run the real rndr app state/input/update systems
with synthetic worker replies; backend fixtures use the real SDK over loopback.
See [verification evidence](docs/VERIFICATION.md) for exact executed outcomes.
Full engine/Tauri CI is not a substitute for this suite, nor claimed here.

### Reproduce screenshots without inference

```sh
# Headless GPU outputs: empty.png, chat.png, narrow.png. Chat text is synthetic.
cargo run --manifest-path apps/rndr-home/Cargo.toml -j 2 -- \
  --export apps/rndr-home/evidence

# Actual native surface capture, ONLY an audited offline empty fixture.
# The feature is optional; normal builds create no observation listener.
cargo build --manifest-path apps/rndr-home/Cargo.toml -j 2 --features observe
# Set BINARY to the built executable (Cargo prints its target location).
python3 apps/rndr-home/scripts/capture_native.py "$BINARY" \
  /Users/theelevators/rndr apps/rndr-home/evidence/native-empty
```

The native harness uses rndr's existing read-only observation client, with
bytecode caching disabled so the shared checkout stays read-only. Outputs must
not already exist (no-clobber). It launches `--observe-offline`, captures a fresh
native surface, waits for normal 15-second exit, and checks capability cleanup.
It never inspects other applications or real chats. Capture is a submitted
surface copy, **not compositor/display confirmation**; headless exports are
separately labeled and never presented as live-window screenshots.

## Parity and framework gaps

Not implemented: remote workshops/pairing, embedded daemon startup, full chat
browser/tabs, attachments, mic/voice, stickies, Behavior/persona editing,
permission/budget/credential fulfillment, tool cards/artifacts/browser surfaces,
all studio surfaces, per-chat local drafts, rich model search, packaging/signing,
and mobile support. Unsupported controls are text labels, not enabled fake
buttons. Errors and paused approval requests remain visible in the status strip.

Known framework gaps, with reproducible cases (no shared framework edits):

1. **Multiline Prompt lacks a chronological owner-submit boundary.** With a
   focused `Prompt + TextField`, draft `hi`, queue Enter then `!` in one Input
   batch. `Submitted.text` becomes `hi!`, not the text at Enter. Adding persistent
   `FieldTransactions` instead routes through the single-line dispatcher and
   breaks Prompt's Shift+Enter behavior. Needed framework fix: a Prompt-aware
   per-key submit callback/reconciliation seam preserving multiline/IME input.
   App mitigation: guard the original input batch, refuse ambiguous inference,
   retain draft, show an explicit notice. The UI regression test reproduces it.
2. **No public Prompt-specific draft reset.** `reset_field_draft` requires
   `FieldTransactions`; calling it on an ordinary Prompt returns false. Needed
   API: reset Prompt text/caret/undo/IME/scroll without changing its input policy.
   App mitigation: temporarily install that marker only during the synchronous
   reset, then remove it and reset Prompt scroll. Multiline/clear/Tab tests pass.
3. **SDK (not rndr): transient reconnect attempts have no UI notification hook.**
   Drop an SSE connection: replay/dedup/retry is handled by the SDK and exhausted
   failure is visible, but individual backoff attempts are not shown. A stream
   inactivity watchdog/SDK callback would improve Connection status fidelity.

These do not prevent launching or clickable/native ordinary Enter chat sending.
Physical IME/device-DPI/accessibility certification, all platform runners, and
production live inference are not claimed. No live paid inference was used for
verification. See the evidence report for the visual-inspection boundary.
