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

## Executed verification

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

Actual generated image pixels were opened with the image viewer, including the
native empty state and the narrow formatted transcript. Visual review was not
inferred from a file/capture receipt. The original reference attachment could be
located by history (`usr:ses_012d:8e0599a3a70a44d492e763d8475fe95c`), but no stable
on-disk export was returned; the detailed reference supplied in the assignment
was the implementation baseline rather than a pixel-diff input.

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
