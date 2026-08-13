# Medousa repo-wide technical critique

**Status:** first repo-wide pass complete
**Started:** 2026-08-12
**Completed:** 2026-08-12
**Scope:** tracked production code, tests, build configuration, integrations, and documentation
**Intent:** a deliberately blunt engineering review. Criticism applies to code and design decisions, never to contributors.

This is an architecture review, not canonical product documentation. It records static evidence and proposed experiments. A performance claim that has not yet been benchmarked is labeled as such.

## Baseline

The tracked tree is large enough that intuition and a handful of `clone()` searches are not a review:

| Surface | Tracked files | Lines (including tests/generated source) |
| --- | ---: | ---: |
| Rust | 736 | 289,720 |
| TypeScript | 919 | 133,942 |
| Svelte | 519 | 134,149 |
| Python | 70 | 8,105 |
| Markdown | 273 | 41,567 |

The audit therefore follows request-critical paths first, then uses repository-wide searches and build tooling to find repeated instances. Generated, vendored, and application-owned code are separated in the conclusions rather than scored as if they had equal ownership.

The blunt verdict: Medousa is not slow because Rust, Svelte, or Tauri are slow. It is slow because the architecture repeatedly turns local changes into global work. One token becomes synchronous JSONL I/O and a full Markdown render. One feed append becomes a whole-log rewrite. One checkpoint becomes a repository diff and untracked-file hashing pass. One vault read becomes a recursive tree scan. These are algorithm and ownership failures; deleting random `clone()` calls will not rescue them.

The security story has the same shape. Authority is represented by ordinary strings and broad routers, then each caller is trusted to remember the missing invariant. Public mode publishes the main daemon without mandatory authentication, session identifiers become paths, vault containment is lexical, and arbitrary browser pages receive a broad desktop capability. The code contains useful local safeguards, but they are inconsistent enough that the gaps dominate the result.

There is substantial good engineering here: explicit recovery concepts, typed DTO crates, broad unit-test coverage, a real SDK manifest, careful comments, and a clean warning-denying engine Clippy run. The problem is that too many important guarantees exist as conventions, comments, and duplicate tables rather than enforced boundaries. The repo has outgrown that model.

## Severity

- **Critical:** directly taxes or jeopardizes a primary runtime path and needs architectural correction.
- **High:** repeatable material cost, correctness risk, or development drag.
- **Medium:** localized waste or design debt with a clear fix.
- **Low:** hygiene, consistency, or future-risk issue.

## Findings index

| ID | Severity | Finding | State |
| --- | --- | --- | --- |
| SEC-001 | Critical | Public personal-mode daemon access is unauthenticated and the complete router permits every browser origin | Confirmed statically; adversarial integration test required |
| SEC-002 | Critical | Unvalidated session IDs become filesystem paths, including recursive-delete targets | Confirmed statically; exploit regression test required |
| SEC-003 | High | Vault “containment” is a lexical prefix check that follows symlinks outside the vault | Confirmed statically; exploit regression test required |
| DESKTOP-001 | Critical | Arbitrary web pages receive broad Tauri core privileges while the custom browser bridge is undeclared under the current ACL model | Current-version behavior confirmed from code and Tauri policy; cross-platform runtime test required |
| PERF-001 | Critical | One model delta is copied, serialized, synchronously written, reparsed, and rerendered across the entire stack | Confirmed statically; benchmark pending |
| STORE-001 | Critical | Feed appends rewrite whole logs and can persist stale snapshots out of order | Confirmed statically |
| DUR-001 | Critical | The session writer counts failed attempts as successful durable commits | Confirmed statically |
| PERF-002 | Critical | Forge mutations repeatedly replay complete event logs and force multiple sync points | Confirmed statically; benchmark pending |
| PERF-004 | Critical | Every Coder checkpoint replays Forge, diffs the repository, hashes untracked files, and rewrites a large snapshot | Confirmed statically; benchmark pending |
| PERF-005 | Critical | Streaming text triggers whole-answer Markdown parse, sanitization, DOM replacement, and rehydration per delta | Confirmed statically; browser profile pending |
| MEM-001 | Critical | Completed project-task runs are retained forever with duplicated bounded-but-large output | Confirmed statically |
| MEM-002 | High | Nested unbounded stream queues remove backpressure directly in front of blocking and stateful consumers | Confirmed statically; stress test pending |
| ASYNC-001 | High | Async Forge HTTP handlers run blocking filesystem, Git, and network subprocess work inline | Confirmed statically |
| PERF-003 | High | The vault index recursively stats the vault on ordinary reads and rebuilds global derived state on writes | Confirmed statically; benchmark pending |
| PERF-006 | High | Vault UI resolution and tree rendering repeatedly rebuild whole-vault sets and scan subtrees | Confirmed statically; benchmark pending |
| STORE-002 | High | Workspace debouncing begins after callers clone and serialize the complete state at stream frequency | Confirmed statically; benchmark pending |
| CONSIST-001 | High | Vault precondition checks and writes are non-atomic, allowing lost updates | Confirmed statically |
| CONC-001 | Critical | “Per-turn” tool, scope, and worker-bus context is shared mutable process/runtime state | Confirmed statically; concurrent regression test required |
| CONC-002 | High | Native-browser request/response rendezvous uses one global slot per command kind | Confirmed statically |
| TYPE-001 | High | The primary stream protocol is a stringly mega-struct with dozens of nullable variant fields | Confirmed statically |
| FRONT-001 | High | The initial app route statically loads 7.10 MB of minified JavaScript and 1.45 MB of CSS | Measured from production build |
| ARCH-001 | High | Frontend singleton stores, Markdown, Liquid, and vault code form large runtime import cycles | Confirmed by static import graph |
| ARCH-002 | High | Core ownership boundaries have collapsed into 3k–15k-line mega-modules and a 427-command desktop registry | Confirmed from source inventory |
| CONTRACT-001 | High | The claimed SDK source of truth has multiple hand-copied route tables and a checker that ignores route/type parity | Confirmed statically |
| DEP-001 | High | The main daemon has a 932-package normal dependency closure, 93 duplicate-version names, and unused direct adapter frameworks | Measured with `cargo tree` and source search |
| CI-001 | High | Required product tests/builds are absent from CI; the omitted frontend suite currently fails | Confirmed in workflow and local runs |
| TEST-001 | High | The required Rust lib suite mutates process-global state and consults the real OS keyring, causing races and hangs | Reproduced locally; isolated comparison performed |
| PERF-007 | High | Critical performance paths have no repeatable benchmarks, profiles, or regression budgets | Confirmed from repository and CI inventory |
| DATA-001 | Medium | Session deletion calls directory removal on a ledger file, leaving supposedly deleted activity behind | Confirmed statically |

## Detailed findings

### SEC-001 — The “public” daemon is a broad unauthenticated LAN API with permissive CORS

**Severity:** Critical
**Affected path:** `medousa --public`, mobile/pairing access, and any browser-reachable daemon

The router merges the core, Forge, vault, package, model, browser, delivery, session, maintenance, and other feature surfaces, then applies `CorsLayer::permissive()` to the whole thing (`src/daemon/router.rs:615-631`). The CLI's `--public` path intentionally resolves a non-loopback bind suitable for mobile access (`src/bin/medousa.rs:2339-2362`). Those two choices would require strong request authentication. Personal mode explicitly does the opposite.

`portal_acl::authorize_request` allows every untrusted remote request in personal mode unless it presents a recognized Peer credential and then tries to exceed that peer's restricted surface (`src/portal_acl.rs:140-160`). The comment is unambiguous: “Portal (or no bearer) stays open.” The peer-scope middleware is installed over the application only when pairing exists (`src/bin/medousa_daemon.rs:640-670`), but that middleware delegates to this policy; absence of credentials is still `Allow` in the default personal mode. Bearer handling enriches identity when a valid token exists; it does not make a token mandatory (`src/daemon/interactive.rs:298-304`).

The result is not a narrowly scoped pairing endpoint. A device on the LAN can reach the main control plane without proving possession of a pairing secret. That plane contains mutations for sessions, vault content, Forge/Git state, models, packages, workspace jobs, approvals, browser control, and runtime administration. `CorsLayer::permissive()` additionally tells arbitrary web origins that the daemon consents to browser requests. Browser Private Network Access, mixed-content rules, loopback binding, and “the user probably trusts their Wi-Fi” are environmental mitigations, not authorization boundaries.

This is release-blocking for any advertised public/LAN mode. A remote unauthenticated API that can cause local filesystem, process, Git, and model actions is remote code execution by composition even if no single handler calls itself `exec`.

#### Recommended correction

1. Require an unguessable, scoped credential on every non-bootstrap route whenever the requester is not a verified co-located app transport. Default-deny must happen before the request reaches feature handlers.
2. Split the router. Public pairing/mobile access should expose only the minimum bootstrap and paired-client surface; do not publish the desktop daemon's entire administrative router on `0.0.0.0`.
3. Replace permissive CORS with an exact allowlist for known development/app origins. Add a CSRF/origin-bound capability for browser-accessible loopback requests; CORS alone is not authentication.
4. Give paired devices explicit capabilities and require them at the route group, not through an exception-heavy path classifier. Unknown/no credential should be denied remotely in both Personal and Shared modes.
5. Rotate/revoke credentials, rate-limit bootstrap attempts, redact them from logs, and bind the mobile URL to the actual authenticated pairing flow.
6. Add black-box tests that bind a real server on a non-loopback interface and try every route class with no token, an invalid token, Peer, Portal, and root credentials. Generate the route matrix from the router so new endpoints cannot silently inherit public access.

### SEC-002 — A session identifier is also an unsanitized filesystem path

**Severity:** Critical
**Affected path:** history, artifacts, media, extraction, verification, and session deletion

Interactive request validation trims `session_id` and checks only that it is non-empty (`src/interactive_turn_runtime.rs:10-16`, `src/daemon/interactive.rs:85-94`). That externally supplied string then crosses a large number of filesystem boundaries without a single canonical representation:

- History constructs `history/<session_id>.jsonl` directly (`src/session.rs:33-37`), and the file store reads, appends, and deletes that path (`src/session.rs:611-637`, `src/session_store.rs:164-175`).
- Artifacts repeatedly use `artifacts_root().join(session_id)` (`src/artifact_store.rs:148`, `:224`, `:301`, `:709`); media, extraction, and verification do the same (`src/media_store.rs:73`, `src/artifact_extraction.rs:99`, `src/verification_store.rs:99`).
- Verification cleanup recursively removes `verifications_root().join(session_id.trim())` (`src/verification_store.rs:152-154`). Session deletion accepts any non-empty ID and invokes that cleanup (`src/session_lifecycle.rs:23-35`, `:78-84`).

`Path::join` is not containment. `..` components escape the storage root, and an absolute component replaces it. Separators create arbitrary descendants. Symlinks can redirect later traversal. Depending on which endpoint accepts the value and which surface exists, this permits reads/writes outside the intended session directory and turns cleanup into an attacker-influenced recursive deletion. Route decoding may reject some slash spellings on a particular path parameter; it does not repair body-provided IDs or the unsafe persistence API.

The repository already sanitizes the same identifier for turn-ledger files (`src/agent_runtime/turn_ledger.rs:232-250`). That inconsistency is evidence of the real defect: `session_id: String` has no enforceable invariant, so every caller independently remembers—or forgets—to make it safe.

#### Recommended correction

1. Introduce one validated opaque `SessionId` newtype at the API boundary. Prefer a fixed canonical alphabet/length (UUID/ULID or a similarly strict format); reject separators, absolute paths, dot components, control characters, and non-canonical aliases.
2. Never derive authority-bearing paths from display IDs. Encode or hash the validated ID into a filename and keep the mapping in a store when compatibility requires arbitrary external labels.
3. Centralize path construction. After joining, verify containment against a canonical root and handle symlinks deliberately. For destructive operations, operate through a directory handle/no-follow semantics rather than trusting a string prefix check.
4. Migrate or quarantine legacy filenames whose decoded IDs are invalid. Do not silently reinterpret two old IDs as one sanitized filename.
5. Add table-driven tests for `..`, `.`, absolute paths, both platform separators, percent-encoded forms, Unicode lookalikes, trailing dots/spaces on Windows, symlink escapes, and overlong identifiers. Run them against every write, read, move, and delete surface.

### SEC-003 — Vault containment checks strings, not the filesystem

**Severity:** High
**Affected path:** vault reads, writes, trash, restore, and project overlays

Vault path normalization correctly rejects separators, dot components, and obvious `..` traversal (`src/vault/path.rs:30-59`). The final containment check is nevertheless only an absolute lexical prefix comparison. `ensure_within_root` absolutizes the candidate's parent without canonicalizing any existing component, then asks whether that spelling starts with the root spelling (`src/vault/path.rs:92-107`). A symlink inside the vault is therefore treated as if it were an ordinary directory inside the vault.

That is exploitable by composition. If `{vault}/linked` points outside the vault, `linked/secret.md` passes `resolve_user_note_path`. Reads follow the link (`src/vault/store.rs:465-473`); writes create parents and call `fs::write` on the escaped target (`:478-510`); delete and restore perform rename/remove operations using similarly resolved paths (`:553-567`, `:619-633`). The overlay resolver has the same property. The existing containment tests cover lexical `..` and a missing nested parent, but no symlink or reparse-point case (`src/vault/path.rs:152-183`).

This matters even if the daemon is only local: vaults are user-controlled content, can be Git checkouts, can be synced from another machine, and are exposed to agent tools. “The user created the symlink” is not a filesystem authorization policy.

#### Recommended correction

- Decide the product rule explicitly. The safe default is to reject symlinks/reparse points in every ancestor used by authority-bearing vault operations. If links are supported, resolve them and verify the resolved target is inside an explicitly allowed root.
- Use handle-relative, no-follow operations where the platform permits them. A preflight `canonicalize` followed by an ordinary path write remains subject to time-of-check/time-of-use replacement.
- For a new leaf, resolve and verify the nearest existing ancestor, then create/open descendants without following links. Apply the same primitive to trash and overlay roots.
- Add Unix symlink and Windows junction/reparse-point tests for read, create, overwrite, delete, rename, restore, and a link swapped concurrently with the operation.

### DESKTOP-001 — The embedded browser's IPC security model is both overbroad and incomplete

**Severity:** Critical
**Affected path:** desktop embedded/pop-out browser, metadata, hotkeys, find, snapshot, and browser actions

The capability named `browser-tab-webviews` matches the browser webviews, permits every `https://*` and `http://*` origin, and grants `core:default` (`apps/medousa-home/src-tauri/capabilities/browser-tab-webviews.json:1-10`). That permission is not a harmless prerequisite for one callback. The generated Tauri manifest expands it to the default path, event, window, webview, app, image, resource, menu, and tray permission sets. For example, the event default permits listen, unlisten, emit, and emit-to. An arbitrary visited page should not be a participant in the desktop shell's event bus or window/resource APIs.

The browser implementation then injects scripts into those untrusted pages and calls custom commands through `window.__TAURI_INTERNALS__.invoke`: location/favicon/navigation reporting, new-window interception, shell hotkeys, snapshots containing the complete page HTML, DOM actions, and find results (`apps/medousa-home/src-tauri/src/human_browser.rs:798-844`, `:2638-2716`, `:2754-2773`, `:2829-2873`, `:2896-2961`). There are 427 `#[tauri::command]` declarations in the desktop crate and one giant generated handler list (`apps/medousa-home/src-tauri/src/lib.rs:276-575`).

Current Tauri behavior prevents the worst interpretation but exposes a second defect. The lock selects Tauri 2.11.2. Tauri 2.11.1 changed custom commands so remote origins are always ACL-checked; applications must declare an `AppManifest` and permissions for commands intended for remote webviews. Medousa's build script calls plain `tauri_build::build()` and defines no application command manifest (`apps/medousa-home/src-tauri/build.rs:1-9`); the browser capability contains no custom-command permission. Therefore the injected custom command calls should be denied under the locked version, while their JavaScript catches or ignores the failure and Rust waits for two- or eight-second timeouts. Tauri's official [2.11.1 release note](https://v2.tauri.app/release/tauri/v2.11.1/) and [security-fix pull request](https://github.com/tauri-apps/tauri/pull/15266) document this exact change.

That yields an unacceptable version-dependent design:

- on vulnerable pre-2.11.1 behavior, a remote page could bypass custom-command ACL and potentially reach the full handler surface;
- on the locked behavior, `core:default` remains much broader than the browser needs, while the custom report bridge is not declared and should silently fail; and
- if someone “fixes” the timeouts by granting all custom commands to remote origins, any website can invoke filesystem, daemon, credential, package, window, terminal, and other shell authority exposed by the 427-command handler.

`cargo check` cannot detect this runtime permission mismatch. The main Tauri config also disables CSP and gives the asset protocol a home-wide scope (`apps/medousa-home/src-tauri/tauri.conf.json:31-36`), increasing the consequence of a local frontend injection even though it is not by itself proof that a remote page can fetch those assets.

#### Recommended correction

1. Give remote browser content no generic Tauri IPC capability. Remove `core:default`; untrusted web content should not know or access the shell's internal IPC object.
2. Prefer native navigation/title/new-window hooks and Rust-side webview APIs. For operations that require page execution, return results through a dedicated per-request native channel or a tiny isolated bridge with a nonce/request ID, strict schema/size limits, and explicit origin/webview binding.
3. If Tauri custom commands must remain, create an app manifest exposing only the report commands, generate individual allow permissions, and attach only those permissions to the exact browser webviews. Validate the invoking webview label and current origin in Rust; never grant the 427-command application surface.
4. Replace global response slots as described in CONC-002 and cap snapshot HTML before it crosses IPC.
5. Add a packaged-app security test that visits an attacker-controlled local origin and asserts every core/plugin/application command is denied except the deliberately minimal bridge. Separately assert location, hotkey, find, snapshot, and action callbacks work on Tauri 2.11.2 across macOS, Windows, and Linux.
6. Add a restrictive CSP to the trusted app shell and narrow asset scope to selected vault/artifact roots. Treat those as independent defense-in-depth boundaries rather than compensation for remote-webview privileges.

### PERF-001 — The token-stream path is an allocation and I/O amplification pipeline

**Severity:** Critical
**Affected path:** model output → engine sink → durable turn log → Tokio broadcast → Axum SSE → Tauri bridge → TypeScript chat store → Svelte UI

The path carrying the most latency-sensitive payload in the product—a content or reasoning delta—does substantial work at every boundary. The problem is not any single `to_string()`. The same tiny payload is repeatedly materialized, serialized, copied, and made reactive.

#### Engine-side amplification

1. `AgentStreamSink::content_chunk` requires an owned `String` for every delta (`crates/medousa-engine/src/stream_sink.rs:12-15`). Ownership may be reasonable at the producer boundary, but everything downstream should then avoid rematerializing it.
2. `InteractiveTurnStreamSink::content_chunk` appends the delta to a second cumulative `String` (`src/agent_runtime/daemon_interactive_turn.rs:265-277`). It then acquires `parts: Mutex<_>` solely to call `push_content_delta`, which is intentionally a no-op (`src/turn_parts.rs:38-41`). That is a mutex acquisition per content delta for literally no state change.
3. Building the wire event allocates fresh owned strings for the turn ID, event type, phase, and delta (`src/interactive_turn_runtime.rs:193-208`, `src/interactive_turn_runtime.rs:652-701`). Two of those strings are constants (`"content_delta"`, `"streaming"`) represented as heap-owned `String`s.
4. Journaling immediately clones the delta into a separate `TurnEvent` (`src/sse_turn_projection.rs:9-16`).
5. `TurnEventLog::append` serializes every event into a new JSON `String`, writes it through an unbuffered `File` while holding a `std::sync::Mutex`, calls `flush`, clones the full sequenced event into an in-memory vector, and only then returns (`crates/medousa-engine/src/turn_event_log.rs:85-105`). This synchronous filesystem work executes inline from the async stream sink (`src/agent_runtime/daemon_interactive_turn.rs:190-217`). It can block a Tokio worker on every model delta. `File::flush` is not `sync_data`/`sync_all`, so the latency buys neither batched throughput nor power-loss durability.
6. The broadcast stores the large string-heavy DTO and clones it for receivers (`src/daemon/turn_event_channel.rs:15-39`). The log also retains every delta in memory for replay, in addition to the cumulative presentation buffer and journal file.

This design simultaneously keeps an event-sourced token history, a cumulative streamed body, a reasoning accumulator, an in-memory clone of every sequenced event, and a JSONL copy. Some of those serve valid recovery or presentation requirements, but the current representation pays for all of them synchronously at token granularity.

#### Bridge-side amplification

The Tauri SSE parser turns an otherwise linear buffer drain into repeated copying:

- It finds the first frame delimiter, copies the frame with `to_string()`, then copies the entire unconsumed suffix into a second new `String` for every frame (`apps/medousa-home/src-tauri/src/daemon/sse.rs:190-203`). With many frames already buffered, repeated suffix copies are quadratic in buffered bytes.
- `parse_sse_data` allocates one `String` per `data:` line and then joins them into yet another `String` (`apps/medousa-home/src-tauri/src/daemon/sse.rs:227-238`).
- The generic helper deserializes the JSON into `T`, but every current callback discards `T`; for interactive turns the callback is exactly `|_event| {}` (`apps/medousa-home/src-tauri/src/daemon/mod.rs:509-519`). The bridge then emits the original JSON as a string (`apps/medousa-home/src-tauri/src/daemon/sse.rs:208-212`).
- TypeScript receives that string and parses the same JSON again (`apps/medousa-home/src/lib/daemon.ts:1001-1010`). Workspace and environment streams repeat the same pattern.

So Rust pays to construct an owned typed payload that nobody reads, while JavaScript pays to reconstruct another typed object immediately afterward.

#### UI-side amplification

For each content delta, `applyStreamEventToMessage`:

- linearly searches the messages array (`apps/medousa-home/src/lib/stores/chat.svelte.ts:2748-2758`);
- appends to an immutable JavaScript string (`:2887-2890`), which can repeatedly copy or build rope state as the response grows;
- copies tool arrays and scans for duplicates even when the event is only text (`:2970-2987`);
- rebuilds the entire messages array from two slices plus a copied message (`:2988-2992`);
- schedules every event through a new Promise link in `streamApplyChain` (`:2390-2401`), with no frame- or time-based batching.

This makes rendering cost a function of both response length and transcript length. Long answers in long chats are exactly the case in which an assistant UI should stay smooth, and this implementation makes that its worst case.

#### Why this matters

This path affects time-to-first-token, inter-token jitter, daemon scheduler fairness, UI main-thread work, allocation rate, and replay memory. It is also multiplicative: improving only the Rust event constructor leaves synchronous journaling and per-token reactive array copies in place.

#### Recommended correction

Treat streaming as a deliberately batched pipeline while retaining sequence correctness:

1. Introduce a compact internal event enum whose event kind/phase are enums or static values and whose turn identity is shared (`Arc<str>` or an interned/turn-scoped handle). Convert to the string-heavy public DTO only at the HTTP serialization boundary.
2. Coalesce content/reasoning deltas by a small latency budget (for example, one UI frame or 10–25 ms) and/or byte threshold. Sequence batches, not individual provider token fragments.
3. Move journal writes to a dedicated buffered writer task. Send typed events over a bounded channel, write multiple JSONL records per batch, and define an explicit durability policy (terminal `sync_data`, periodic sync, or documented best effort). Never perform unbuffered file I/O while holding the turn log mutex on an async executor thread.
4. Remove the no-op `parts` lock from content deltas. For reasoning, either batch into the same actor or use the already-owned delta without another event-shaped clone.
5. Store replay data once. If the JSONL journal is authoritative, keep only a bounded live ring plus sparse offsets; if memory is authoritative during the 30-second grace period, batch events and avoid cloning every string-heavy envelope.
6. Parse SSE incrementally over bytes. Drain consumed bytes (`BytesMut::split_to`, a cursor/offset plus occasional compaction, or a proper SSE codec) instead of copying the remainder for every frame.
7. Cross the Tauri boundary once: either emit the deserialized typed payload directly (requiring `Serialize`) or validate/forward raw JSON without constructing a discarded `T`. Do not deserialize in Rust and immediately parse again in JavaScript.
8. In the chat store, buffer deltas per turn and commit at most once per animation frame. Maintain an ID→message-index map (or normalized message store), mutate/replace only the target entry in the reactive structure, and skip tool-array work for plain content events.

#### Verification required before and after the fix

- Add a benchmark that streams 10,000 small deltas into a 1,000-message transcript.
- Record allocations/bytes, journal write calls, p50/p99 sink latency, Tokio worker blocking time, bridge CPU, and dropped UI frames.
- Test reconnect from every batch boundary and after a forced process kill so batching does not weaken the promised replay semantics.

### MEM-002 — Streaming removes backpressure at the exact point it is needed

**Severity:** High
**Affected path:** primary turns, inference attempts, worker turns, and other streaming producers

The orchestrator explicitly says model pipelines enqueue through an unbounded sender. `TurnStreamBridge` creates an unbounded queue whose one consumer awaits each sink call (`src/agent_runtime/turn_orchestrator.rs:62-84`). Each inference attempt adds another unbounded queue in front of it (`:109-130`). Worker execution repeats the pattern (`src/agent_runtime/turn_worker/run.rs:900-927`), and the provider-facing stream API exposes `UnboundedSender<StreamDelta>` throughout the tool loop and OpenAI/Codex client.

That construction preserves event order but provides no flow control. The consumer immediately downstream performs the synchronous journal, mutex, broadcast, persistence, and UI delivery work described in PERF-001 and STORE-002. If disk, a client, serialization, or the workspace writer stalls, the provider keeps producing owned `String` deltas and the process keeps retaining them. The two nested primary-turn queues can both accumulate. Draining before terminal publication protects ordering only after the producer stops; it does nothing to bound live memory or latency.

Other unbounded channels—the Tauri terminal outbound path and Forge filesystem watcher—show the same default, but the model path is the clearest production risk because input is bursty and consumer work is intentionally stateful.

#### Recommended correction

- Use a bounded channel sized from an explicit latency/memory budget. Make provider callbacks async/backpressured where possible; where an SDK callback cannot await, coalesce into a bounded byte ring and surface overflow as a visible turn failure rather than allocating until the OS intervenes.
- Collapse the nested attempt/turn queues into one turn-owned sequencing actor. Tag attempt boundaries in the message protocol instead of creating an unbounded forwarding layer.
- Coalesce adjacent content/reasoning fragments before enqueue and measure queue depth/bytes, blocked-send duration, batch size, and high-water marks.
- Define cancellation behavior: close producers immediately, discard only events beyond a documented sequence fence, and never wait indefinitely to drain a consumer that has failed.
- Stress with a producer substantially faster than a deliberately stalled disk/UI sink. Assert a hard resident-memory ceiling and bounded cancellation/terminal latency.

### STORE-001 — The feed “append” is a whole-file rewrite with a lost-update race

**Severity:** Critical
**Affected path:** recurring/custom feed publication and reads

`FeedStore` calls its files JSONL logs, but `append` does not append:

1. A single global `RwLock<HashMap<profile, HashMap<feed, state>>>` guards every feed for every profile (`src/feed_store.rs:21-24`).
2. On a cold feed, `append`, `tail`, and `set_read_cursor` hold that global **write** lock while awaiting a complete file read and JSON parse (`src/feed_store.rs:96-105`, `:119-130`, `:184-195`). One cold or slow feed blocks unrelated reads and writes.
3. Every append pushes one event, shifts the entire vector with `remove(0)` at capacity, clones the complete retained channel—including every `serde_json::Value` body—then drops the lock (`:105-115`).
4. `persist_feed_channel` serializes all retained events into one new `String` and truncates/rewrites the entire file (`:76-93`). With a 200-event cap, steady-state append cost remains O(200 × average event size), and the total work while filling a feed is quadratic.
5. `tail` clones every returned `FeedEvent`; `event_count` obtains the count by cloning up to 200 events through `tail` (`:119-130`, `:157-160`). `latest_good` does the same before scanning backward (`:162-180`).

The more serious issue is ordering. `append` releases the state lock before awaiting persistence. Two tasks can therefore obtain snapshots A and A+B, then finish their file writes in the opposite order. The older A snapshot can truncate the file after A+B completed, losing B across restart. Because `tokio::fs::write` is not an atomic replace, overlapping writes can also expose a partial file to readers or a process crash.

This is a classic “in-memory state is serialized; durable state is not” bug. The lock protects mutation but not the side effect whose order defines persistence.

#### Recommended correction

- Give each `(profile_id, feed_id)` a loaded-once channel state and a dedicated ordered writer (or per-feed mutex that spans sequence assignment and append I/O). Do not hold a global map lock during file I/O.
- Make the file genuinely append-only with one serialized record plus newline per event. Periodically compact to a temporary file and atomically rename it when enforcing retention.
- Use `VecDeque` for bounded in-memory retention, or a fixed-capacity ring, instead of shifting a `Vec`.
- Return counts from metadata; scan borrowed events under a read guard for `latest_good`; clone only the response event(s).
- Sanitize/profile-encode path components before using IDs as directory/file names.
- Add a deterministic concurrency test with a persistence hook that forces snapshot A to finish after A+B, then reopen the store and assert both records exist. Add crash/partial-tail recovery tests.

### DUR-001 — The “never drops” session writer cannot observe failure

**Severity:** Critical
**Affected path:** conversation history durability

The writer module repeatedly promises that a lost turn is impossible without being observed (`src/session_writer.rs:13-26`, `:53-65`, `:141-145`). Its types make that promise impossible to keep:

- `SessionStore::append_turn` returns `()` (`src/session_store.rs:135-142`). The Surreal implementation logs an error and returns normally on either the query or response failure (`:268-292`). The file implementation likewise discards file-write outcomes through the lower-level API.
- `commit_blocking` explicitly acknowledges that errors are swallowed, then unconditionally increments a success counter (`src/session_writer.rs:119-139`).
- `WriterMetrics::write_failures` is only declared and read; no production code ever increments it (`:55-87`). It will report zero even while every write fails.
- The unit test waits for those attempt counters and calls them “committed”; it never reloads the session or injects a failing store (`:217-247`). The test proves queue consumption, not durability.
- The advertised batch is a `Vec` followed by up to 64 individual blocking `append_turn` calls (`:102-116`). There is no database transaction or batched store API, so the claim that the store can group commits is unsupported by this layer.
- The static sender has no explicit shutdown/drain protocol. A process exit can still discard queued jobs.

This is worse than missing metrics: it creates false confidence around the product’s core “your history survives” contract.

#### Recommended correction

1. Make the persistence boundary honest: `async fn append_turn(...) -> Result<CommitReceipt, StoreError>` (or a synchronous `Result` only for a truly synchronous backend). Remove the `block_in_place(Handle::block_on(...))` adapter pattern from the store trait.
2. Have each queued job carry an optional acknowledgement channel. Terminal paths that claim durability must await an actual store acknowledgement before marking the event log committed; lower-value writes can use an explicitly documented buffered policy.
3. Add a real `append_batch`/transaction API if batching is desired. Merely draining into a `Vec` is not batching at the storage boundary.
4. Increment success only after `Ok`; increment and classify failures on `Err`; retry only retryable errors with bounded backoff and a dead-letter path.
5. Add graceful shutdown that closes the sender, drains with a deadline, and reports anything uncommitted.
6. Replace the current counter test with fault-injection tests: store returns an error, actor shuts down with queued jobs, queue overflows, and successful writes are reloaded from a fresh store instance.

### PERF-002 — Forge uses an event log like a database but reads it like a shell script

**Severity:** Critical
**Affected path:** work-item creation, transitions, leases, listings, attempts, and most Forge-backed UI

The Forge persistence format could be perfectly adequate at this product scale. The implementation turns it into an algorithmic trap:

- `EventStore::append` calls `last_seq`, and `last_seq` performs a complete `replay` of the log before appending one event (`crates/medousa-forge/src/store.rs:128-154`). Every append is therefore O(number of prior events).
- `replay` first collects every line into `Vec<String>`, then parses those owned strings into a second `Vec<EventEnvelope>` (`:156-205`). It does not stream parsed envelopes or seek from a snapshot offset.
- Every individual append calls `sync_all` (`:128-150`). Snapshot writes pretty-print a full cloned `WorkItem`, sync the temporary file, rename it, and sync the directory (`:207-241`). These are defensible durability primitives in isolation, but the call graph invokes them repeatedly for one logical operation.
- `Forge::load` still replays the entire log just to obtain its last sequence before considering the snapshot (`crates/medousa-forge/src/forge.rs:272-286`). The snapshot avoids some folding, but not the read, allocation, parse, and validation cost it should eliminate.
- `Forge::list` loads every item (`:302-307`), so listing work items reparses every work-item event log. Registration also loads all items merely to collect their slugs (`:248-258`).
- Lease generation replays the complete log to count prior leases (`:1911-1917`). `transition` appends—which already replays—then discards the returned envelope and replays again to rediscover the sequence it just received (`:1978-2000`). `persist_fresh` adds another replay (`:2017-2025`).

This produces O(n²) lifetime parsing for a growing work item, multiplied by listings and compound commands, with a forced storage sync at event granularity. The event-sourcing abstraction is not the problem; throwing away the tail position and returned sequence on every call is.

#### Recommended correction

1. Keep authoritative tail metadata (`last_seq`, byte offset, checksum/version) under the same per-item lock as append. Recover it once on open; do not replay to increment a counter.
2. Use the sequence returned by `append`. Delete immediate “tell me what I just appended” replays.
3. Load a validated snapshot first, then seek to and replay only its tail. Include the log byte offset and last envelope hash in the snapshot so truncation or divergence remains detectable.
4. Parse records directly from `BufRead::lines` into the fold/replay consumer. Do not first allocate a complete `Vec<String>`.
5. Give compound domain operations one persistence transaction/batch and one explicit durability boundary. If every event must remain independently durable, document and benchmark that requirement rather than accidentally paying several syncs per HTTP request.
6. Add scaling benchmarks at 10, 1,000, and 100,000 events for append, load, list, and transition. Assert near-constant append cost and snapshot-tail load cost.

### MEM-001 — Every completed project-task run is a permanent, multiply-copied allocation

**Severity:** Critical
**Affected path:** project command/task execution in the daemon

`PROJECT_TASK_RUNS` is a process-global `RwLock<HashMap<...>>` (`src/daemon/forge_api.rs:5433-5435`). Runs are inserted at startup, but the completion path removes only the child-process handle (`:6308-6463`). There is no `remove`, `retain`, TTL, LRU, or other eviction for `PROJECT_TASK_RUNS` anywhere in production code.

The retained value is not small:

- Each `ProjectTaskRun` owns cumulative stdout and stderr (`:5375-5399`), each capped at 256 KiB (`:5420-5422`).
- The cap helper appends to a `String`, then uses `drain(..overflow)` from the front (`:5442-5458`). Once full, every output chunk can memmove roughly the remaining 256 KiB. A long noisy command turns a fixed memory cap into sustained O(capacity × chunks) copying.
- Every output event is cloned into a replay chunk queue and broadcast (`:5571-5638`). Terminal state clones stdout/stderr into the stored result and then clones that result into a state event (`:5653-5700`). The nominal 512 KiB cap can therefore occupy well over 1 MiB per completed run before allocator overhead.
- Fetching a run clones the whole retained structure. Replay clones pending chunks. On broadcast lag, the SSE loop simply continues instead of repairing the gap from that retained replay buffer (`:6583-6650`), so the code pays the retention cost without delivering reliable recovery.
- One global write lock serializes output publication for every concurrently running project task.

At 1 MiB of retained data per noisy task, 1,000 completed runs is roughly a gigabyte of live process memory. The exact multiplier needs measurement; the absence of eviction does not.

#### Recommended correction

- Remove terminal runs after a documented reconnect window, backed by a TTL/LRU and hard byte/count caps. Expose eviction and retained-byte metrics.
- Store output once in a bounded byte ring or spool file. Use `VecDeque<u8>`, a circular buffer, or cursor-based chunks rather than front-draining a UTF-8 `String`; preserve UTF-8 only when rendering frames.
- Make each run an `Arc` with a per-run lock/channel. Keep the registry lock only long enough to look up the run.
- Store result metadata separately from output references. Do not clone two 256 KiB strings into every terminal representation.
- Give replay chunks sequence numbers and make a lagged subscriber replay from the bounded ring or explicitly receive a gap/reset event.
- Add a soak test that completes thousands of noisy tasks and asserts resident/retained memory returns to the configured steady-state bound.

### ASYNC-001 — `async fn` does not make blocking Forge and Git work asynchronous

**Severity:** High
**Affected path:** Forge HTTP API and daemon responsiveness

`src/daemon/forge_api.rs` exposes dozens of async handlers, but most call the synchronous `Forge` and `GitEngine` stacks directly. Representative examples include list/load (`:1560-1571`), fetch/pull/push/sync (`:3952-4125`), provisioning (`:7389-7405`), attempt lifecycle methods (`:7425-7784`), and discard/cleanup operations that follow them. The Git calls ultimately wait on `std::process::Command::output`, including network operations.

Only a small subset of the module uses `spawn_blocking`. Consequently, recursive filesystem work, event-log replay, `sync_all`, Git index operations, subprocess waits, and network-backed Git commands can occupy Tokio worker threads. Under concurrency this inflates unrelated request latency and can starve streaming work—the exact work most sensitive to jitter.

#### Recommended correction

- Put the synchronous Forge/Git subsystem behind a bounded blocking service or actor. A tactical `spawn_blocking` wrapper is acceptable, but it needs a semaphore/queue; unlimited blocking jobs merely move overload to another pool.
- Use async subprocess APIs for long-lived or network Git commands where cancellation and streamed progress matter. Apply explicit timeouts and terminate child processes on cancellation.
- Never hold an async/global registry lock while awaiting the blocking service.
- Add a concurrency test: run slow fetches and large Forge loads while measuring a trivial health request and token-stream p99 latency.

### PERF-003 — The vault index performs the work an index is supposed to eliminate

**Severity:** High
**Affected path:** note reads, lists, backlinks, writes, deletes, and search

Nearly every `VaultStore` accessor begins with `ensure_index_fresh` (`src/vault/store.rs:127-220`). That function recursively walks both user and overlay trees and stats every candidate file. It then clones and compares index-shaped maps and sets. This happens for `list_entries`, `get_entry`, `note_exists`, `backlinks`, and `all_entries` (`:443-462`, `:637-677`).

The layering compounds the scan count. A single `VaultService::get_note` calls `get_entry`, reads the file, then calls `backlinks`; both store calls perform freshness walks (`src/vault/service.rs:57-66`). Search calls `all_entries`, then reads every matching note file and performs case folding over titles, filenames, headings, and content (`src/vault/search.rs:15-132`). It recomputes `query.to_ascii_lowercase()` inside the per-file loop (`:86`) and lowercases lines again while making snippets (`:142-153`). After visiting everything, it sorts every hit merely to truncate to a limit.

Writes are also global rebuilds. `write_content` clones all index keys and entries, writes one file, then rebuilds and persists the entire link index and persists the entire note index (`src/vault/store.rs:478-550`). Link resolution can be O(notes × links × notes): an unresolved basename scans every known path and entry and repeatedly slugifies titles (`src/vault/links.rs:76-130`, `:185-194`). The service then asks for backlinks for the response, initiating another freshness walk.

Freshness is not even robust enough to justify this expense: the comparison uses modification timestamps truncated to whole seconds (`src/vault/store.rs:151-159`) and ignores file size/content identity. An external edit that preserves the observed second can remain invisible.

Finally, all vault handlers are async functions that invoke this synchronous recursive I/O inline (`src/vault_handlers.rs:31-180`). The PUT handler also copies the entire Axum body before UTF-8 validation with `String::from_utf8(body.to_vec())` (`:66-86`).

#### Recommended correction

1. Make the index resident and generation-based. Use a filesystem watcher plus targeted reconciliation, with an explicit full rescan only at startup, watcher overflow, or manual refresh.
2. Track nanosecond mtime, size, and preferably a cheap content identity. Keep a stable note-ID/path map and update only the changed note and affected backlink edges.
3. Replace repeated title/path scans with precomputed exact-name and slug lookup maps. Build backlink adjacency incrementally.
4. Put full-text search behind an inverted index (SQLite FTS5, Tantivy, or a small purpose-built index). At minimum precompute folded query terms, avoid rereading unchanged files, and use bounded top-k selection.
5. Return a combined note snapshot/backlink result from one index generation rather than independently “refreshing” each accessor.
6. Move unavoidable scans and synchronous filesystem work to the bounded blocking service described in ASYNC-001.
7. Benchmark cold and warm get/list/search/write at 100, 10,000, and 100,000 notes, while asserting event-loop latency.

### CONSIST-001 — Vault optimistic concurrency is a check-then-write race

**Severity:** High
**Affected path:** concurrent note edits, creates, moves, and crash recovery

The write path reads the current bytes, compares `If-Match`, and later performs a normal `fs::write` (`src/vault/store.rs:478-509`). There is no per-note lock, transaction, atomic compare-and-swap, or atomic replacement. Two callers with the same valid ETag can both pass the comparison and then overwrite each other; both may report success. `create_note` similarly checks `note_exists` and later writes, so simultaneous creates can clobber (`src/vault/service.rs:99-110`).

The index and link files are written with direct create/truncate operations, and ordinary content writes are not temporary-file-plus-rename commits. A crash can leave a partial note or partial metadata file. Relocation writes the destination, emits intermediate side effects, and then deletes the source (`src/vault/service.rs:179-209`), so failure can leave duplicates or a partially reported move.

#### Recommended correction

- Serialize mutations per canonical note identity. Perform precondition validation and commit inside that critical section.
- Write content to a same-directory temporary file, sync according to the documented durability contract, and atomically rename. For create, use create-new/no-clobber semantics.
- Treat move as one recoverable operation: reserve destination, atomically rename when possible, update the index once, and emit feed/events only after commit. Define the cross-filesystem fallback and recovery marker.
- Add deterministic two-writer tests where both callers present the same ETag; exactly one must commit. Inject failures at every move and replace stage and verify a restart reconstructs one coherent state.

### CONC-001 — “Per-turn” context is global mutable state and will cross-wire concurrent turns

**Severity:** Critical
**Affected path:** tool execution, browser handoff, UI artifacts, history access, worker delegation, and turn cleanup

The most misleading comment in the runtime is `Per-turn ambient tool sink`: the value immediately below it is a process-global `Lazy<RwLock<Option<Arc<...>>>>` (`src/engine_adapters.rs:121-132`). An interactive turn installs its sink before running and unconditionally writes `None` afterward (`src/agent_runtime/daemon_interactive_turn.rs:976-1007`). Browser tools retrieve whatever sink happens to occupy that singleton when they emit (`src/browser_act_tools.rs:378`, `src/browser_search.rs:31-41`).

For overlapping turns A and B, this schedule is legal:

1. A installs sink A.
2. B installs sink B.
3. A emits a browser event; it is delivered to B.
4. A finishes and clears the global slot.
5. B emits; its event disappears.

The same flaw is embedded more deeply in `TuiRuntime`. It owns one `Arc<RwLock<Option<TurnContinuationScope>>>` (`src/tools.rs:2850-2873`). Every interactive turn saves that shared value, replaces it with its own scope, and later restores the stale snapshot (`src/agent_runtime/daemon_interactive_turn.rs:931-970`, `:1007`). Many registered tool instances retain this shared lock and read it at invocation time, including history, environment, browser, UI, bootstrap, and OpenShell tools. A tool can therefore read another session's identity, chat session, feature surface, provider/model, or delivery target.

Worker delegation repeats the pattern. The one runtime-owned `TurnWorkerScheduler` has singleton `runtime_ctx` and `bus_session` slots (`src/agent_runtime/turn_worker/run.rs:160-230`). Each host turn overwrites both (`src/agent_runtime/turn_orchestrator.rs:978-1008`), while numerous exit branches unconditionally clear the bus session. A concurrent host can spawn a worker against another host's session or have its context cleared underneath it.

The ticket registry only excludes a second interactive turn **within the same session** (`src/turn_ticket.rs:94-111`). The daemon spawns accepted turns (`src/daemon/interactive.rs:233-263`), so cross-session overlap is both allowed and expected. This is not a theoretical microservice-scale race; one local user can trigger it with two chats or a foreground turn plus worker activity.

#### Recommended correction

- Delete ambient mutable turn state. Construct an immutable `TurnContext` per execution and pass it through the tool-call context or bind it with Tokio task-local scope where an API truly cannot accept an argument.
- Make tool sinks part of that context. Task-local values must propagate explicitly into spawned tasks; do not fall back to a process singleton.
- Key worker host sessions by an unforgeable turn/session handle. `spawn_worker` should require the caller's handle and look up that exact immutable session, not read “the active one.”
- Do not implement scoped state with save/overwrite/restore on a shared slot; restoration is still a lost-update race.
- Add a barrier-controlled regression test with turns A and B in different sessions. Interleave installation, tool calls, worker spawn, and cleanup, and assert every emitted event, identity, artifact, and child work item remains attached to its origin.
- Until the architecture is corrected, a single global turn mutex would at least preserve correctness, but it would intentionally destroy concurrency and should be treated only as a stopgap.

### CONC-002 — The native browser has one reply mailbox for all callers and surfaces

**Severity:** High
**Affected path:** embedded/pop-out browser actions, snapshots, navigation queries, and find-in-page

The Tauri browser host uses four globals of the form `Mutex<Option<oneshot::Sender<_>>>`: `SNAPSHOT_TX`, `NAV_STATE_TX`, `FIND_TX`, and `ACT_TX` (`apps/medousa-home/src-tauri/src/human_browser.rs:56`, `:325-326`, `:2649`). A command creates a channel and overwrites the corresponding singleton before evaluating JavaScript (`:2740-2776`, `:2893-2977`). The JavaScript callback takes whichever sender is present.

Two snapshots, acts, or finds in flight do not queue. The second caller drops the first sender, causing the first request to fail, and the first browser callback can then satisfy the second caller with the wrong result. Embedded and pop-out navigation/find commands even share the same slot, so two distinct webviews can cross-deliver replies. A timeout does not remove its sender conditionally, allowing a late callback to collide with a later request.

#### Recommended correction

- Generate a request ID, include it in the evaluated script and callback payload, and store senders in a bounded `HashMap<RequestId, Sender>` (ideally scoped by webview/surface).
- Remove the exact request entry on completion, evaluation failure, cancellation, and timeout.
- If a command semantically permits only one in-flight operation per webview, enforce that with a per-webview semaphore and return an explicit busy error instead of silently replacing a waiter.
- Test overlapping embed/pop-out requests and reversed callback order.

### TYPE-001 — The stream event type encodes a sum type as strings plus nullable soup

**Severity:** High
**Affected path:** engine/public API/SDK/Tauri/UI contract and every stream consumer

`InteractiveTurnStreamEvent` has a required `event_type: String`, `phase: String`, generic `message: String`, and roughly thirty optional fields representing mutually exclusive payload variants (`crates/medousa-types/src/daemon_api.rs:1688-1776`). A content delta can legally carry a permission request, terminal flag, UI scene, browser challenge, tool result, and final text simultaneously. The compiler cannot prohibit impossible states or require the payload that a particular event kind needs.

Constructors allocate the discriminator and phase as owned strings and initialize the entire nullable envelope (`src/interactive_turn_runtime.rs:643-701`). Consumers then rediscover variants with string comparisons—at least 35 production comparisons in the audited Rust/TypeScript surfaces—including fallback checks that treat either `event_type` or `phase` as the discriminator (`apps/medousa-home/src/lib/utils/streamEvents.ts:4-93`). This is both PERF-001's constant-string allocation source and a protocol-correctness problem: typos compile, exhaustiveness is impossible, and semantics are duplicated across clients.

The repository already generates a TypeScript definition from the Rust schema (`apps/medousa-home/src/lib/types/generated/daemon_api.ts:51-88`), but `apps/medousa-home/src/lib/types/chat.ts:129-184` manually declares the same interface again and only points at the generated contract in a comment. That defeats the generator's main value and creates a silent drift surface.

#### Recommended correction

- Model events as a serde-tagged enum with variant-specific payloads, for example `{ seq, turn_id, emitted_at, event: { type: "content_delta", delta } }`. Use enums for bounded phase/status/mode fields; reserve `String` for genuinely open-ended text or third-party values.
- Separate the small common envelope from variant data. Internal hot-path events can borrow/share IDs and use static enum discriminants, converting to the public representation once.
- Generate and import the TypeScript discriminated union directly. Delete the handwritten duplicate and make switch exhaustiveness fail the frontend build when a variant is added.
- If wire compatibility prevents an immediate replacement, introduce a v2 stream and an explicit adapter. In v1, at least use Rust enums with `serde(rename_all)` and validated constructors so arbitrary strings cannot enter internally.
- Add schema round-trip tests across Rust, generated TypeScript fixtures, Python, and SDK reconnect logic for every variant.

### STORE-002 — The workspace “debouncer” receives already-built whole-database snapshots

**Severity:** High
**Affected path:** worker streaming, ask jobs, workspace cards, and daemon shutdown

The workspace persistence actor looks sensible at first glance: it retains only the newest snapshot of each category and writes after a 1.5-second quiet period (`src/workspace/persist.rs:128-176`). The expensive work happens before that actor can debounce it.

`TurnWorkerStore::update` mutates one record, clones that record for its return value, releases the lock, reacquires the complete records map, prunes it, and pretty-serializes the entire map while holding the mutex (`src/agent_runtime/turn_worker/store.rs:224-237`, `:364-375`). Only the finished `String` is sent to the writer. Superseded queued snapshots therefore still consumed a full-map traversal, JSON allocation, formatting pass, and channel transfer. `AskJobStore` repeats the same pattern (`src/workspace/ask_job_store.rs:113-134`).

The streaming worker sink makes the mismatch explicit. It buffers only 400 characters or 250 ms because “every `store.update` rewrites `turn_workers.json`” (`src/agent_runtime/turn_worker_job.rs:242-305`). Each flush modifies at most two small tails, but can clone and serialize hundreds of active/completed worker records. Up to four full snapshots per second per worker are constructed even though the persistence actor will throw most away. The 4,000-character tail cap itself counts all Unicode scalars and reconstructs the retained suffix when full (`:574-580`).

Backpressure is inverted. The bounded channel uses `try_send`; when it is full, closed, or not initialized, `try_enqueue` executes synchronous filesystem writes on the caller thread (`src/workspace/persist.rs:91-101`, `:223-255`). An overloaded async runtime therefore responds by blocking the producer. Snapshot writes use direct truncate/write, so a crash can corrupt the only snapshot. `flush_persist_writer` acknowledges that processing finished, not that every write succeeded (`:80-89`, `:181-199`). Errors are printed and erased.

#### Recommended correction

- Send typed mutations or a dirty-category signal to the owner actor. The actor should clone/serialize the current state once when the debounce expires; callers should never manufacture snapshots that are designed to be superseded.
- Prefer one writer-owned state machine, per-record files, an append journal plus periodic compaction, or a small transactional store. A worker delta should be O(delta), not O(all workers).
- Keep output in a byte/character ring with a cursor rather than recounting and rebuilding the full tail.
- Remove synchronous fallback from async request/stream paths. Define bounded admission and explicit degradation: await capacity, drop a replaceable dirty notification, or return a visible persistence error.
- Commit snapshots by temporary file plus atomic rename, and make flush return a `Result` that includes the last durable generation.
- Benchmark 1, 100, and 500 worker records with 1–20 concurrent streams. Count bytes allocated/serialized versus bytes of new output and assert producer p99 latency remains bounded when the writer is slow.

### PERF-004 — A “safe boundary” fingerprints the whole Git worktree and rewrites the whole checkpoint

**Severity:** Critical
**Affected path:** every Coder start, model/tool boundary, status transition, and resume check

Coder recovery is implemented as a heavyweight repository audit in the synchronous control path. `persist_current`, `persist_boundary`, `mark_status`, and `latest_safe_resume` all call `refresh_runtime_metadata` (`src/agent_runtime/coder_turn_checkpoint.rs:687-799`). One refresh:

1. loads the Forge work item—which already has the full-log replay costs in PERF-002—and resolves the attempt environment;
2. canonicalizes the worktree and asks Git for its root, branch, HEAD, and complete porcelain status;
3. formats every status entry into a `Vec<String>` and joins it;
4. asks Git for a full binary worktree diff from HEAD and hashes the result; and
5. opens and hashes the complete contents of every untracked regular file (`src/agent_runtime/coder_turn_checkpoint.rs:864-985`).

This is done even for boundaries that did not mutate the filesystem. A large ignored mistake, generated archive, model, video, dependency tree, or untracked build output can turn a single checkpoint into gigabytes of reads. The binary diff is also allocated as a complete value before hashing (`:909-918`). The checkpoint then clones/redacts/bounds a potentially large transcript and invocation history and pretty-serializes the complete object. Size bounding repeatedly serializes collections and removes their first element until they fit (`:1049-1162`, `:1307`), combining O(n²) shifting with repeated JSON work. The allowed snapshot is hundreds of KiB, so this is not small bookkeeping.

Correct recovery fencing is valuable. Recomputing an exact repository digest at every logical boundary is not. It couples tool-loop responsiveness to repository size and makes a non-mutating model round pay for Git and filesystem work. Because these operations are synchronous, it also compounds ASYNC-001 when invoked from async orchestration.

#### Recommended correction

1. Separate logical checkpoint state from workspace-integrity observation. Persist the cheap logical delta at each boundary; refresh the repository fingerprint only after tools that may mutate it, on an explicit watcher generation change, and at resume validation.
2. Cache observation by Git HEAD/index identity plus a dirty-generation counter. Track changed paths incrementally; cache untracked hashes by canonical path, size, high-resolution mtime, and file identity, with an explicit conservative invalidation policy.
3. Stream diffs into the hasher. Do not allocate a full binary patch merely to hash it, and cap/reject pathological untracked files with a documented recovery fallback rather than silently reading infinity.
4. Journal checkpoint deltas or use a transactional record store. Maintain byte counts while appending and use `VecDeque`/ring retention; do not discover size by repeatedly serializing and front-removing.
5. Run blocking Git/filesystem observation in one bounded worker, coalesce duplicate refreshes, and make cancellation/timeout behavior explicit.
6. Benchmark checkpoint latency on clean and dirty repositories at 1k, 100k, and 1m files, including a multi-gigabyte untracked file. Record Git subprocesses, bytes read/allocated, and tool-loop p99 pause.

### PERF-005 — The UI reparses and rehydrates the complete answer on every stream delta

**Severity:** Critical
**Affected path:** assistant text streaming, Markdown, code highlighting, Mermaid, Liquid embeds, and DOM updates

PERF-001 follows a delta to the frontend store. The rendering pipeline then turns the growing immutable string into quadratic work:

1. Each chat delta replaces the message object and its complete accumulated `content` string. `LiquidChatMessage` derives a new scene whenever that object changes (`apps/medousa-home/src/lib/components/chat/LiquidChatMessage.svelte:43-76`).
2. `chatMessageToScene` scans/strips the complete body, allocates new node/slot arrays and prop objects, and passes the complete Markdown string to a stable-ID prose node (`apps/medousa-home/src/lib/liquid/surfaces/chat/messageToScene.ts:60-152`). Keying preserves the Svelte component instance, but not the expensive derived work inside it.
3. `MarkdownContent` calls `renderMarkdown(content, ...)` for the entire accumulated answer in a `$derived` (`apps/medousa-home/src/lib/components/ui/MarkdownContent.svelte:44-51`). `renderMarkdown` preprocesses the whole source, parses it with Marked, applies table/resume transforms, and sanitizes the complete HTML (`apps/medousa-home/src/lib/markdown/render.ts:261-278`).
4. `{@html html}` replaces the rendered subtree. Its effect then hydrates the container again (`MarkdownContent.svelte:69-100`). Hydration destroys Liquid and draw mounts up front, scans/highlights code, invokes Mermaid, resolves images, and mounts embeds again (`apps/medousa-home/src/lib/markdown/hydrateMarkdownContainer.ts:49-86`). The fingerprint only suppresses an animation; teardown already happened.

For `n` roughly equal-sized deltas, parsing sees lengths `1d + 2d + ... + nd`: O(n²) total characters, plus sanitizer allocation and DOM churn. A long code block or diagram magnifies it. The renderer also uses module-global mutable `activeRenderOptions`, heading counts, and checkbox index to communicate with Marked callbacks (`render.ts:26-30`, `:268-278`), making reentrancy fragile even though today's calls are usually synchronous.

This path should be profiled, but its algorithm is already wrong. No JIT or clever string rope makes whole-document parsing and subtree replacement per token a reasonable streaming architecture.

#### Recommended correction

- Coalesce frontend deltas to at most one update per animation frame, matching the engine/bridge batching in PERF-001.
- While a message is streaming, render a cheap escaped text tail or an incremental block parser. Fully parse/sanitize only completed blocks and once at the terminal boundary.
- Represent the answer as stable blocks with source ranges and cached AST/HTML. Reparse only the final incomplete block; preserve prior DOM nodes, syntax highlighting, diagrams, and mounted embeds.
- Make render context explicit per call. Do not communicate options to renderer callbacks through module globals.
- Hydration should diff placeholders and hydrate only new/changed blocks. Never destroy every existing mount just because the text tail changed.
- Add a browser benchmark/profile for 1k, 10k, and 100k streamed characters with prose, fenced code, tables, Mermaid, and Liquid. Measure total parse/sanitize time, long tasks, DOM nodes replaced, mount churn, allocations, and missed frames.

### FRONT-001 — The initial route eagerly loads most of the product

**Severity:** High
**Affected path:** cold start, WebView parse/evaluation, memory, and every desktop/mobile launch

A production `npm run build` on 2026-08-12 succeeded, but the generated Vite manifest shows that the root route's static import closure—entry/start, layout node 0, page node 2, and their non-dynamic imports—contains:

| Initial static asset | Files | Minified bytes | Per-file gzip bytes |
| --- | ---: | ---: | ---: |
| JavaScript | 56 | 7,102,090 (6.77 MiB) | 2,120,493 (2.02 MiB) |
| CSS | 11 | 1,448,096 (1.38 MiB) | 189,858 (185 KiB) |

The complete generated client contains 164 JavaScript chunks totaling 11,761,808 minified bytes. The initial closure alone contains a 2,199,774-byte page/application chunk, 1,563,872 bytes named `vault.svelte`, 1,365,204 bytes named `VaultNoteWorkshop`, 890,576 bytes named `shellTabs.svelte`, 350,301 bytes named `ChatPanel`, and 320,300 bytes named `vaultCodeMirror`. These are manifest/file measurements, not source-line estimates.

`AppShell.svelte` explains the shape. It statically imports desktop and mobile shells, vault workshop and attachments, embedded and mobile browsers, import wizard, spotlight, context menus, work popovers, stores, and other surfaces before deciding what is visible (`apps/medousa-home/src/lib/components/layout/AppShell.svelte:1-33`). A hidden panel is still parsed, instantiated at module scope, and entangled through singleton-store side effects.

CSS has the same disease. The global layout stylesheet is 953,407 minified bytes. Source `app.postcss` is 15,070 lines / 399,347 bytes, and Tailwind/Skeleton is configured with all 50 shipped theme variants at build time (`apps/medousa-home/tailwind.config.ts:5`, `:50-52`; `apps/medousa-home/themes/theme-catalog.ts`). Local Tauri assets avoid WAN transfer, but WebKit/WebView still reads, parses, compiles, and retains them. Mobile and remote web clients also pay transport cost.

#### Recommended correction

1. Define real feature boundaries. Dynamically import vault editing/CodeMirror, browser workshops, export/HTML-to-canvas, complex Liquid organisms, settings subsections, wizards, and mobile-only or desktop-only shells on first use.
2. Stop importing singleton stores for side effects from the application root. Give each feature an explicit `start()`/`dispose()` lifecycle after its chunk is loaded.
3. Choose the mobile or desktop shell before loading its component graph. Shared primitives belong in a small shell core; destinations do not.
4. Generate theme tokens separately or load the selected theme at runtime. Do not ship 50 complete selector trees in the global critical stylesheet.
5. Add CI budgets from the Vite manifest: initial static JS/CSS, largest chunk, and route closure. A reasonable first ratchet is the current measured value minus each intentional split; the eventual target should be chosen from cold-start profiling, not aesthetics.
6. Profile Tauri cold start and mobile navigation with WebView tracing. Track parse/compile/evaluate time and heap after launch, not only gzip size.

### ARCH-001 — The frontend module graph contains runtime cycles measured in dozens of modules

**Severity:** High
**Affected path:** initialization order, tree-shaking, lazy loading, tests, and maintainability

A static runtime-import graph over application `.ts` and `.svelte` files (dynamic and top-level type-only imports excluded) finds seven strongly connected components. The largest contains **74 modules** spanning Markdown, the Liquid registry/components, vault/workshop stores, live editor extensions, artifact presentation, and export code. Other runtime cycles include:

- `lmeWorkspace` ↔ `shellTabs` ↔ `undertakings` ↔ `codeWorkspaceController` (4 modules);
- vault-space configuration/templates/custom spaces (3);
- human-browser API/store/surface state (3);
- `voicePresets` ↔ `workshopDefaults`;
- `identity` ↔ `userProfiles`; and
- `browserCompositor` ↔ `browserPopoverOverlay`.

One short path demonstrates how the large component forms: `MarkdownContent.svelte` imports the `$lib/markdown` barrel; that barrel exports `hydrateLiquidEmbeds`; hydration imports `LiquidMdHost.svelte`; the host side-effect-imports the entire Liquid archetype registry; the `prose` archetype imports `MarkdownContent.svelte` again (`apps/medousa-home/src/lib/markdown/index.ts`, `hydrateLiquidEmbeds.ts`, `LiquidMdHost.svelte`, `liquid/archetypes/index.ts`, `liquid/archetypes/atoms/prose/Prose.svelte`). Vault and workshop singleton stores join that component through helpers that import the stores back.

ES modules can execute many cycles, but “it currently initializes” is not a design property. A cycle makes values sensitive to evaluation order, obstructs feature chunking, broadens test setup, and lets a pure-looking helper pull in process-wide reactive state. The 7.10 MB initial closure in FRONT-001 is the build artifact of these ownership failures.

#### Recommended correction

- Make dependency direction explicit: domain types/pure transforms → services/ports → stores → components. Lower layers must never import a singleton store or UI component.
- Split the Liquid descriptor/schema registry from component loaders. Register component factories lazily by feature; do not use one side-effect barrel to install the vocabulary.
- Have Markdown accept embed renderers/resolvers as injected adapters. A prose renderer must not import the registry that eventually imports prose.
- Move cross-store actions into an orchestration service with passed interfaces, or expose commands/events; do not solve coordination by importing singleton A from B and singleton B from A.
- Enforce zero new runtime cycles with a checked-in dependency graph rule (`dependency-cruiser`, Madge, or an equivalent Vite-aware check), then break existing SCCs from smallest to largest.

### PERF-006 — Vault UI code rebuilds global indexes inside every node and every wikilink

**Severity:** High
**Affected path:** large vault tree selection, previews, and wikilink rendering

The backend vault scans are only half of PERF-003. The UI repeatedly reconstructs the indexes it already conceptually owns:

- Every recursive `VaultTreeNode` instance implements `treeNodeContainsPath` by walking its complete subtree. On a selection change, every mounted node's effect can perform that traversal (`apps/medousa-home/src/lib/components/vault/VaultTreeNode.svelte:52-76`). Across a fully expanded tree, the sum of subtree sizes is O(n²) in a degenerate tree.
- Every node also derives `new Set(vault.notes.map(note => note.path))`, even leaf rows that never use recent-folder logic (`VaultTreeNode.svelte:133-159`). A notes update can allocate one full-vault set per mounted row.
- `VaultMarkdownPreview` constructs the same full path set in its render options (`apps/medousa-home/src/lib/components/vault/VaultMarkdownPreview.svelte:57-64`).
- For each wikilink, the Markdown renderer converts that set into a freshly allocated array of fake `VaultNote` objects (`apps/medousa-home/src/lib/markdown/render.ts:32-57`). `resolveWikilinkTarget` then maps it back into a path array, builds another `Set`, scans filenames, and scans note titles (`apps/medousa-home/src/lib/utils/resolveWikilink.ts:47-93`). One note with `L` links in a vault of `N` notes performs O(L×N) allocation/scanning per render—and a streaming/edited preview can render repeatedly.

#### Recommended correction

- Build one immutable vault lookup snapshot per vault generation: normalized path set, filename-stem multimap, folded-title index, parent/ancestor table, and note metadata by path. Pass a reference or generation handle, not `VaultNote[]` stubs.
- Precompute the selected path's ancestor set once. A tree row should answer “am I an ancestor?” with O(1) lookup. Flatten and virtualize the visible tree so collapsed/offscreen nodes do no reactive work.
- Hoist the shared path set out of recursive row components. Compute recent rows only for folders that are visible/expanded.
- Make wikilink resolution accept the precomputed maps and return deterministic ambiguity information. Do not allocate a note corpus for every link.
- Benchmark selecting and editing in 100, 10k, and 100k-note vaults, with deeply nested and wide trees and a link-heavy note.

### ARCH-002 — Mega-modules have erased ownership and review boundaries

**Severity:** High
**Affected path:** daemon API, Coder, tools, Forge, Home stores/editors, desktop commands, and global styling

Line count alone is not a defect. This repository's outliers are not cohesive generated tables, though; they are central modules with many reasons to change:

| File | Lines |
| --- | ---: |
| `apps/medousa-home/src/app.postcss` | 15,070 |
| `src/daemon/forge_api.rs` | 9,174 |
| `src/agent_runtime/coder_tools.rs` | 4,930 |
| `apps/medousa-home/src/lib/components/work/CodeSourceEditor.svelte` | 3,983 |
| `apps/medousa-home/src/lib/stores/chat.svelte.ts` | 3,700 |
| `src/bin/medousa.rs` | 3,674 |
| `crates/medousa-forge/src/forge.rs` | 3,648 |
| `crates/medousa-types/src/daemon_api.rs` | 3,524 |
| `src/tools.rs` | 3,078 |
| `apps/medousa-home/src-tauri/src/human_browser.rs` | 2,992 |
| `apps/medousa-home/src/lib/components/work/UndertakingsPanel.svelte` | 2,923 |
| `apps/medousa-home/src/lib/stores/vault.svelte.ts` | 2,904 |

The Tauri root adds a 336-line `generate_handler!` registry and the crate declares 427 commands. The chat store combines transport sequencing, stream interpretation, persistence, UI message shaping, tool/artifact state, error handling, and reactive ownership. `forge_api.rs` combines a broad HTTP surface with domain orchestration and blocking execution. `coder_tools.rs` combines schemas, dispatch, policy, execution, persistence, and tests. `app.postcss` is effectively an unscoped global UI subsystem.

The consequence is visible throughout this review. A stream DTO grows nullable fields because no variant owner exists (TYPE-001); all text events enter one giant interpreter (PERF-001); Forge handlers can call blocking internals because HTTP and domain boundaries are mixed (ASYNC-001); browser callback permissions cannot be reviewed next to a bounded command surface (DESKTOP-001); and feature CSS/modules enter the initial route because ownership is global (FRONT-001/ARCH-001).

Splitting files mechanically would only create a directory full of mutual imports. The required fix is to split authority and state ownership.

#### Recommended correction

- Define bounded modules around invariants: authenticated route groups, turn sequencing actor, Forge repository/event store, Coder checkpoint service, browser bridge, vault index, chat transcript reducer, and feature-scoped UI/CSS.
- Give each boundary a small typed public interface and private state. Move tests beside that interface and forbid sibling modules from reaching internal globals.
- Replace giant command/event lists with registration by feature and generated inventories that security/contract tests can enumerate.
- Extract pure reducers/transforms from reactive Svelte stores and keep lifecycle/transport adapters thin. Split large components along independently loaded features, not arbitrary visual fragments.
- Add dependency-direction and file/module ownership checks. Use size thresholds as a review alarm, not an absolute law; an exception should explain why the module has one reason to change.

### CONTRACT-001 — The “single source of truth” is one more hand-maintained copy

**Severity:** High
**Affected path:** daemon API, Rust/Python SDKs, Tauri proxy, Home transport, and API documentation

`sdk-contract/manifest.yaml` says it is the single source of truth for Rust and Python accessor parity. It contains 105 method entries with HTTP methods, paths, request types, response types, streaming, and sync metadata. None of the SDK clients is generated from it. The repository instead maintains:

- handwritten Rust SDK route strings across roughly 4,786 lines;
- handwritten async and sync Python clients across roughly 6,560 lines;
- a handwritten 115-entry `PARITY_ROUTES` constant in `crates/medousa-sdk/tests/contract_parity.rs`, explicitly claiming it must match sources and docs but never comparing itself to either;
- a 4,622-line Tauri daemon proxy layer across 34 files plus a 2,135-line TypeScript bridge; and
- the YAML manifest itself.

The copies have already diverged structurally. The Rust parity table contains session agent-mode/code-binding routes absent from the YAML and uses different placeholder names (`{id}` versus `{session_id}`, `{model_id}`, or `{turn_id}`). That may be intentional coverage beyond the public contract; the tests cannot tell.

The CI checker does not validate the route metadata that makes a contract useful. `scripts/check-sdk-contract.sh` parses the YAML, maps each accessor to a guessed source filename, then uses `grep` to ask whether a function with that name appears (`scripts/check-sdk-contract.sh:17-100`, `:102-149`). It never checks the HTTP verb, path, placeholder encoding, request/response type, streaming behavior, sync parity, or the daemon router. The Python parity test likewise only calls `getattr` and then verifies that the manifest's own path begins with `/` (`python/medousa-sdk/tests/test_parity_paths.py:24-87`). A method can send the wrong request to the wrong route with the wrong schema and every “parity” check passes.

This is duplication with a correctness tax, not mere verbosity. Path construction already accepts raw trimmed IDs in several SDKs, which compounds SEC-002 and makes encoding behavior differ by language.

#### Recommended correction

1. Make one machine-readable API description authoritative. OpenAPI plus explicit SSE extensions is a reasonable fit; the existing manifest can work if it gains schemas and generation tooling.
2. Generate route constants, parameter encoders, DTO bindings/adapters, mock-server expectations, and the parity matrix. Handwritten ergonomic methods may wrap generated transport calls, but must not restate verbs and paths.
3. Compare the authoritative description to the actual Axum router in a black-box contract suite. Exercise every method with awkward path/query values and validate request and response bodies, status codes, SSE framing/reconnect, and authentication requirements.
4. Either generate the Tauri bridge or delete the per-endpoint proxy in favor of the Rust SDK/one generic typed transport. Do not maintain a fourth HTTP client inside the same product.
5. Delete regex existence checks once generated artifacts and executable contract tests cover the invariant. A green grep is false confidence.

### DEP-001 — The daemon's dependency graph is an unmanaged product surface

**Severity:** High
**Affected path:** clean builds, linker work, security review, updates, binary/package size, and release reliability

`cargo tree -p medousa -e normal --no-dedupe`, reduced to unique name/version pairs, contains **932 packages**. Ninety-three crate names occur at multiple versions. Notable simultaneous stacks include `reqwest` 0.11/0.12/0.13, `rustls` 0.21/0.22/0.23, `tokio-tungstenite`/`tungstenite` 0.21/0.28/0.29, `tonic` 0.12/0.14, `genai` 0.5/0.6, `schemars` 0.8/1.2, and `sysinfo` 0.33/0.37.

Some complexity is inherent in a product with local inference, Git, browser, SDK, calendar, document parsing, adapters, and observability. The root package still directly depends on `teloxide`, `serenity`, and `slack-morphism` (`Cargo.toml:99-101`) even though repository search finds no use of those crates in the root `src`; separate adapter packages own those frameworks. `cargo tree -i` confirms each remains a direct normal dependency of `medousa`. Root feature selection also enables broad bundles such as `stasis-rs` with `grapheme-full` alongside direct Grapheme compiler/runtime/LSP dependencies (`Cargo.toml:90-94`).

Dead-code elimination may keep some unused code out of the final executable, so this review does not pretend 932 packages equals 932 packages shipped byte-for-byte. Cargo still resolves, downloads, compiles/checks, audits, caches, and must update the graph; build scripts and proc macros still run; duplicate TLS/HTTP stacks enlarge the compatibility and vulnerability matrix. The Tauri check even reports vendored Grapheme patches that are not used and a future-incompatible `block` crate, concrete evidence that ownership is already fuzzy.

#### Recommended correction

- Remove unused direct root dependencies immediately. Move optional subsystems behind narrow features or separate binaries/crates so the personal daemon does not compile every integration surface.
- Run `cargo machete`/`udeps` with reviewed exceptions, `cargo deny` for bans/duplicates/advisories/licenses/sources, and a dependency diff on pull requests.
- Choose and enforce one supported HTTP/TLS/WebSocket generation where upstream constraints permit. Record unavoidable duplicate-version exceptions with owners and expiry conditions.
- Measure clean build time, incremental build time, release binary/app size, and link peak memory per feature set. Establish budgets and publish a feature-to-dependency map.
- Avoid “full” umbrella features in the primary binary. Make compiler, LSP, local-model, adapter, and export workloads pay-for-play packages aligned with Medousa's Home-first package model.

### CI-001 — CI omits the tests that are currently red

**Severity:** High
**Affected path:** pull requests, releases, desktop/mobile packaging, docs, and regressions

The Home CI job installs dependencies and runs only `npm run check` (`.github/workflows/ci.yml:35-54`). It does not run `npm test`, `npm run build`, or compile the Tauri crate. Locally, type checking passes but the omitted Vitest suite reports **1,201 passing and 3 failing tests** across 224 files. The keyboard shortcut catalog gained a `review` group while its exact snapshots still expect the old six groups (`apps/medousa-home/src/lib/utils/keyboardShortcutsCatalog.ts:4-11`, `:102-115`; `keyboardShortcutsCatalog.test.ts:13-32`, `:59-109`). The committed generated commands appendix is also stale (`apps/medousa-home/src/lib/guide/loadGuide.test.ts:77-88`). Those are ordinary change-synchronization failures that a required test job would have caught.

The Rust job runs warning-denying workspace Clippy but tests only `cargo test -p medousa --lib` (`.github/workflows/ci.yml:19-33`). It does not run tests for most workspace crates as a workspace, nor the Tauri and installer crates, which are outside the root workspace. The docs job is explicitly `continue-on-error: true` (`:123-129`), turning canonical-document drift into decoration. There is no PR matrix for the macOS/Windows-specific desktop code, no packaged-app smoke/security test, no bundle budget, and no performance regression gate.

A local `cargo check --manifest-path apps/medousa-home/src-tauri/Cargo.toml --all-targets` succeeds but emits 50 library warnings, unused/dead proxy helpers, unused Grapheme patches, and a future-incompatibility warning. That code does not satisfy the root's `-D warnings` standard because CI never subjects it to one.

#### Recommended correction

- Require Home unit tests and production build. Make generated guides/catalogs deterministic build artifacts and fail on diff.
- Compile, lint, and test both Tauri apps on supported desktop OSes; add minimal mobile checks where runners/toolchains permit. Run a packaged-app browser ACL smoke test for DESKTOP-001.
- Test the workspace deliberately: enumerate crates excluded for cost/platform reasons and give each a separate required job. “Clippy compiled it once” is not a test strategy.
- Make canonical docs checks required after fixing noise. Add dependency/advisory checks and manifest-based initial-bundle budgets.
- Add a fast pull-request tier and broader nightly/release tier, but make every release-blocking invariant required somewhere before artifacts are published.

### TEST-001 — Unit tests reach into the user's machine and race one another

**Severity:** High
**Affected path:** required Rust CI suite and developer feedback

The required `cargo test -p medousa --lib` run did not complete in this audit. With 1,096 tests started, `tui::workshop_connection::tests::remember_and_active_round_trip` failed during the parallel suite, then `inference_router::tests::openai_codex_requires_oauth_not_api_key` ran for more than 60 seconds until the suite was terminated.

The first failure is a textbook non-hermetic race. The test changes the process-global `MEDOUSA_DATA_DIR`, performs filesystem work, then restores it without a shared cross-module lock (`src/tui/workshop_connection.rs:377-395`). Numerous other tests change the same variable. The test passes immediately in isolation, which is strong evidence of cross-test interference rather than a deterministic product failure.

The hanging “credential requirement” unit test calls `target_ineligibility_reason`, which calls `session::chatgpt_oauth_configured`, which calls the real singleton OAuth broker and `DaemonCredentialStore` (`src/inference_router.rs:165-180`, `src/session.rs:359-361`, `src/chatgpt_oauth.rs:778-786`). That broker can consult the host OS keyring and its answer depends on whether the developer is signed in. The test expects “missing OAuth,” so a real credential also changes its semantics. In isolation it still failed to finish within ten seconds and was terminated.

Tests may need integration access, but tests labeled as small pure policy checks must not open a keyring prompt or share mutable environment with hundreds of peers. A required suite that intermittently fails or hangs trains maintainers to rerun/ignore it—the exact opposite of a safety net.

#### Recommended correction

- Inject credential availability into inference eligibility. Unit-test the pure decision with a fake boolean/provider; integration-test the broker against an in-memory credential store.
- Centralize environment overrides behind one RAII guard and one suite-wide lock, or better pass a data-root configuration into stores. Never mutate environment variables in parallel unit tests.
- Mark the small number of genuine OS keyring/network/GUI tests ignored or feature-gated and run them in a serialized, isolated integration job with explicit timeouts.
- Add per-test and suite timeouts, deterministic temp roots, and a CI mode that fails on access to the real home/keyring/network unless explicitly allowed.
- Run the full suite repeatedly under high parallelism after the cleanup; isolation should not be established by `--test-threads=1` alone.

### PERF-007 — Performance is discussed but not governed

**Severity:** High
**Affected path:** all critical latency, memory, persistence, and startup paths in this review

The repository contains useful one-off scripts for local inference and semantic typing, plus a `medousa_local_bench` executable. It has no Criterion benches or equivalent repeatable harness for the daemon token stream, feed store, Forge replay/mutation, Coder checkpoints, vault scan/index/link resolution, workspace persistence, Home streaming render, or app startup/bundle evaluation. CI records no latency, allocation, syscall, queue-depth, resident-memory, file-size, or bundle regression budget.

Consequently, performance-sensitive changes are argued from taste and local perception. That is how obviously global work survives on hot paths: there is no executable definition of “too slow.” It also makes optimization dangerous—someone can remove a clone and celebrate while the same request still rewrites a file or reparses the whole document.

#### Recommended correction

Build a small performance suite around the findings, not a vanity benchmark collection:

- token streaming: time-to-first-delta, p50/p99 sink latency, allocations, journal syscalls, queue high-water, UI long tasks, and cancellation latency;
- stores: append/update cost versus retained record count, durability sync policy, and concurrent lost-update tests;
- Forge/Coder: log length, repository file count/dirty bytes, subprocess count, checkpoint pause, and recovery validation;
- vault: cold/warm reads and writes across 100/10k/100k notes, symlink policy, link-heavy rendering, and expanded-tree interaction;
- desktop: cold start, initial static asset closure, WebView parse/evaluate time, heap, and first-interaction latency.

Check stable microbenchmarks on PRs with conservative ratchets; run noisy browser/end-to-end profiles nightly on pinned hardware and retain time series. Couple every performance fix to a correctness/durability test so “fast” does not become “quietly loses data.”

### DATA-001 — Session deletion tries to remove a file as a directory

**Severity:** Medium
**Affected path:** privacy/data cleanup and repeated session IDs

`delete_session` calls `remove_turn_ledger_dir` (`src/session_lifecycle.rs:78-83`). That helper obtains `turn_ledger_path(session_id)` and passes it to `std::fs::remove_dir_all` (`:102-105`). `turn_ledger_path` returns `turn_ledger/<sanitized-id>.jsonl`, a file, not a directory (`src/agent_runtime/turn_ledger.rs:232-250`). The ignored error leaves the ledger behind while the API returns `deleted: true` unconditionally.

Use `remove_file` for the exact validated path and propagate/report failures. More importantly, define deletion as an enumerated, testable data contract: create every session satellite, delete, then assert from a fresh process/store that no transcript, catalog, metadata, artifact, media, verification, ledger, channel reference, or memory node remains. SEC-002 must be fixed first so deletion never expands beyond that exact inventory.

## Remediation order

Do not start with allocation whack-a-mole. The work has dependency order:

1. **Contain authority now.** Disable non-loopback personal-mode exposure until SEC-001 has mandatory authentication and a reduced router. Fix `SessionId`/filesystem boundaries (SEC-002/SEC-003). Remove `core:default` from remote browser pages and build a minimal tested bridge (DESKTOP-001). These are release-boundary issues.
2. **Make correctness failures impossible to report as success.** Repair session durability accounting, serialize feed mutations, make vault compare-and-write atomic, remove per-turn/global response state, and make deletion complete (DUR-001, STORE-001, CONSIST-001, CONC-001/002, DATA-001). Add adversarial/concurrent tests before optimizing these paths.
3. **Replace the token pipeline as one unit.** One bounded turn actor should batch deltas, own sequencing, buffer journal I/O, maintain a bounded replay window, and publish typed batches. The Tauri/TypeScript/UI side should parse once and render stable completed blocks plus one streaming tail (PERF-001, MEM-002, PERF-005, TYPE-001). Fixing only one layer will hide cost in the next.
4. **Stop global persistence work.** Give Feed, Forge, checkpoints, workspaces, and the vault transactional/incremental storage owners. Move blocking work off async executors and put hard limits around replay, untracked-file hashing, retained runs, and snapshots (PERF-002/003/004, STORE-002, MEM-001, ASYNC-001).
5. **Enforce the architecture.** Generate API clients/contracts, break frontend cycles, load features lazily, split authority-owning mega-modules, trim dependencies, and make the full tests/build/Tauri matrix required (ARCH-001/002, CONTRACT-001, FRONT-001, DEP-001, CI-001, TEST-001).
6. **Then tune.** Establish the benchmark/profile baselines in PERF-007, implement the structural changes, and ratchet budgets. Optimize demonstrated remaining allocation/copy hotspots after the O(n), O(n²), blocking-I/O, and unbounded-retention defects are gone.

The first milestone should be a boring secure spine: authenticated routes, validated IDs, bounded queues, one durable event owner, deterministic tests. Feature work on top of the current spine compounds migration cost.

## Verification record

Commands were run from the repository root on macOS on 2026-08-12 unless a working directory is shown.

| Check | Result | Relevant evidence |
| --- | --- | --- |
| `cargo clippy --workspace --all-targets --exclude medousa-sdk-iroh -- -D warnings` | Pass | Official root Clippy target completed cleanly. |
| `cargo test -p medousa --lib` | Did not complete | 1,096 tests started; one parallel-only failure occurred and an OAuth/keyring-dependent test exceeded 60 seconds. The suite was terminated. See TEST-001. |
| Isolated `remember_and_active_round_trip` | Pass | Completed immediately, unlike the parallel suite failure; supports the global-environment race diagnosis. |
| Isolated `openai_codex_requires_oauth_not_api_key` | Hung | Exceeded 10 seconds and was terminated; code trace reaches the real singleton credential store. |
| `npm run check` in `apps/medousa-home` | Pass | Svelte check reported 0 errors and 0 warnings. |
| `npm run build` in `apps/medousa-home` | Pass with warnings | Production build completed in about 28 seconds; repeated theme external-dependency and chunk-size warnings. Manifest measurements support FRONT-001. |
| `npm test` in `apps/medousa-home` | Fail | 224 files / 1,204 tests: 1,201 pass, 3 fail in two files. See CI-001. |
| `cargo check --manifest-path apps/medousa-home/src-tauri/Cargo.toml --all-targets` | Pass with warnings | 50 library warnings, unused Grapheme patches, and a future-incompatible dependency warning. |
| `bash scripts/verify-docs.sh --strict` | Pass | Strict documentation consistency checks completed. |
| `PYTHON=python/medousa-sdk/.venv/bin/python bash scripts/check-sdk-contract.sh` | Pass, weak invariant | The script passes when PyYAML is available; CONTRACT-001 explains why that does not establish route parity. |
| Cargo dependency inventory | Measured | 932 unique normal name/version pairs and 93 names at multiple versions for `medousa`. |
| Vite manifest/import graph analysis | Measured | 7.10 MB initial static JS, 1.45 MB CSS, and seven runtime SCCs; largest SCC has 74 modules. |

No production code was changed as part of this audit. The test failures and warnings pre-existed the review.

## Method and limits

This first pass covered the tracked engine/crates, daemon/router, agent runtime and streaming spine, session/artifact persistence, Forge/Coder paths, vault backend and UI, Home stores/components/build output, Tauri transport/browser capabilities, SDKs/contracts, adapters/integrations at inventory level, CI, docs checks, and dependency graphs. It combined request-path tracing, repository-wide pattern searches, source-size/dependency inventories, a static ESM strongly-connected-component analysis, production bundle-manifest measurement, and the checks above.

“Repo-wide” does not mean every line received equal manual scrutiny, and static evidence is not a substitute for exploitation or profiling. This was not a formal cryptographic review, third-party dependency vulnerability audit, fuzzing campaign, mobile-device test, Windows/Linux packaged-app test, or hardware-controlled performance study. Findings labeled “benchmark pending” deliberately state an algorithmic or I/O amplification visible in code without fabricating a speedup number. Findings involving ACL/runtime integration identify the locked-version policy and still call for packaged cross-platform confirmation.

## Final assessment

Medousa has ambitious product breadth and several sound primitives, but the current codebase is operating beyond the scale its ownership model can safely support. The dominant defect is **amplification**: tiny events trigger whole-state work, ordinary strings carry authority, and local modules reach process-wide state. That produces the observed mix of latency, heap growth, lost updates, races, security gaps, huge bundles, brittle tests, and duplicated contracts.

The code is not “dumb”; it is insufficiently bounded. The fix is not cleverer syntax. Establish owners for authority, sequencing, storage, and feature lifecycles; make illegal states unrepresentable; bound every queue and retention policy; generate repeated contracts; and make performance/security invariants executable in CI. Until then, adding features will keep making unrelated paths slower and harder to trust.
