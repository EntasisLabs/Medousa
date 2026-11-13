# Non-chat desktop preview implementation and evidence

## Review boundary

- Exact starting baseline: `a42c5236cd2239864d27aa9c151c3acd3e8191a0`.
- Isolated undertaking: `work-a81bc97abbb37c28cbe77b9cb4a8e0bd`.
- Implementation revision and evidence/source hashes are recorded in
  [`desktop-evidence/seal.json`](desktop-evidence/seal.json). Final evidence commit
  is reported in the sender handback; a commit cannot embed its own final hash.
- Reviewable **preview implementation**, not approval, integration, deployment,
  push/publishing, production readiness or physical native-input qualification.
- Production `apps/medousa-home`, framework sources/dependency pins, SDK/engine
  services and prior committed evidence are unchanged.

## Delivered working coverage

The authoritative reference is the existing production app. See the complete
[source destination / status / remaining-view matrix](DESKTOP_PARITY.md).

- Shared source-derived app rail, active destination, title/chrome and truth footer.
  Runtime and Messaging are not invented top-level silos. Native pane/tab/notch
  and dynamic environment rail parity are explicitly deferred, not claimed complete.
- All **nine** real Settings destinations, in production order, grouped app/host
  sidebar at wide sizes and a complete narrow section pager. Separate scrollports
  keep the last section reachable. Narrow header secondary copy is hidden to avoid
  title clipping; compact rail labels remain named semantically.
- Working window-local light/dark/high-contrast preferences, never persisted to
  production or consequential configuration.
- **Ten** existing real administrative models wired for read-only service access:
  workshop health/authority/version, cached models/providers, external runtimes,
  agent modes, MCP gateway/servers, local engine, schedules, shared mode, worker
  capacity and hosted review provider discovery.
- Search/filter, list, detail, Back and refresh. Explicit offline/not loaded,
  loading, permission denied, disconnected worker, malformed response, unavailable,
  empty and no-match states. Per-resource generations reject stale replies;
  pending refreshes deduplicate; old results are hidden until read settlement.
- Effectful/unsupported operations are isolated explanations, not enabled pretend
  controls. No raw config editor, credential input, installation, permission grant,
  live daemon control, destructive admin operation or paid inference.
- Chat remains mounted. Existing transcript virtualization, history/scroll,
  explicit Jump to latest, session and draft behavior are retained. **Send,
  Restore, New, Recent and Reload remain exactly gated.** Native IME work was not
  reopened and is not a blanket blocker on the implemented non-chat views.

## Architecture and changed source responsibilities

| File | Responsibility |
|---|---|
| `src/admin_model.rs` | Reference section metadata, routes, sanitized records, read/error state and per-resource generation fencing |
| `src/admin_backend.rs` | Independent read worker using existing `backend::client` authorization boundary; ten bounded GET projections; generic sanitized error classification |
| `src/desktop.rs` | Shared rail/chrome, responsive route/view lifecycle, focus and read dispatch; retained chat subtree |
| `src/settings_views.rs` | Nine source-derived settings pages and grouped/paged settings navigation; effectful-operation boundaries |
| `src/admin_views.rs` | Shared truthful searchable list/detail/refresh presentation |
| `src/desktop_fixtures.rs` | Explicit offline synthetic GPU evidence only; never a live-data fallback |
| `src/admin_backend_tests.rs` | Loopback fixture contracts for all ten reads, error classification, malformed/empty distinction, omitted-secret/diagnostic fields |
| `src/desktop_tests.rs` | Routing while busy/unqualified, local-only preferences/filtering, stale data, errors, details, session/draft/scroll retention, disconnected worker, observation privacy |
| `src/desktop_layout_tests.rs` | Parley bounds across both widths and all supported themes, title clipping regression, sidebar-scroll and narrow-pager reachability |
| `src/main.rs` | Resource/worker registration and `--export-desktop` offline entry |
| `src/controller.rs`, `src/shell.rs` | Small shared-shell hooks; chat reading width subtracts actual rail; status copy names the exact five gated actions |
| `src/fixtures.rs` | Observation Deny additionally covers admin transports, reads, non-chat routes and filter input |
| `src/ui_tests.rs`, `src/qualification_tests.rs` | Existing geometry assertions now account for rail/admin scrollports, without weakening transcript behavior or input gates |
| README, guide and docs indexes | Current coverage, safe launch, explicit restrictions, matrix and reproduction instructions |

No generic effectful admin command channel exists. Worker requests accept only a
closed Resource enum plus serial, never arbitrary URLs, commands or JSON writes.
Each service read has a 12-second deadline when serviced. The worker services
requests serially; rapid visits to many uncached pages can queue behind earlier
reads. Lists display at most 500 entries and labels are bounded plain text. Full
prompts, execution payloads, commands, auth detail, gateway URLs, raw errors and
secret/config fields are not projected. Account auth status is allowlisted.

## Verification and exact reproduction

Run from this isolated checkout, with two build jobs and a local disposable target:

```sh
sh apps/rndr-home/docs/desktop-evidence/reproduce.sh
```

The script saves exact commands, complete output and exit status to
[`desktop-evidence/results.tsv`](desktop-evidence/results.tsv), with per-check
`.command.txt` / `.log` files. It records compiler/tool versions and disk usage.
It does **not** connect to a real daemon or load user credentials. Wire tests bind
only ephemeral local fixture sockets and reject any non-GET/wrong-path request.

Canonical checks:

| Check | Outcome |
|---|---|
| `cargo fmt --check` (preview manifest) | Pass |
| Default tests | 6 library + 62 application tests pass |
| Observation-feature tests | 6 library + 70 application tests pass |
| Strict scoped clippy, all targets, default | Pass, `-D warnings` |
| Strict scoped clippy, all targets, observe | Pass, `-D warnings` |
| Preview build, default / observe | Both pass |
| Offline desktop GPU export | 108 PNGs: 18 cases × 2 sizes × 3 themes |
| Repository docs verifier `--strict` | Pass for this increment; prior evidence/remarks remain preserved |

The root Cargo manifest can emit an existing `redundant_homepage` metadata warning;
strict Rust clippy still passes. Workspace-wide engine/Svelte builds and full CI
were not run: no engine/SDK/production Home changes were made, and this is an
isolated preview with a bounded disk budget. The app's profiles already disable
incremental/debug output. Target usage is recorded in `desktop-evidence/disk.txt`;
the disposable build directory is not part of the implementation/evidence commits.

Regression coverage includes all previous chat and history tests, the 1,000-message
fixture, virtualization, scrollbar dragging, explicit latest behavior, native-input
gates and default/fixture-only observation privacy. The reading-measure tests now
subtract rail width instead of assuming the entire viewport belongs to chat.
Admin scrollport assertions explicitly retain the transcript's single scrollport.

## Render evidence (not native-window observation)

Reproduce separately:

```sh
cargo run --manifest-path apps/rndr-home/Cargo.toml --target-dir .rndr-target -j 2 -- \
  --export-desktop apps/rndr-home/docs/desktop-evidence/renders
```

All 108 actual GPU PNGs are under [`desktop-evidence/renders`](desktop-evidence/renders/).
Sizes: **1120×820 wide**, **480×760 narrow**. Palettes: dark, light and dark/high
contrast. Every Settings section is rendered. Further cases cover runtime list,
detail, empty, loading, permission, unavailable, disconnected MCP, model catalog
and schedule detail. Fixture labels persist through frame updates.

Each image visibly says **FIXTURE** and **SYNTHETIC GPU RENDER / NO NETWORK / NOT
NATIVE WINDOW / INPUT PROOF**. Export forces offline mode, asserts both live command
senders absent, and uses fixture-only sanitized records. No default/live UI falls
back to fixture data. These images do not demonstrate live service connectivity,
physical typing, IME, native accessibility or compositor/device behavior.

Final inspection notes, exact inspected paths and image hashes are stored in
`desktop-evidence/visual-inspection.md` and the seal manifest. An earlier inspection
found missing persistent fixture labels, sidebar reachability and a narrow header
collision; these were fixed and regression-tested before final evidence export.

## Live backend and parity limitations

No actual user daemon/provider/account was contacted in these checks. Live read
availability depends on the user-configured loopback daemon, same data directory,
existing credentials, service versions and permissions. A configured sender is
never presented as proof of a connected workshop. Health is a responder snapshot,
not proof of all subsystems. Runtime availability is not auth, auth unknown is not
signed out, a catalog row is not working inference, and review provider discovery
is not a linked review account.

The current SDK erases HTTP status into error text. The preview classifies 401/403
as permission errors and otherwise shows a generic unavailable state without raw
response bodies; a future structured SDK error improvement is not implemented here.
Package inventory relies on production's native Tauri host package bridge and is
honestly unavailable in this port. No package or model status is fabricated.

The broader product is not fully ported: actual agent/activity rosters, scripts,
flows, project/review browser, editors, identity, context, channels, pairing and
native tabs/panes remain explicitly deferred. This pass completes a coherent
shell/settings/connection/read-only-admin increment; the matrix lists concrete
follow-on views rather than hiding them behind an IME blanket hold.

## Compact final human verification checklist

1. Launch `--offline`; at wide/narrow sizes open all nine sections using sidebar,
   scrolling and Previous/Next. Check text, hit targets, focus/Tab, wheel/trackpad,
   display scaling and titlebar behavior on the actual device.
2. Change light/dark/high contrast. Filter an admin list with physical keyboard,
   IME, paste and Unicode. Verify edits remain window-local; close/reopen resets
   local preferences. Do not treat this as qualification of chat input safety.
3. Using the already configured local daemon/data directory, read health and verify
   authority/version against the existing app. Refresh runtimes/models/MCP/local
   engine/schedules/sharing/capacity/review-provider pages; confirm actual values,
   read-only behavior, permission/unavailable/empty states and explicit snapshot
   semantics. Do not enter or copy secrets and do not change live settings.
4. Return to Chat after browsing Settings: session, unsent draft and away-scroll
   position remain; Jump to latest remains explicit. Confirm Send/Restore/New/
   Recent/Reload are still disabled without native-input proof.
5. Check screen-reader/focus announcements and physical IME/device behavior.
   Report gaps for the appropriate app/rndr follow-on; do not approve production
   rollout or enable input-sensitive actions on the strength of synthetic images.

Independent review and native integration remain the sender's responsibility.
Nothing was approved, integrated, pushed or deployed by this implementation turn.
