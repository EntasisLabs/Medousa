# Home Code workbench parity plan

> **Status:** Active (started 2026-08-06)
>
> **Scope:** Desktop Medousa shell, Code, Review, Terminal, Forge, and workshop
> coding services
>
> **Baseline:** Current Visual Studio Code behavioral workflows, not a visual
> clone
>
> **Related:** [Code flow-state roadmap](code-flowstate-roadmap.md),
> [Code surface bridge](code-surface-bridge-plan.md),
> [Coding engine orchestrator](coding-engine-orchestrator.md), and
> [Coding session terminal](coding-session-terminal.md)

## Objective

An engineer who is fluent in VS Code should be able to enter Code in Medousa
and keep their existing coding instincts:

- commands finish the operation their labels promise;
- files, locations, diagnostics, changes, tasks, tests, and debug state remain
  visible and navigable;
- keyboard-first workflows have familiar defaults and can be remapped;
- local and remote workshops behave the same because the workshop daemon owns
  files, tools, language servers, terminals, tasks, tests, and debuggers;
- leaving Code returns to the broader Medousa workshop without discarding
  coding state.

This is behavioral parity. Medousa remains a user-domain workspace whose peers
include Chat, Notes, Browser, Code, Review, Terminal, and Projects. Code does
not become the product's organizing metaphor.

## Product invariants

1. Filesystem authority follows the connected workshop daemon. Remote Code
   never uses a Home-device picker, upload flow, `convertFileSrc`, or local
   Reveal action.
2. Forge retains custody of coding work. Human, agent, Terminal, task, test,
   and debugger changes meet in the same governed working copy and Review.
3. Familiar UI is not allowed to overstate capability. An unavailable command
   is disabled with a repair path; an enabled command completes across files,
   panes, and remote boundaries.
4. Code-specific chrome is contextual and restorable. Permanent IDE panels are
   optional, not imposed on Chat, Notes, Browser, or other Home surfaces.
5. Language, task, test, debug, and terminal processes run on the workshop.
   Home presents and controls them through versioned daemon contracts.
6. Every slice is independently testable and committed. A major checkpoint
   must be revertible without taking later unrelated behavior with it.

## Completion definition

The bridge is complete only when all acceptance journeys at the end of this
document pass on a co-located workshop and a remote workshop, the status matrix
contains no required incomplete row, user-facing docs describe the shipped
behavior, and the repository CI-parity gates pass.

Passing a component test or exposing a button is not completion evidence for a
workflow. Evidence must exercise the operation through its authoritative
boundary: Home UI, daemon API or stream, workshop process, persisted state, and
recovery where applicable.

## Current baseline

### Strengths to preserve

- Daemon-owned local/remote repository discovery, recent and pinned
  repositories, provider-backed clone, and explicit dirty/trust explanation.
- Digest-fenced writes, durable drafts, conflict preservation, and controlled
  human/agent handoff.
- CodeMirror editing with folding, multiple selections, find, completion,
  hover, signature help, diagnostics, rename, formatting, references, and
  workspace symbols when their providers are usable.
- Shared shell tabs, binary split trees, virtual desktops, and daemon-owned PTY
  sessions.
- Forge Review synthesis, provenance, attempts, risk, verification, provider
  handoff, and explicit finish decisions.

### Trust gaps to close first

- Debugging is absent. Source-control operations, deep test state, and editor
  contribution points remain below the daily-driver baseline. A full keybinding
  editor and context-key `when` clauses remain deferred. Custom terminal
  profiles, OSC shell integration, task→PTY attach, and remote Browser handoff
  remain ahead (HCP-7D+).

## Target architecture

### Home workbench

`CodeWorkbenchState` owns the contextual Code posture:

- editor groups and group-local visible tabs;
- active and most-recently-used editors;
- one global code-location history;
- optional Explorer, Search, Changes, Problems, Output, Terminal, Tests, and
  Debug regions;
- a project-scoped layout restored independently of the broader Home desktop;
- commands, menus, context keys, and user keybindings.

It composes with `shellTabs`; it does not create a second window manager. A
Code resource can still sit beside Chat, Browser, Review, or Terminal in a Home
split.

### Editor workspace adapter

`MedousaCodeWorkspace` implements the CodeMirror LSP workspace contract and is
the sole bridge between LSP URIs and governed source buffers. It must:

- request unopened files through the daemon-backed Code workspace;
- open or focus the target editor and return its `EditorView`;
- retain multiple views of one document without duplicate `didOpen` traffic;
- apply text edits and create/rename/delete resource operations through
  digest-fenced Forge batch APIs;
- preview multi-file refactors before application when requested;
- keep URI, document version, and source digest mappings explicit;
- expose navigation completion/failure instead of silently dropping a target.

### Workshop project intelligence

The coding engine owns one recoverable project-language session per resolved
language root. A session contract includes:

- root resolution from the document through registered markers;
- lifecycle state, health, restart, logs, progress, and server configuration;
- workspace diagnostics and symbols;
- file-operation and watched-file notifications;
- semantic capabilities advertised to Home only when usable;
- package identity and an exact repair/install action.

Language packs bind a language id, extensions, grammar, LSP binary, default
configuration, formatter, task/test/debug contributions, and package source.
Home never calls a language “supported” from a server registry entry alone.

### Workshop events and search

The daemon exposes bounded, resumable project event streams for source create,
change, rename, delete, Git status, diagnostics, task/test/debug state, and
service health. Events carry a sequence/cursor so reconnect can reconcile from
an authoritative snapshot.

Repository search is daemon-owned and supports literal/regex, case, whole word,
multiline, include/exclude globs, ignored-file policy, changed-files scope,
tracked and untracked files, cancellation, pagination, previews, and
digest-fenced replace plans.

### Execution, tests, and debugging

`ProjectExecutionService` unifies detected and configured tasks with shared
terminal sessions. Output is streamed once, parsed incrementally by registered
problem matchers, retained with bounded replay, and summarized into Forge
evidence when the run completes.

`ProjectTestService` owns adapter discovery, stable test ids, hierarchy, state,
results, messages, coverage, run profiles, cancellation, watch mode, and debug
handoff.

`ProjectDebugService` proxies Debug Adapter Protocol sessions running on the
workshop. Home owns breakpoint presentation, launch selection, toolbar, stack,
variables, watch, console, and source navigation. Debug processes and paths
never move to the Home device for a remote workshop.

### Changes and Review

Forge Review remains the decision surface. A contextual Changes view supplies
the missing inner-loop operations:

- branch, base, upstream, ahead/behind, and conflict state;
- working changes grouped by provenance;
- syntax-aware inline/side diff, word changes, real context expansion, and
  file/hunk revert;
- a Forge-native “include in candidate” operation where staging semantics are
  needed;
- guarded fetch/pull/push/sync, commit/checkpoint, conflict resolution, blame,
  timeline, and arbitrary comparison through capability-gated commands.

### Contribution registry

Medousa does not embed the VS Code extension host. A bounded, versioned
workshop contribution contract supplies:

- commands, menus, context keys, and keybindings;
- languages, grammars, snippets, language servers, formatters, and save
  actions;
- task providers and problem matchers;
- test adapters and run profiles;
- debug adapters and launch providers;
- editor themes, icons, and project recommendations.

Packages install contributions into `{dataDir}/bin` and workshop-owned package
directories. Home reads resolved capabilities from the daemon. A useful subset
of `.vscode/settings.json`, `tasks.json`, `launch.json`, and extension
recommendations may be imported without promising VSIX compatibility.

## Delivery and commit slices

The status legend is `⬜ pending`, `🔄 active`, `✅ verified`, and `⛔ blocked`.

| Slice | Deliverable | Status |
|---|---|---|
| HCP-0 | Authoritative plan, truthful roadmap/docs, verification inventory | ✅ |
| HCP-1A | `MedousaCodeWorkspace` file/URI/view model with focused tests | ✅ |
| HCP-1B | Cross-file definition/declaration/type/implementation navigation and history | ✅ |
| HCP-1C | Complete text/resource workspace edits plus governed refactor preview | ✅ |
| HCP-2A | Real workspace Problems model and diagnostics navigation | ✅ |
| HCP-2B | Per-document language-root resolution and nested-project sessions | ✅ |
| HCP-2C | LSP lifecycle, restart/reconnect, progress, logs, and configuration | ✅ |
| HCP-3A | Svelte, JSX, and TSX grammar/LSP dogfood pack | ✅ |
| HCP-3B | Capability-derived language matrix and exact package repair | ✅ |
| HCP-3C | Remaining registered language grammar/package packs | ✅ |
| HCP-4A | Cursor-based project source/Git event stream | ✅ |
| HCP-4B | All-open-buffer reconcile, rename/delete recovery, and watched-file LSP notifications | ✅ |
| HCP-5A | Geometry-correct groups, split-with-retained-editor, and unified code history | ✅ |
| HCP-5B | Contextual Code layout preset with group-local visible tabs and optional regions | ✅ |
| HCP-5C | Shared command registry, VS Code aliases, context keys, and keybinding editor | ✅ |
| HCP-6A | Full repository search API/UI and cancellable pagination | ✅ |
| HCP-6B | Previewed repository replace and complete file/folder operations | ✅ |
| HCP-6C | Large-file, encoding, binary-preview, and fuzzy Quick Open fallbacks | ✅ |
| HCP-7A | Streaming execution protocol and bounded output replay | ✅ |
| HCP-7B | Named task terminals, background readiness, configured tasks, and problem matchers | ✅ |
| HCP-7C | Terminal search, profiles, groups, shell integration, file links, and run selection | 🔄 |
| HCP-7D | Remote service detection/proxy and Browser preview handoff | ⬜ |
| HCP-8A | Forge Changes model and branch/upstream/conflict status | ⬜ |
| HCP-8B | High-fidelity diff, real context expansion, file/hunk actions, and conflict editor | ⬜ |
| HCP-8C | Guarded Git sync/history/blame/checkpoint operations and provider continuity | ⬜ |
| HCP-9A | Test adapter contract, discovery hierarchy, stable state, and results | ⛔ |
| HCP-9B | Gutter/Explorer runs, watch, coverage, profiles, and Forge evidence | ⛔ |
| HCP-10A | Workshop DAP proxy, launch configuration, sessions, and source mapping | ⛔ |
| HCP-10B | Breakpoints, stepping, stack, variables, watch, console, and debug-test | ⛔ |
| HCP-11A | Versioned contribution registry and first-party language/tool packs | ⬜ |
| HCP-11B | Settings/keybinding/profile persistence and useful VS Code config import | ⛔ |
| HCP-11C | Multi-root/environment adapters and full remote-parity audit | ⬜ |

### Verified implementation ledger

- **HCP-1A — `83c31264`.** Added the pooled `MedousaCodeWorkspace`, canonical
  URI/file/view ownership, multi-view synchronization with explicit divergent
  draft refusal, headless reference loading, and focused tests. Migration: none.
  Compatibility: direct Grapheme callers retain the package default workspace;
  Code leases opt into the adapter. Rollback: revert this commit; no persisted
  data or daemon contract changes are involved.
- **HCP-1B — `e78c65f2`.** Added safe workshop file-URI conversion, live editor
  view registration, definition/declaration/type-definition/implementation
  requests, unopened-file presentation, and precise Back/Forward history.
  Migration: none. Compatibility: unsupported language methods remain
  capability-gated and existing in-file navigation stays available. Rollback:
  revert this commit; no persisted data or daemon contract changes are involved.
- **HCP-1C — `ca1a815b`.** Added complete LSP workspace-edit parsing, a
  before/after Review Refactor surface, and one digest-fenced Forge transaction
  for ordered text, create, rename, and delete operations with rollback.
  Migration: none. Compatibility: older daemons retain the text-only batch
  fallback; resource operations require an updated workshop and are never
  partially applied. Rollback: revert this commit; the additive daemon route is
  harmless if Home is rolled back first.
- **HCP-2A — `39d61398`.** Added project-scoped diagnostic capture for
  transparent editor sessions, aggregate diagnostics across active editor and
  agent language sessions, and a grouped/filterable Problems panel that opens
  unopened files. Migration: none. Compatibility: older coding engines are
  queried once per open language and partial gaps are shown. Rollback: revert
  this commit; the aggregate response fields are additive and older Home builds
  ignore them.
- **HCP-2B — `2ef49079`.** Added canonical per-document language-root
  discovery, closest-marker resolution inside the governed Forge working copy,
  nested-root Home pooling, and daemon-side rejection of caller-supplied
  workshop paths. Migration: none. Compatibility: Home falls back to the whole
  project root when an older coding engine lacks discovery. Rollback: revert
  this commit; the route and document query are additive, and the daemon remains
  the outer path authority.
- **HCP-2C — `c04d7fe1`.** Added workshop language-session lifecycle records,
  stderr/LSP/progress capture, `GET /v1/code/language-sessions`, bounded Home
  reconnect with Restart/Logs/Repair, and coding-engine answers for
  configuration, workspace folders, and work-done progress on the editor
  channel. Migration: none. Compatibility: older engines omit the sessions
  route; Home keeps editing and shows an explicit log-fetch failure. Rollback:
  revert this commit; the route and initialize capability rewrite are additive.
- **HCP-3A — `45f679a5`.** Added Svelte grammar/LSP (`svelteserver`), JSX/TSX
  CodeMirror modes, shared `langservers` packaging for Pyright/tsserver/Svelte,
  and Repair that installs the language's package id. Migration: none.
  Compatibility: older langservers tarballs without `svelteserver` still install
  the other binaries; Home shows a missing-binary matrix/repair state. Rollback:
  revert this commit; the registry and package catalog entries are additive.
- **HCP-3B — `683d9558`.** Added `GET /v1/code/language-matrix` with binary
  probes, package ids, and extensions; Home consults usability before claiming
  support and Repair installs the row's exact package id (or explains a PATH
  binary). Migration: none. Compatibility: older engines omit the matrix and
  Home keeps attempting LSP with the previous reconnect/repair path. Rollback:
  revert this commit; the route is additive.
- **HCP-3C — `72938652`.** Added CodeMirror grammars for the remaining
  registered LSP languages (Go/C++/Java/PHP official packs; C#/Kotlin/Ruby/
  Lua/Swift legacy modes) so those servers are no longer plaintext in Code.
  Migration: none. Compatibility: older Homes keep plaintext highlighting;
  Repair still explains PATH-only binaries without a package id. Rollback:
  revert this commit; grammar packages are additive.
- **HCP-4A — `e5809d7c`.** Added sequenced `ForgeProjectEvent` journal,
  debounced worktree watcher, path-aware publishes from source routes, and
  `GET /v1/forge/items/{id}/project-events?since=` SSE replay. Migration: none.
  Compatibility: older Homes ignore the route; `/v1/forge/stream` stays the
  list-freshness channel. Rollback: revert this commit; the route and watcher
  are additive.
- **HCP-4B — `e5a86d54`.** Home consumes project-events with `?since=` reconnect,
  reconciles every open buffer, recovers rename/delete, keeps dirty drafts with
  compare/rebase, and notifies LSP via `workspace/didChangeWatchedFiles`.
  Migration: none. Compatibility: older daemons omit the stream; focus-based
  reconcile remains. Rollback: revert this commit; additive client surface.
- **HCP-5A — `1793b47f`.** Split Editor retains the current tab in both panes
  (`moveActiveToNewSplit` / drag-edge still move), directional focus follows
  unit-square geometry, and Code history records/restores shell `groupId`.
  Migration: none. Compatibility: existing shell layouts restore unchanged.
  Rollback: revert this commit; additive split/history behavior.
- **HCP-5B — `7a43454a`.** Persisted contextual Code `layout` in workspace-state
  (Problems/Terminal/Tests), group-targeted `openCodeFile` / mirror, and
  `visibleCodeTabsInGroup` helpers composing shell strips. Migration: none.
  Compatibility: older daemons ignore `layout`; Home keeps defaults. Rollback:
  revert this commit; `layout` is additive.
- **HCP-5C — `dbd4a3d4`.** Shared Code command ids/aliases in Spotlight,
  `medousa-code-command` dispatch into the editor, and a thin remappable chord
  allowlist (`commandBindings`) without a Settings keybinding editor.
  Migration: none. Compatibility: catalog chords remain; overrides are local.
  Rollback: revert this commit; additive command surface.
- **HCP-6A — `f6c8b3ec`.** Full repository Search (`git grep` options + cursor
  pagination), Code Search panel with cancel/load-more, `layout.search`, and
  Mod+Shift+F. Migration: none. Compatibility: older Homes ignore new query
  params and `next_cursor`. Rollback: revert this commit; additive search
  surface.
- **HCP-6B — `2314d228`.** Digest-fenced Search replace preview/apply, New
  folder (`kind=directory`), nested parent creation, and guarded folder
  rename/delete via workspace-edit. Migration: none. Compatibility: older Homes
  ignore replace/directory fields. Rollback: revert this commit; additive
  routes and UI.
- **HCP-6C — `f72799cf`.** Large/binary/lossy source reads return
  `encoding`/`preview`/`truncated` with hex or truncated text; Code opens those
  tabs read-only; Quick Open uses fuzzy path matching. Migration: none.
  Compatibility: older Homes ignore preview fields. Rollback: revert this
  commit; additive read metadata and UI.
- **HCP-7A — `7fe7bef5`.** Streaming project task runs with SSE
  `…/task-runs/{id}/events?since=`, bounded live stdout/stderr on poll, and a
  Code Output panel. Migration: none. Compatibility: older Homes keep
  poll-until-exit. Rollback: revert this commit; additive stream route/fields.
- **HCP-7B — `a91774b5`.** Named Output channels with background readiness,
  incremental problem locations, and thin `.vscode/tasks.json` import
  (`npm`/`shell`/`process` + inline matchers). Migration: none. Compatibility:
  older Homes ignore new task/run fields. Rollback: revert this commit;
  additive task metadata and UI.

### Product-fit boundary (post-HCP-8)

Home ships through HCP-8 workbench depth (Search/replace, Terminal/tasks,
Changes/diff) plus a thin remote Browser handoff (HCP-7D). Full Testing IDE
(HCP-9), debugger DAP surfaces (HCP-10), and heavy VS Code settings/profile
import (HCP-11B) are **out of current Home scope** — Code stays one peer among
Chat/Notes/Browser, not a VS Code fork. HCP-11A/11C may reopen only for pack
registry or remote-parity needs that stay workshop-owned.

### Slice rules

- One slice commit contains its production code, migrations/contracts, tests,
  and documentation. Do not commit a status checkmark before its acceptance
  evidence exists.
- A slice starts from a green previous commit and ends with relevant focused
  tests, `npm run check` for Home changes, and Rust formatting/tests for changed
  crates.
- Commit subjects use a narrow scope, for example
  `feat(home-code): open cross-file language targets`.
- A failing experiment is repaired in a follow-up commit or reverted as a
  whole slice; unrelated cleanup is never folded into the revert.
- At HCP-4, HCP-7, HCP-8, HCP-10, and final completion, record a checkpoint
  summary with commit ids, migrations, compatibility behavior, and rollback
  notes.
- Daemon contracts remain backward-compatible for one Home release wherever a
  rolling remote upgrade can mix versions. Home must degrade explicitly when
  an older daemon lacks a new endpoint.

## Slice acceptance requirements

### HCP-1 through HCP-4: trustworthy editing

- F12, declaration, type definition, and implementation open an unopened target
  in the requested group and place a reversible location-history entry.
- Multi-file edits either apply atomically through valid digests or leave every
  file unchanged with a conflict preview.
- Problems includes every diagnostic known to every active project-language
  session, groups and filters it, and navigates to unopened files.
- Opening `apps/medousa-home/src/**/*.svelte` supplies Svelte syntax,
  completion, hover, navigation, diagnostics, formatting, and rename after one
  exact package repair action.
- Nested Cargo/npm/Go/etc. packages use the closest valid language root without
  escaping the governed worktree.
- Killing a language server produces visible degraded state, bounded reconnect,
  restart/log actions, and successful recovery without closing the project.
- A source change from an agent or Terminal updates every clean open buffer;
  dirty buffers retain the draft and open a compare/reconcile path.

### HCP-5 through HCP-8: daily-driver workbench

- Split Editor retains the current source in both groups; moving a tab remains
  a separate command. Directional focus follows rendered geometry.
- `Cmd/Ctrl+Shift+P`, Quick Open, Back/Forward, Explorer, Search, Changes,
  Problems, Output, Terminal, Tests, and Debug have familiar aliases and
  remappable command identities.
- Regex workspace search spans tracked and untracked files, honors scope and
  excludes, streams/paginates results, and previews digest-fenced replacement.
- A long-running development task displays live output in a named terminal,
  reports background readiness, yields clickable locations and URLs, survives
  pane changes, and stops cleanly.
- A service bound on a remote workshop can open in the Home Browser through an
  authenticated private proxy without exposing a public listener by default.
- Changes shows authoritative branch/upstream/conflict state. Diff context
  expansion reveals real lines. File/hunk revert and conflict resolution are
  recoverable and visible in Forge provenance.

### HCP-9 through HCP-11: IDE and ecosystem depth

**Parked for Home.** These slices describe Testing IDE, debugger, and deep
settings/profile parity that would pull Code outside the current product model.
Leave them ⛔ unless a later decision reopens a thin, workshop-owned subset
(for example first-party pack registry without VSIX or multi-root audit).

- Test adapters discover hierarchy without regex-only source scans; run,
  cancel, rerun, watch, coverage, and debug state persist and navigate.
- F5 launches or selects a workshop debugger. Breakpoints, stepping, stack,
  variables, watch, console, source maps, restart, and stop work across remote
  reconnects where the adapter permits it.
- Installing a first-party pack contributes its grammar, language server,
  formatter, commands, tasks, tests, debugger, settings, and exact health state
  without a Home rebuild.
- User and project keybindings/settings support context rules, conflicts,
  reset, profiles, and workshop overrides.
- Multi-root or environment-backed projects make authority and active root
  visible and never mix local Home paths with workshop paths.

## End-to-end acceptance journeys

1. **Remote Svelte monorepo:** Open a remote repository, Quick Open a Svelte
   file, navigate by F12 into an unopened nested package, go Back, rename across
   files with preview, and inspect all project diagnostics.
2. **Editor groups:** Split the current file, keep it visible on both sides,
   open a comparison, move through groups geometrically, remap one shortcut,
   close Home, and restore the layout and dirty drafts.
3. **Search/refactor:** Regex-search tracked and untracked files with globs,
   preview a repository replacement, reject selected replacements, apply the
   rest atomically, and undo/review the governed change.
4. **Run/web loop:** Start a remote development server, see live output and
   readiness, open its private proxied URL beside Code, navigate a streamed
   compiler error, then stop and rerun the task.
5. **Test/debug loop:** Run one test from the gutter, inspect structured
   failure output, rerun with coverage, debug it at a conditional breakpoint,
   inspect stack/variables/watch, and evaluate in the Debug Console.
6. **Concurrent change:** While a dirty human draft is open, let an agent change
   the same file and Terminal rename another open file. Resolve the conflict,
   retain the intended draft, and verify all LSP/file tabs recover.
7. **Changes/finish:** Inspect branch and upstream state, review and revert a
   hunk, resolve a merge conflict, see provenance and verification in Review,
   finish through Forge, and open the resulting provider review.
8. **Failure recovery:** Disconnect Home, kill LSP/task/debug processes, restart
   the workshop, reconnect, and recover layout, drafts, terminals, task history,
   test state, breakpoints, and explicit service health.

## Verification gates

Run focused tests for each changed package plus the relevant integration or
end-to-end journey. Before a major checkpoint or final completion, run from the
repository root:

```bash
cargo clippy --workspace --all-targets --exclude medousa-sdk-iroh -- -D warnings
cargo test -p medousa --lib
cd apps/medousa-home && npm ci && npm run check
```

Additional required suites will be added beside their contracts:

- Code workspace/navigation browser tests with unopened and duplicate views;
- daemon event-stream cursor/reconnect and compatibility tests;
- LSP crash/root/package matrices;
- search/replace property and digest-conflict tests;
- PTY/task stream ordering, replay, cancellation, and background readiness;
- remote service proxy authentication/isolation tests;
- Git/Forge conflict and rollback matrices;
- test-adapter and DAP conformance fixtures;
- contribution manifest/schema and package lifecycle tests;
- local and remote desktop acceptance smoke tests.

## Explicit non-goals

- Pixel-for-pixel VS Code chrome.
- Making an Activity Bar or every IDE panel permanent across Home.
- Running remote-project processes or reading remote-project files on the Home
  device.
- Replacing Forge custody with an unmanaged checkout-editing path.
- Binary compatibility with arbitrary VSIX extensions or embedding the VS Code
  extension host.
- Requiring AI controls on every line.
- Treating mobile as a full VS Code parity target; mobile must remain safe and
  coherent, while the complete workbench target is desktop.
