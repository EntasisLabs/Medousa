# H09 — Home runtime and feature boundaries

> **Status:** Implementing complete for H09 code trains. ARCH-001 and ARCH-002
> are **Mitigated** on unit/CI exit tests. FRONT-001 stays **Proposed**: root
> CSS is 640,347 bytes, still above the 600 KiB table target. JS 2,109,096,
> largest chunk 852,853, and gzip CSS 83,636 meet their table targets; dormant
> overlays are absent from the static closure. Validated/Shipped still need P08
> packaged multi-OS evidence.
>
> **Accountable owner:** Medousa Home maintainers
>
> **Reviewers:** chat/streaming, vault/code/browser/workspace features, design system, desktop/mobile, build/release
>
> **Audit findings:** FRONT-001 (High), ARCH-001 (High), ARCH-002 (High)
>
> **Release gate:** Gate D — enforced architecture
>
> **Required decision:** [ADR-020](../../docs/architecture/decisions/adr-020-feature-boundaries-and-lazy-runtime.md)
>
> **Dependencies:** H03 frontend stream pipeline; H07 vault projection; H08 trusted-shell CSP; H10 generated contracts; H11 optional package composition
>
> **Verification:** [Performance budget P08](verification/performance-budgets.md)

## Outcome

Medousa opens into a small Home-first shell and immediate chat path. Desktop or
mobile composition is selected before loading its implementation. Vault/Code,
browser, work hub, exporters, complex Liquid renderers, settings subsections,
wizards, and other destinations load on real user/restored-state intent, own
their state/listeners/CSS, and dispose cleanly.

The first-party Home runtime graph has no cycles. Pure reducers and contracts sit
below effects/reactivity/components. Cross-feature behavior goes through typed
ports and shell orchestration instead of importing singleton stores. Mega-files
are decomposed by state/authority owner, with generated inventories and CI
boundaries preventing reassembly under new filenames.

H09 owns Home loading, runtime dependency direction, frontend state/component/
CSS ownership, and the UI side of feature composition. H03 owns stream batching
and incremental message rendering. H07 owns the shared vault generation lookup.
H10 owns generated API types/transports. H11 owns Rust dependency/package cost.

## Baseline

The 2026-08-12 production build measured:

| Metric | Baseline |
| --- | ---: |
| Root static JavaScript | 7,102,090 minified bytes / 56 files |
| Root static JavaScript gzip | 2,120,493 bytes |
| Root static CSS | 1,448,096 minified bytes / 11 files |
| Root static CSS gzip | 189,858 bytes |
| Total generated JavaScript | 11,761,808 minified bytes / 164 chunks |
| Largest known runtime SCC | 74 first-party modules |
| Runtime SCCs found | 7 |
| Global layout CSS asset | 953,407 minified bytes |
| `app.postcss` source | 15,070 lines / 399,347 bytes |

Large initial chunks were attributable to page/application composition, vault
store/workshop, shell tabs, chat, and CodeMirror. These measurements are binding
ceilings until the lower ratchets below are enforced; they are not targets.

## Current ownership failures

### Root composition

`AppShell.svelte` statically imports both `WorkshopShell` and `MobileShell`,
vault workshop/attachments/import wizard, browser workshops, work popover,
wizard, multiple context menus, spotlight, and numerous singleton stores. It
then decides visibility and platform at runtime. Module evaluation has already
paid the cost and installed side effects before that decision.

Root `onMount` starts wizard bootstrap, viewport/native listeners, peer polling,
and agent-browser coordination. Some are shell responsibilities; others belong
to dormant features or connection-scoped services. There is no common ownership
record proving what started, why it remains live, or how it disposes on workshop,
platform, session, or feature change.

### Runtime cycles

Train 3 deleted the first-party SCCs. The Markdown barrel is parse/sanitize
only; Liquid descriptor `index.ts` has no component/CSS side effects; prose
renders through the parse pipeline; vault/workshop/export helpers no longer
close a loop through hydrate. `check:runtime-graph` requires an empty ledger.
ARCH-001 is **Mitigated** on that unit/CI exit; Validated still needs P08.

### State and component mega-owners

The chat store owns transport sequencing, stream interpretation, transcript/UI
shaping, tools/artifacts, errors, persistence, and reactive state. The vault
store combines note buffers, autosave, tree/browse/search, many editor modes,
workspace coordination, native files, templates, imports, export, and view
orchestration. Large work/code components combine state machines, editor/tool
integration, services, and several independently loadable UI modes.

Splitting only source text would create mutually importing siblings. Authority
must move first into reducers, controllers, adapters, and feature-local state.

Train 4 moved chat transcript/turn/draft owners, injected the H07 vault lookup
and note buffers, routed code/work panels through document and undertaking
controllers, deleted the remaining high-leverage feature-store imports via
shell ports, and added start/dispose leak tests for workshop/platform/navigate
cycles. ARCH-002 is **Mitigated** on that unit/CI exit; Validated still needs
P08.

### CSS and themes

`app.postcss` is a global feature dump, and Tailwind/Skeleton expands all shipped
themes into the build. Feature chunks cannot omit their styling; WebView parses
and retains selectors for hidden destinations. Global selectors also make
component ownership and deletion unsafe.

## Invariants

1. Initial static graph contains only boot/connection/auth, platform selector,
   navigation/layout core, immediate chat shell, shared tokens, error/loading,
   and dependency-light contracts.
2. Desktop and mobile destination graphs are mutually lazy after the platform
   selector; a desktop launch does not evaluate mobile features and vice versa.
3. Dormant feature code/CSS/state/listeners/workers/pollers are not loaded or
   started before declared navigation/restoration/prefetch intent.
4. Every feature has one idempotent `start`, deterministic `dispose`, scoped
   cancellation, and observable retained-resource inventory.
5. Descriptor/type registries do not import implementation components/stores.
6. Runtime dependencies follow contracts/domain → services → state/controllers
   → views → composition. First-party runtime SCC count is zero.
7. Feature stores do not import sibling feature stores. Shell orchestration uses
   typed ports/events/commands and owns cross-feature sequencing.
8. Transport is an adapter; pure reducers cannot import Svelte, browser/native
   APIs, persistence, components, or singleton services.
9. H03 is the only chat stream reducer/order owner; UI presentation derives
   from its typed state without reinterpreting raw event strings elsewhere.
10. Feature CSS loads with its feature and cannot select another feature's
    internals. Global CSS is limited to reviewed shell/design-system concerns.
11. Only selected theme tokens enter the critical style path; catalog metadata
    does not expand all theme selector trees at startup.
12. Lazy first-use has explicit loading/error/retry/cancel UX and preserves
    navigation/accessibility state.
13. Unloading/reconnecting cannot leave subscriptions, intervals, native
    listeners, observers, workers, large caches, or stale store authority.
14. Build manifest, module graph, CSS ownership, lifecycle, and first-use costs
    are enforceable CI artifacts, not manual review claims.
15. File size is a review alarm; boundary success is measured by ownership,
    public surface, dependency direction, and lifecycle—not line-count theater.

## Non-goals

- turning every component into a separate chunk;
- lazily loading the immediate chat shell required by Home-first startup;
- scheduling all feature imports one tick after launch and calling it lazy;
- breaking cycles with dynamic imports in lower layers;
- introducing a global event bus with untyped string events;
- preserving singleton feature state forever for convenience;
- moving all code from one mega-file into a directory of mutual imports;
- optimizing H03 stream rendering or H07 vault algorithms a second time; or
- forcing optional daemon workloads into the Home bundle to advertise them.

## Shell core and feature catalog

### Initial shell

The initial shell owns:

- build/config bootstrap and authenticated workshop connection selection;
- desktop/mobile platform selection using a dependency-light probe;
- route/destination state and accessible navigation skeleton;
- minimal chat history/composer/stream presentation defined with H03;
- session/workshop identity summary needed for that chat;
- global error boundary, loading skeleton, toast service, and offline/reconnect;
- selected theme tokens and core typography/layout CSS; and
- `FeatureCatalog` descriptors/loaders, not implementations.

Spotlight, onboarding/import wizards, peer/calendar polling, browser coordination,
vault workshop, work hub, CodeMirror, exporters, complex Liquid organisms, and
settings panels are not shell-core merely because they can overlay the root.
Their triggers use dependency-light commands that load the owning feature.

### Feature boundaries

Initial feature set and likely entry points:

| Feature | Load trigger | Heavy/private implementation |
| --- | --- | --- |
| Vault browse | Library destination/restored note | projection/tree/search/list views |
| Vault edit | note open/edit intent | TipTap/CodeMirror, data-first editors, attachments |
| Code/work | Code/work destination or card | CodeMirror languages/LSP, undertakings/source editor |
| Browser | Web destination/agent handoff | compositor, human-browser stores/workshops |
| Rich renderers | first matching stable block | Mermaid, Liquid organisms, draw/view hydrators |
| Export/import | explicit command | docx/PDF/html2canvas/spreadsheet parsers/wizards |
| Settings section | section navigation | packages/models/connections/themes diagnostics |
| Peers/calendar/profile | destination/event | pollers, editors, domain stores |
| Spotlight | shortcut/button | command index and feature command providers |

Some boundaries may split further after P08 first-use traces. Avoid one “vault”
chunk containing every editor/exporter, or one “settings” chunk containing all
optional package UI.

### Descriptor and loader

```ts
interface FeatureDescriptor {
  id: FeatureId;
  destinations: readonly DestinationId[];
  clientPlatforms: readonly ClientPlatform[];
  requiredCapabilities: readonly CapabilityId[];
  preload: "never" | "intent";
}

interface FeatureModule {
  start(context: FeatureContext): Promise<FeatureInstance>;
}

interface FeatureInstance {
  dispose(reason: DisposeReason): Promise<void> | void;
}
```

`FeatureContext` supplies scoped generated daemon client, H05 connection/session
context, navigation/dialog/toast ports, feature storage namespace, cancellation,
metrics, and explicitly declared shared domain ports. It does not expose every
singleton store or raw Tauri invoke.

The loader deduplicates concurrent loads, handles cancellation/failure, records
chunk/CSS/start timing, and never exposes a half-started instance. Start failure
disposes partial resources. Disposal runs on workshop switch/logout, platform
composition replacement, app teardown, and explicit eviction for cacheable
features; hiding a pane alone need not dispose if its bounded retention policy
says otherwise.

## State ownership refactor

### Chat

Split into:

- generated transport adapter and connection lifecycle;
- H03 pure typed stream/transcript reducer with O(1) identity indexes;
- turn command/controller for send/cancel/retry/reconnect;
- bounded draft/session persistence service;
- normalized tool/artifact/approval projections; and
- thin Svelte view model/selectors.

Raw protocol event interpretation exists once. Components subscribe to narrow
selectors so a content batch does not make unrelated transcript/tool surfaces
react. Rich block rendering is a lazy presentation feature over stable H03
blocks, not part of transport/reducer authority.

### Vault, work, and code

H07's generation projection and note buffers become injected domain services.
Vault browse, editor, attachments, data-first views, imports, exports, and
workshop coordination receive narrow ports rather than importing one store.

Work/code separates undertaking projection, command controller, editor document
service, LSP/runtime adapter, terminal integration, source/tree/search views,
and independent panel modes. Large visual components consume controllers and
view models; they do not perform daemon orchestration/persistence directly.

### Cross-feature orchestration

Create small shell/application use cases such as `OpenVaultNote`, `OpenWorkCard`,
`OpenBrowserHandoff`, and `OpenPeerThread`. They depend on loader/navigation and
domain ports and can asynchronously load the destination. Feature stores do not
import each other to implement these flows.

Typed events describe facts (`NoteCommitted`, `TurnCompleted`) with one owner and
bounded subscribers. Commands request effects and return results. Do not replace
cycles with a global string event soup or document `CustomEvent` as domain API.

## Markdown and Liquid boundary

Split the registry into:

- dependency-free block/descriptor schemas and parsing contracts;
- pure Markdown parse/sanitize pipeline with explicit per-call render context;
- lazy renderer factories keyed by stable descriptor id;
- feature-local hydration/mount adapters with exact disposal; and
- presentation features for Mermaid/Liquid/draw/view/export implementations.

`MarkdownContent` may depend on parser contracts and a passed renderer resolver.
It cannot import the full Liquid registry. The prose renderer uses the pure
Markdown port and cannot import the host/registry that resolves prose. Remove
module-global render options/counters; concurrent rendering gets independent
context.

H03 terminal/stable blocks trigger full parsing. Lazy renderer loading does not
reparse or destroy already stable blocks. Unsupported/unloaded blocks show a
bounded placeholder and load on visibility/intent according to policy.

## CSS and themes

Classify every `app.postcss` section:

1. core reset/tokens/typography/accessibility/shell layout;
2. design-system primitive owned by a small shared package;
3. feature/component style moved beside its feature entry; or
4. dead/duplicated selector deleted with visual evidence.

Feature CSS uses component-scoped styles, explicit feature root layers, or CSS
modules according to existing Svelte conventions. It cannot target another
feature's private DOM. Cascade layer order is declared centrally without
importing feature rules.

Theme catalog exports names/previews/token asset locations only. Build one small
base token set and separate theme-variable assets; startup loads the selected
theme. Switching atomically replaces theme variables and measures style/layout
cost. The complete 50-theme catalog must not generate complete critical selector
trees. Preserve accessibility/contrast and offline availability.

H08 owns the production CSP implications of lazy scripts/styles. Loaders use
bundled hashed assets accepted by that CSP; they do not fetch executable code
from package/CDN URLs.

## Loading, caching, and UX

Navigation transitions to a feature-owned skeleton immediately, loads module
and required daemon capability in parallel where safe, starts it, then restores
selection/focus/scroll. Error states identify unavailable package, incompatible
client/daemon, load failure, or start failure and offer bounded retry.

Prefetch is allowed only after first interaction and when device/memory/network
policy permits. Intent prefetch may begin on hover/focus/touchstart shortly
before navigation. Record trigger and useful/wasted result. Cancel/evict unused
prefetch under memory pressure. A feature chunk fetched at startup without
declared restored intent counts against startup budgets.

Feature instances declare cache policy and retained bytes. Editors may preserve
bounded dirty buffers through a domain service, but component trees, language
packages, DOM observers, and transport listeners dispose. Re-enter reconstructs
from domain state rather than relying on hidden mounted UI.

## Enforced dependency graph

Add a Vite/Svelte-aware analyzer that resolves aliases, `.svelte` scripts,
barrels, static imports, side-effect imports, and dynamic feature edges. It
classifies type-only imports separately and emits:

- runtime SCCs with shortest cycle paths;
- layer/feature boundary violations;
- shell static and eager-dynamic closure;
- feature-to-feature implementation edges;
- modules imported for side effects;
- chunk/CSS ownership and duplicate heavy libraries; and
- exception ledger entries with owner, reason, expiry, and test.

CI first enforces zero new SCCs/boundary violations against a checked-in ledger,
then burns the seven known SCCs down. H09 validation requires zero first-party
runtime SCCs; no permanent baseline exceptions. Generated/vendor graphs are
reported separately. Barrels may export within one layer/feature but cannot
collapse implementation boundaries.

## Size and ownership alarms

Add review alarms, not automatic refactor quotas:

- first-party source over 1,000 logical lines requires an ownership/interface
  note and named reviewer;
- over 2,000 logical lines fails unless generated/declarative data has a checked
  exception and no mutable authority;
- state/controller/component modules with more than one lifecycle/transport/
  persistence owner fail the boundary test regardless of size;
- root command/feature/renderer registries must be generated/composed from
  bounded registrations; and
- global CSS additions require classification and byte/rule delta.

Targeted files include `chat.svelte.ts`, `vault.svelte.ts`,
`CodeSourceEditor.svelte`, `UndertakingsPanel.svelte`, `app.postcss`, Markdown/
Liquid registry/hydration, and `AppShell.svelte`. Backend mega-modules are split
by their owning hardening workstreams; H09 supplies composition rules, not
duplicate refactors.

## Initial performance ratchets

These first design budgets are deliberately below the measured ceiling and may
only tighten after slice evidence. A proposal to loosen one requires retained
P08 traces and owner approval.

| Metric | H09 validation ratchet |
| --- | ---: |
| Root static JavaScript | ≤ 3.0 MiB minified and ≤ 1.0 MiB gzip |
| Root static CSS | ≤ 600 KiB minified and ≤ 100 KiB gzip |
| Largest initial JavaScript chunk | ≤ 900 KiB minified |
| Initial first-party runtime SCCs | 0 |
| Dormant feature imports in first 5 s idle | 0 without restored/user/prefetch intent |
| Feature lifecycle residue after dispose | 0 listeners/timers/observers/workers; bounded documented cache only |
| Platform cross-load | 0 mobile destination modules on desktop and vice versa |

P08 also sets machine-specific paint/interaction/heap/style-recalc budgets from
the first boundary slice. Byte compliance alone is insufficient: immediately
loading 8 MiB dynamically or regressing first interaction fails.

Per-feature first-use budgets record JS/CSS bytes, parse/evaluate/start time,
heap delta, network/disk reads, and time to usable. Editor/exporter load cost may
be large but must be paid once on explicit use, cancellable where possible, and
retained/evicted under policy.

## Observability

Record without user content:

- shell static/eager closure bytes and chunk/CSS identities by build revision;
- boot/platform-select/connection/chat paint/interaction milestones;
- feature load trigger, fetch/parse/evaluate/start/usable/failure/dispose timing;
- active feature instances, listeners/timers/observers/workers and retained heap;
- useful/wasted/cancelled prefetch;
- reducer action/selectors changed/render counts for hot H03 flows;
- runtime SCC/boundary/side-effect import inventory;
- CSS rules/bytes/style-recalc by core/feature/theme; and
- optional capability unavailable/installable state without probing secrets.

Production metrics use feature IDs and coarse timing/size buckets, not route
parameters, note/chat content, filesystem paths, or raw errors containing data.

## Migration plan

### H09.0 — Reproducible graph and P08 baseline

- Check in manifest closure/CSS analyzer and exact P08 workload/machine record
  (`apps/medousa-home/scripts/verify-bundle-budget.mjs`,
  `apps/medousa-home/security/bundle-budget.json`; `npm run build` then
  `npm run check:bundle-budget`).
- Add Svelte/TypeScript runtime graph analyzer and seven-SCC migration ledger
  (`apps/medousa-home/scripts/verify-runtime-graph.mjs`,
  `apps/medousa-home/security/runtime-scc-ledger.json`; `npm run check:runtime-graph`).
- Instrument feature module evaluation and root-started resources
  (`apps/medousa-home/src/lib/runtime/rootResources.ts`).
- Add screenshot/accessibility/interaction fixtures for desktop/mobile and major
  overlays before changing composition (`SHELL_A11Y_FIXTURES` in rootResources).

### H09.1 — Shell composition and lifecycle kernel

- Define descriptor/loader/context/instance/dispose contracts and typed feature IDs.
- Extract platform probe, shell services, navigation, and cross-feature use cases.
- Choose desktop/mobile loader before importing its graph.
- Move root pollers/listeners/bootstrap to explicit shell or feature owners.
- Add load/error/retry/focus/restoration and lifecycle leak tests.

### H09.2 — First high-value splits

- Lazy-load vault browse/edit, Code/work, browser, import/export, settings
  subsections, spotlight/wizards, and complex renderers by real trigger.
- Keep descriptor registry dependency-light and prove no eager dynamic imports.
- Move each feature's CSS with its entry and measure ratchet after every split.
- Establish H11 capability/package state without importing optional implementation.
- Train 2 measured root static closure: 3,067,547 JS (gzip 970,441) / 1,075,634 CSS
  across 29 JS / 4 CSS files; largest initial JS chunk 1,583,570. Desktop static
  closure contains 0 `MobileShell` / `src/lib/components/mobile/` destinations.
- Train 5 measured root static closure: 2,109,096 JS (gzip 695,687) / 640,347 CSS
  (gzip 83,636) across 54 JS / 3 CSS files; largest initial JS chunk 852,853.
  Tailwind compiles no palettes; boot loads one stored `/themes/<name>.css` sheet.
  Dormant overlays are absent from the static closure. CSS minified remains above
  the 600 KiB table target.

### H09.3 — Break cycles from contracts upward

- Split Markdown descriptor/parser/render context from Liquid implementations.
- Replace side-effect archetype registry with lazy factories.
- Extract pure config/profile/default transforms from stores.
- Move workspace/shell/code and browser cross-store flows into typed orchestration.
- Delete barrels and singleton imports that recreate SCC edges.
- Reach zero first-party runtime SCCs before validation.

### H09.4 — Decompose state/component owners

- Migrate chat to H03 reducer/controller/persistence/reactive adapter.
- Migrate vault to H07 projection/buffer plus browse/editor/export feature owners.
- Split code/work controllers, document/LSP/terminal services, and panel modes.
- Generate/compose feature/native command/renderer inventories.
- Delete legacy singleton authority only after consumers use narrow ports.

### H09.5 — CSS/theme ownership

- Classify all global CSS; delete dead/duplicate rules with visual evidence.
- Move feature/component rules into lazy assets and declare cascade layers.
- Replace build-expanded theme catalog with selected token stylesheet loading.
  Inventory: `apps/medousa-home/security/css-inventory.json`. Browser/peers
  sheets load from their feature entries; vault/chat/settings remain
  pending-extract in `app.postcss`. Contrast fixtures in
  `themes/theme-contract.test.ts`; reduced-motion remains in `app.postcss`
  and `SHELL_A11Y_FIXTURES`.
- Validate CSP, visual regression, contrast, reduced motion, zoom, and platforms.

### H09.6 — Enforce and close

- Graph, manifest, CSS inventory, and lifecycle leak tests are required CI
  (`npm run check` includes `check:runtime-graph`; home job also runs
  `test:h09`, `npm run build`, `check:bundle-budget`). P08 packaged
  paint/interaction/heap remains a Validated gate, not this train.
- Catalog preload is `"never"` or `"intent"` only; no post-launch feature
  prefetch, no `void import(` cheat, SCC ledger is empty.
- Contributor and Home architecture docs record allowed dependency direction.

H09.1 precedes broad splits. H09.2 and SCC work can interleave carefully, but a
dynamic import cannot be used to hide a cycle. Chat decomposition aligns with
H03 implementation; vault decomposition consumes H07 rather than inventing an
interim second projection. H10 generated transport should land before final
transport-adapter deletion.

## Rollout and rollback

Enable feature loaders one boundary at a time behind internal build flags with
identical domain/API behavior. Compare startup/first-use traces and visual/
interaction fixtures. A rollback may restore an eager import for one feature
temporarily if it stays within the current binding ceiling and has a dated
deletion issue; it cannot restore runtime cycles or duplicate mutable authority.

State migrations are one-way by owner: during transition, legacy reactive
adapters derive from the new controller/reducer. Do not dual-write old and new
stores. If a lazy feature fails, keep shell/chat functional and show feature-
specific recovery rather than reloading the whole application.

CSS rollback restores the prior feature stylesheet, not the entire unclassified
global dump. Theme rollback retains one selected theme asset and never re-enables
all-theme critical expansion.

## Verification and exit criteria

FRONT-001 is validated only when:

- the initial graph meets all JavaScript/CSS/largest-chunk/platform/dormant-load
  ratchets and P08 paint/interaction/heap/style budgets;
- desktop/mobile shells are selected before destination graph import;
- each listed heavy feature is absent until its declared trigger and has measured
  usable/error UX;
- selected-theme critical CSS replaces all-theme expansion; and
- lazy work is not merely shifted into immediate idle/startup imports.

ARCH-001 is validated only when:

- the analyzer reports zero first-party runtime SCCs;
- layer and feature implementation edges follow ADR-020 direction;
- Markdown/Liquid and all six smaller audited cycle families are deleted from
  the ledger with shortest-path regression tests;
- descriptor registries and pure helpers import no component/store side effects;
  and zero-new-cycle/boundary checks are required CI.

ARCH-002 is validated only when:

- chat, vault, code/work, Markdown/Liquid, shell composition, and CSS have named
  state/authority owners with small tested public interfaces;
- legacy mega-owner mutable paths and cross-store imports are deleted, not wrapped;
- feature start/dispose leak tests pass across repeated workshop/platform/
  navigation cycles;
- generated/composed registries replace giant hand-maintained ownership lists;
- size exceptions contain no mixed mutable authority and have owner/expiry; and
- behavior/accessibility/visual tests prove decomposition did not erase product
  flows.

All findings reach Shipped only after supported packaged builds, rollout,
rollback cleanup, observability, required CI, and canonical docs ship.

## Canonical documentation at ship time

- Home architecture: shell core, feature catalog/lifecycle, platform composition,
  state layers, cross-feature orchestration, CSS/theme ownership;
- contributor guide: allowed dependency direction, registries, dynamic imports,
  graph/size exceptions, feature tests, and budgets;
- Home app reference: first-use loading/degraded/installable feature behavior;
- design-system/theme docs: token assets, cascade layers, feature styles; and
- build/release runbooks: manifest/SCC/CSS/P08 regressions and safe disablement.

## Superseded code and concepts to delete

- `AppShell.svelte` static imports of dormant desktop/mobile/features/overlays;
- unconditional post-launch feature imports/bootstrap/polling;
- module-level singleton feature initialization without lifecycle ownership;
- feature-store-to-feature-store imports and document string events as domain API;
- Markdown ↔ Liquid registry/prose/hydration runtime cycle;
- audited workspace/shell/code, vault config, browser, identity/profile, and
  defaults/presets runtime cycles;
- monolithic chat/vault/code/work mutable authority paths after adapters migrate;
- side-effect component/archetype/command registration;
- feature selectors and dead/duplicate rules in global `app.postcss`;
- all-theme critical CSS expansion;
- hidden/offscreen mounted feature trees used as state retention; and
- temporary graph exceptions/load flags after release evidence.

## Code anchors

- `apps/medousa-home/src/lib/components/layout/AppShell.svelte`
- desktop/mobile shell and navigation modules
- `apps/medousa-home/src/lib/stores/chat.svelte.ts`
- `apps/medousa-home/src/lib/stores/vault.svelte.ts`
- workspace/shell/code/browser/profile/default stores and controllers
- Markdown/Liquid registry/render/hydration/components
- `CodeSourceEditor.svelte` and `UndertakingsPanel.svelte`
- `apps/medousa-home/src/app.postcss`
- `apps/medousa-home/tailwind.config.ts` and theme catalog
- Vite/Svelte build and test configuration
