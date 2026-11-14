# Medousa — rndr desktop preview

> **Current desktop slice:** [18-destination matrix](docs/CONTENT_SURFACES.md)
> and [Notes checks / pixel evidence](docs/content-evidence/REPORT.md).
> Notes now has a bounded first-page summary browser: title, inert relative path,
> modified UTC, local filter, details/Back and explicit refresh. No bodies, tags,
> pagination, root switching or content writes. This extends the integrated shell,
> Settings and Activity; it is not full Library/Notes completion.
> Twelve existing read models have filter/list/detail/refresh; the nine Settings
> routes remain read-only/local-preview, not editable production panels.
> No real workshop was contacted. No production-readiness claim.
> [Previous increment report](docs/DESKTOP_IMPLEMENTATION.md) is preserved.
>
> **Send, Restore, New, Recent and Reload remain disabled.** The legacy chat
> behavior described below does not override the current native-input gate.
> Harmless settings/navigation and local filtering remain enabled independently.
> Credentials, installs, permissions, pairing, live config and destructive actions
> stay in the existing Medousa app. Production Home is unchanged.
>
> Generate clearly labeled synthetic non-chat GPU evidence (no network):
> `cargo run --manifest-path apps/rndr-home/Cargo.toml -j 2 -- --export-desktop ./desktop-renders`
> Notes-only synthetic matrix: `cargo run --manifest-path apps/rndr-home/Cargo.toml -j 2 -- --export-notes ./notes-renders`
> See the updated [user guide](../../docs/guides/rndr-native-preview.md).
> [Prior shell/Activity matrix](docs/SHELL_ACTIVITY_PARITY.md) and
> [prior evidence](docs/shell-activity-evidence/REPORT.md) are preserved.

> **Prior qualification (preserved, before this non-chat increment):** [rndr 0.6.0 at `44f5dc7c`](docs/QUALIFICATION_0_6_0.md)
> passes 54 default / 55 observe-feature tests, strict scoped clippy and native
> builds. Includes fresh synthetic headless and native submitted-surface evidence,
> explicit device/live/compositor gaps, and later team-boundary recommendations.
> Older 0.4.0 verification is historical, not current evidence.

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

# Normal connected launch: use the same configured local workshop as Tauri.
cargo run --manifest-path apps/rndr-home/Cargo.toml -j 2 --

# Optional existing session; connection discovery is unchanged.
cargo run --manifest-path apps/rndr-home/Cargo.toml -j 2 -- --session 'YOUR_EXISTING_SESSION_ID'

# Advanced attach-only override: matching origin AND engine data directory.
# These are paths/origins, NOT tokens. Do not put secrets on the command line.
cargo run --manifest-path apps/rndr-home/Cargo.toml -j 2 -- \
  --daemon 'http://127.0.0.1:7419' --data-dir '/absolute/path/to/workshop/data'
```

### Connection and bootstrap policy

Tauri and RNDR share the non-renderer `medousa-desktop-host` path, registry,
engine-manager and defaults-schema code. Connected startup loads Tauri's first
readable env overlay (`STASIS_ENV_FILE`, config-dir `.env`, data-dir `.env`),
without replacing process environment variables. The canonical data root is
`MEDOUSA_DATA_DIR` (nonempty), then the platform-default `medousa/data_dir`
bootstrap redirect, then the platform local-data directory joined with `medousa`.
`MEDOUSA_CONFIG_DIR` selects the config overlay directory. As in Tauri, the data
root is resolved/cached while selecting the overlay, before applying its entries.
An overlay cannot retrospectively relocate that root.

`workshops.json` selects the active local workshop. Its bind determines the URL;
public wildcard binds map to loopback for the client. Personal uses the canonical
root (ignoring a stale personal `dataDir`); other local workshops use their own
configured `dataDir`. Missing local fields are backfilled by the shared policy.
A missing registry means personal at `127.0.0.1:7419` — **not 9777**. Environment
`MEDOUSA_DAEMON_URL` / legacy `STASIS_DAEMON_URL` and `home_daemon_url.txt` are
Tauri's legacy seed URL, not overrides of a local workshop's effective bind.
RNDR does not scan for installations, migrate legacy pairing secrets or write a
registry. Malformed/unreadable registries fail with a non-secret diagnostic;
remote/paired active workshops are unsupported, with no credential fallback.

Configured local startup uses the same core-only engine ensure policy as Tauri:
reuse a healthy daemon; wait for an occupied bind to become healthy; otherwise
stop the managed unready engine and bootstrap via the existing binary resolution,
command/env, runtime PID/log paths and bounded readiness wait. Optional local brain
remains opt-in, never inferred from provider. Inherited optional managed-Laya
bootstrap behavior is unchanged. RNDR does not download or rebuild an engine;
if its engine binary is unavailable, install/configure through the existing app.
Daemon binary preference is `MEDOUSA_MEDOUSA_DAEMON_BIN`, executable sibling,
then PATH, exactly as the reference. There is no speculative installation search
or Cargo/build fallback. Optional component binaries retain their existing policy.

`--data-dir` explicitly selects an engine root instead of importing the registry.
`--daemon` overrides the origin and is **attach-only** (no process stop/start),
while retaining the selected credential root; pair both flags for another engine.
Readiness is checked before authenticated SDK clients are created. Origins must
be loopback HTTP(S), with no userinfo, path, query or fragment. Authenticated and
health transports bypass proxies and refuse redirects. Existing first-party local
credentials are loaded only for the selected engine root; RNDR never provisions,
copies or rotates them. Normal daemon bootstrap may initialize its own auth store,
as in Tauri. Missing auth is a distinct, non-secret connection failure. macOS
Keychain may require normal OS consent. Provider setup remains in the existing app;
there is no token entry, credential-copy step, remote selection or auth bypass.

`--offline`, every export mode, `--observe-offline`, and `--help` bypass overlay,
registry, credential and engine discovery entirely: **no credential, network or
bootstrap effects**. Synthetic exports still write their explicitly requested
output files. The five sensitive chat controls remain denied.

## Legacy chat behavior (input-sensitive controls currently gated)

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
- Click **Select model** for a searchable, provider-grouped menu of read-only
  `/v1/models/catalog` choices. Nothing is selected merely by opening it; choose
  an exact provider/model before sending. Escape, outside click, or the close
  button dismisses without changing your draft. Catalog entries may still require
  provider setup in the existing app.
- **Mode** loads the daemon's mode registry and effective session mode. Selecting
  an available mode uses the same session-scoped SDK mutation as the Svelte app;
  the display waits for the server's effective value (a task override can win).
  It does not grant project setup permission. Unavailable modes remain disabled.
- **Behavior → Response depth** selects Concise, Standard, or Deep for the next
  turn in this window. This preview deliberately does **not** persist shared
  workshop defaults like the Tauri app. Style, reasoning, and narration are
  visibly marked not ported. **Runtime** shows Medousa selected and external
  agents disabled: their ACP/provider-conversation session flows are not wired.
- The composer hugs its draft and wrapped text, caps at six visual lines with
  internal scrolling, and shrinks again after clearing. The transcript uses
  right-aligned literal user bubbles and transparent assistant Markdown, with
  one scrollport. Scrolling up unpins automatic following; returning to the
  bottom resumes it. **Copy** copies an assistant message as Markdown using the
  native clipboard. Cross-message selection and per-code-block Copy are not
  ported; Markdown link opening is not wired yet.
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
