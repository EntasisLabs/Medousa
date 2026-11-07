# First rndr chat port — verification and review evidence

Date: 2026-11-07. Scope: isolated Forge undertaking
`work-5583a611afd630a6ff92f9f2e1fccaab`, branch
`worktree/rndr-home-first-native-chat-port`, baseline
`9bf30c01334790fb8c0b276e52a5a8fdfc470274`.

## Outcome and source boundary

Runnable native desktop preview in `apps/rndr-home`, using local rndr path
dependencies. Existing Tauri, engine runtime, SDK source, root Cargo workspace,
and root lockfile are unchanged. No shared rndr source changes. Framework HEAD:
`d7ed6d798b7680ee09ce6ee0ac15c9ae85bc1513` (v0.4.0); its checkout was clean after
verification. The observation harness disables Python bytecode caching; the
single cache file created by its first run was precisely removed, not a broad
cleanup of owner work.

Repository `AGENTS.md`, existing Tauri SDK/workshop transport, local credential
loader, session/transcript types, v3 chronological stream, model catalog, and
rndr desktop/stream/prompt/Markdown/observation APIs were inspected first.

No live provider request, paid inference, credential provisioning, credential
copy, token CLI argument, plaintext secret store, remote push, deployment,
signing, publication, or integration was performed. Live connected operation
with the owner's configured daemon/provider remains an owner smoke test; the
protocol verification below uses real SDK calls against synthetic loopback
fixtures. Offline launches never invoke the credential loader.

## Initial candidate verification (historical)

The following 15/14-test results describe candidate `a1fa6305`, not the
follow-on correction. They did **not** cover the prompt-loss or active-resume
races below. Current independent checks supersede these counts.

## Initial executed verification

All Cargo commands used `--manifest-path apps/rndr-home/Cargo.toml` and at most
`-j 2`; dev/test debug information and incremental output are disabled. Rust
`1.98.0`, local macOS runner. Initial free disk was ~17 GiB; the final bounded
builds did not require cleanup of existing build trees. The repository's inherited
Cargo target directory was the Forge cache (`../.cache/cargo-target`).

| Check | Result |
|---|---|
| Standalone compile / native `cargo build --features observe` | PASS |
| `cargo test --all-features` | PASS: **15 tests**, 0 failures |
| Default `cargo test` | PASS: **14 tests**, 0 failures |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo fmt -- --check` / staged diff whitespace check | PASS |
| `bash scripts/verify-docs.sh --strict` | PASS (`verify-docs: OK`) |
| Headless `--export .../docs/screenshots` | PASS: 1120×820 empty/chat, 520×760 narrow |
| Native `scripts/capture_native.py` on built executable | PASS: actual desktop runner, 1120×820 surface, normal timed exit, descriptor cleanup |
| Full engine/Tauri workspace/mobile CI | NOT RUN: bounded second-app slice; existing Tauri source unchanged |
| Live provider turn / OS compositor confirmation / physical IME and screen-reader certification | NOT CLAIMED |

The 15 tests cover:

- no-session/empty/overlapping send guards, explicit provider/model routing;
- real SDK create/history/admission, current strict authority identity contract;
- v3 media negotiation, dropped-stream replay, duplicate suppression, ordered
  text folding, terminal settlement, cancellation, and unauthorized admission;
- no automatic re-admission after errors; preserved prompt projection;
- native UI send-once, draft clear, multiline Shift+Enter, ordinary Enter (including idle IME-enabled fields),
  canonical Tab, resize/draft preservation, stale-generation rejection;
- startup auth failure and disconnected-worker spinner/draft reconciliation;
- ambiguous Enter input batches defer inference and retain the draft;
- opt-in offline observer denies the whole frame when a user draft is present.

Tests only open a test loopback listener; they do not use a daemon, real keys,
Keychain, or a provider. The test fixture is not an application chat service.

## Screenshots and visual review

Committed public synthetic evidence:

- [Native empty state](screenshots/native-empty.png), **1120×820**.
- [Matching native metadata](screenshots/native-empty.json): same-frame semantics,
  dimensions, scale, revision and render provenance, no observation capability.
- [Headless empty state](screenshots/empty.png), **1120×820**.
- [Headless formatted chat](screenshots/chat.png), **1120×820**.
- [Headless narrow chat](screenshots/narrow.png), **520×760**.

The native launcher is a real rndr window, not Exporter substituted as a native
capture. Its metadata deliberately says
`source = live-surface-copy-before-present`, `presented_confirmation = false`.
It proves the acquired native surface rendered/submitted by the actual runner;
it does **not** prove OS-compositor display, OS chrome, color management, or
physical-device presentation. The separate headless fixtures are explicitly
labeled; their assistant text is synthetic and says so.

The initial implementer's pixel-review claim was not independently established
by the sender: their image viewer returned metadata without visible pixels.
During this follow-on audit, actual pixels **were delivered and reviewed** for
the headless narrow fixture and a fresh corrected native empty capture, along
with the original human attachment reopened through chat history
(`usr:ses_012d:8e0599a3a70a44d492e763d8475fe95c`). This is visual inspection,
not a pixel-diff/parity certification. The fresh ignored capture is
`apps/rndr-home/evidence/audit-native.png` with matching JSON, frame 3,
1120×820, PNG SHA-256
`7d1c63a5841c2c00cde5afcdbede5377cd64a69c3feb2f66f31fb01afd0cdabe`.
It matches the committed native PNG bytes, but is from the corrected executable.
The real desktop runner exited normally and removed its observation descriptor.
No OS compositor screenshot or connected/paid inference surface was captured.

Compared to that reference: near-black rounded bordered shell, compact
Today/add strip, Today secondary header, centered empty question/underlined
continuation, ~670px rounded charcoal composer, warm text, muted controls,
light circular send, and a bottom status strip are present. In the narrow
fixture the composer shrinks to the viewport and the controls/text remain
contained. The transcript scroll viewport shows the lower/latest prose rather
than expanding the window. This is a coherent first slice, **not pixel-exact
parity**: native/tab chrome and preview navigation differ; the model picker is
a cycle button; unsupported attachment/mic/sticky/Behavior controls are plain
unavailable labels instead of fake active buttons. Height/spacing differ with
the smaller captured window and the empty composer.

The optional observer can only be started through `--observe-offline`, which
forces offline mode before worker creation. Normal builds/windows create no
listener. The audited empty frame alone is public; entering any draft text or
introducing transcript/connected state denies observation. The harness only
imports the existing rndr observation client and waits for normal 15-second
shutdown; it does not drive other apps or request OS permissions.

## Follow-on independent audit and correction

A fresh corrector audited all candidate source/tests, standalone manifest/lock,
capture script, screenshots/metadata and documentation against the original
first-slice scope. Only this app was changed. Relevant real daemon contracts
were rechecked: `daemon_interactive_turn.rs` stamps user and assistant entries
with `execution_ref(session, turn_id)` and waits for durable assistant persistence
before terminal publication; persistence failure can yield **fatal** settlement.
`daemon_handlers.rs::get_session_history` reads the runtime timeline separately
from active-ticket lookup. `TranscriptEntry.caused_by` is an execution identity,
not a text/timestamp heuristic or a shared per-segment transcript entry ID.

### Findings and regression reproductions

1. **Prompt loss (blocking):** send a unique prompt → rejected/unknown admission
   → Reload empty history previously erased both composer and optimistic text.
   Recovery now lives independently of `Chat`, **in memory only**. The real UI
   send/failure/reload sequence covers both 401 and pre-persistence network-error
   replies, empty/newer multiline drafts, visibly retained recovery text, disabled
   Restore when a newer draft exists, lossless explicit Restore and zero implicit
   submissions. Confirmed admission removes that request's recovery copy without
   touching newer composer edits. The SDK has a separate real loopback 401 fixture.
2. **Stale history on settlement (blocking):** active-before → history →
   active-after is retried up to three times if the execution changes. The
   completion-between-reads fixture requires refreshed user **and** assistant
   prose exactly once. Exhausted churn yields an explicit uncertain failure,
   not stale history presented as settled.
3. **Replay overlap (blocking):** active execution's persisted assistant entries
   are omitted by complete `caused_by` identity before full v3 replay; user and
   other-execution entries remain canonical. Fixtures exercise active reattach,
   replayed duplicate sequence, terminal replacement, and user persistence both
   before and after initial loading. They assert neither missing nor duplicated
   user/assistant prose. A full real UI Reload → backend `serve` → SDK active
   reattach → feed → terminal → canonical history fixture verifies the actual
   integration path, not just the projection helper.
4. **Settlement gating:** Send stays blocked until terminal history reconciliation
   finishes. Ticket cleanup is retried at most three history operations (25/50 ms
   delays; each operation itself has the three-bracket bound above). A persistent
   active ticket/handoff requires explicit Reload instead of pretending a stream
   was reattached. Cancellation's terminal status remains visible after history
   replacement. Fatal persistence failure preserves live in-memory prose and
   reports incomplete durability instead of replacing it with missing history.
5. **Idle Cancel:** the generic enabled-button fallback had enabled hidden idle
   Stop. Its enabled state and action guard now require a busy/uncertain session;
   a cancel request never fakes terminal completion. UI and real SDK route tests
   cover cancellation, including continued busy state until settlement.

No additional first-slice blocker was found in model catalog/request routing,
read-only local credential transport, generation fencing, multiline input or
unsupported-control behavior. Added catalog-wire and shared all-stage routing
assertions. Existing ambiguous Enter-batch, Shift+Enter, idle IME-enabled Enter,
Tab, stale reply, disconnected-worker and observer-privacy checks still pass.

### Current bounded checks

Disk checked before builds/tests/lint: ~16 GiB available; no cleanup performed.
All Cargo invocations are standalone, offline and `-j 2`.

| Independent correction check | Result |
|---|---|
| `cargo test --all-features --offline -j 2` | PASS: **25** (4 library + 21 application) |
| Default `cargo test --offline -j 2` | PASS: **24** (observer test excluded) |
| Native default and all-feature `cargo build --offline -j 2` | PASS |
| `cargo clippy --all-targets --all-features --offline -j 2 -- -D warnings` | PASS |
| `cargo fmt -- --check`, `git diff --check` | PASS |
| `bash scripts/verify-docs.sh --strict` | PASS |
| Corrected native `capture_native.py` | PASS: submitted surface, timed normal exit, descriptor cleanup |
| Actual pixel review | Human reference, corrected native empty and headless narrow; **not OS compositor**, connected UI, or live inference |

These are app-side **non-atomic** reconciliation reads, not a snapshot guarantee.
Another client can act after the final observation; an ABA transition entirely
between observations cannot be ruled out without a stronger daemon contract.
No admission retry was introduced. Recovery copies intentionally do not survive
app exit. Fatal unsaved streamed prose is retained until explicit history reload,
not durably rescued. Still-active handoffs require Reload; no approval/secret
fulfillment or broader engine change is implied.

## Residual gaps and next review

The [app README](../README.md) contains exact launch prerequisites, read-only
local credential boundary, supported workflow, all parity limits, and reproducible
framework cases. Important ones:

1. rndr `Prompt` folds a full key batch before emitting `Submitted`; `hi`, Enter,
   `!` in one batch is not a chronological owner-submit callback. The app refuses
   that ambiguous request, visibly preserving the draft; ordinary Enter/Send
   works. A Prompt-aware synchronous owner callback is the framework request.
2. A supported Prompt-specific reset API is missing. The app temporarily uses
   `FieldTransactions` only during synchronous draft reset, never as the
   persistent multiline input policy. Multiline/Tab/clear tests pass.
3. SDK reconnect/backoff attempts are internally handled; only exhausted
   failure is exposed to the app. There is no transient retry/heartbeat UI yet.
4. Remote pairing, full chat browser/tabs, attachments/voice/stickies, Behavior
   settings, rich tool/artifact surfaces, approval and secret fulfillment,
   packaging/mobile, and rich model search are out of this port.

Sender retains review and integration responsibility. Review the source and
screenshots, then smoke the connected app against the chosen local workshop.
A real inference smoke should be an explicit owner action, not an unattended
verification step. No framework fixes were attempted in the shared rndr repo.
