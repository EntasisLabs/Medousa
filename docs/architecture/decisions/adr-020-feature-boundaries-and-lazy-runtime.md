# ADR-020: Feature boundaries, lazy runtime, and optional workloads

> **Status:** Accepted
>
> **Date:** 2026-08-13
>
> **Decision owners:** Home, daemon composition, and packaging maintainers
>
> **Related:** [ADR-012](adr-012-medousa-anywhere-surfaces.md), [H09 execution plan](../../../architecture/hardening/09-home-runtime-boundaries.md), H11 package/dependency plan (planned)

## Context

Medousa Home's root route statically imports most product surfaces and their
singleton stores before choosing desktop/mobile layout or a visible destination.
The measured production root closure is 7,102,090 minified JavaScript bytes and
1,448,096 minified CSS bytes. Hidden vault, CodeMirror, browser, export, Liquid,
wizard, mobile, and desktop feature graphs still parse, evaluate, initialize,
and retain state.

The import graph reflects collapsed ownership. Markdown imports Liquid
hydration, which imports a side-effect component registry, whose prose component
imports Markdown. Vault/workshop stores and helpers enter the same large strongly
connected component. Other stores import one another to coordinate actions.
Initialization order is accidental, lazy splitting is obstructed, and pure-
looking helpers can pull in reactive process state and UI components.

Large modules combine unrelated authorities: transport, protocol reduction,
persistence, reactive presentation, orchestration, and feature UI. Mechanical
file splitting would preserve the same cycles and shared state under more names.

Optional daemon workloads have the same composition problem. Home-first install
requires an immediately useful app while optional binaries/features are
installed from Settings → Packages. A UI chunk boundary and an optional runtime
package boundary must use the same capability contract without making the shell
import or link every implementation.

## Decision

### 1. The startup shell is a product boundary

The initial static graph contains only boot/connection/authentication, platform
selection, navigation/layout primitives, immediate chat shell, error/loading/
toast surfaces, and dependency-light contracts. It does not import dormant
destination implementations, editors, exporters, complex renderers, wizards,
or their stores.

Desktop versus mobile composition is selected before importing its shell graph.
Device choice is not a CSS-only visibility decision. Shared shell primitives
remain small and platform-neutral; platform destinations load afterward on
explicit need.

### 2. Every feature has an explicit lifecycle

A feature exports dependency-light metadata separately from implementation:

```text
FeatureDescriptor {
  id, routes/destinations, labels/icons,
  required client capability,
  required daemon capability/package,
  preload policy
}

FeatureLoader = () -> import(feature implementation)

FeatureModule {
  start(FeatureContext) -> FeatureInstance
}

FeatureInstance {
  commands/views/state handles,
  dispose()
}
```

Descriptor registries must not import feature stores/components. `start` creates
scoped state, listeners, polling, workers, caches, and native subscriptions.
`dispose` deterministically removes them. A feature cannot initialize through a
module-level singleton side effect.

The shell loads on navigation, explicit user intent, restored active destination,
or bounded idle prefetch after first interaction. A dynamic import scheduled
unconditionally at startup is counted as startup work and fails the boundary.

### 3. Dependencies point inward, never sideways through globals

Within Home, runtime dependency direction is:

```text
contracts/types + pure domain transforms
  -> service ports/adapters
  -> feature state/reducers/controllers
  -> feature views/components/styles
  -> shell composition
```

Lower layers do not import Svelte components, feature stores, registries with
side effects, or shell state. Feature A does not import Feature B's store.
Cross-feature workflows use shell-owned orchestration over explicit ports,
typed commands/events, or passed callbacks. Shared domain code remains UI- and
runtime-independent.

Runtime cycles among first-party modules are forbidden. Type-only references do
not create runtime edges. Dynamic imports are allowed only from a higher
composition layer to a declared feature boundary, not to hide a reversed
dependency.

### 4. Registries separate vocabulary from implementations

Markdown/Liquid/tool/view registries expose pure schemas/descriptors without
loading renderers. Component and heavy-library factories are resolved lazily by
feature. Markdown receives embed resolvers/renderers as explicit context; a
prose renderer cannot import the registry that imports prose.

The same rule applies to commands and native handlers: feature registration
contributes a typed inventory from bounded modules. Security/contract tooling
can enumerate it without one hand-maintained mega-list or importing every
implementation into the startup shell.

### 5. State owners align with protocol and lifecycle

Transport consumes generated events and feeds pure reducers. Reducers own
normalized domain state and are testable without Svelte/runtime globals.
Feature controllers own async effects, cancellation, persistence, and service
coordination. Thin reactive adapters expose only view state/actions. Components
do not own durable/transport authority.

Splits follow invariants and independent lifecycle, not an arbitrary line count
or visual fragment. A module has one state owner and a small public interface;
internal siblings cannot reach its mutable implementation directly.

### 6. JavaScript, CSS, and themes share the feature boundary

Feature CSS is imported by the feature entry and is absent before the feature
loads. Global CSS contains reset, typography/tokens, shell layout primitives,
accessibility, and genuinely shared utilities only. Feature selectors and
component internals do not accumulate in `app.postcss`.

Theme metadata is dependency-light. The selected theme's variables are loaded
at startup; the complete theme catalog is not expanded into the critical CSS.
Theme switching loads/replaces a bounded token stylesheet without importing
feature implementations.

### 7. UI availability and workload installation are capability-driven

A feature may be built into Home while its daemon workload is absent. The shell
uses an authoritative capability/package descriptor to show available,
installable, degraded, or unsupported state. It does not detect availability by
importing an implementation, probing an arbitrary binary path, or exposing a
route/tool that later fails mysteriously.

Optional workload implementations live behind daemon composition/package
boundaries and are installed to `{dataDir}/bin` through Settings → Packages in
the Home-first model. H11 defines Cargo/dependency/binary budgets, installers,
and route/tool composition. ADR-020 fixes the shared rule: metadata/ports may be
core; optional implementation/dependencies and UI chunks are not pulled into
the default path merely to advertise availability.

## Consequences

### Positive

- First interaction no longer parses and initializes most of the product.
- Feature code, CSS, state, listeners, and optional workloads have matching
  ownership and lifecycle.
- Runtime cycles and singleton initialization order become enforceable errors.
- Tests can construct reducers/controllers without booting the application.
- Large modules split around actual authority rather than cosmetic files.
- Home-first packaging can advertise optional capabilities without bundling all
  implementations into startup/default daemon composition.

### Costs

- Shell navigation and restored state become asynchronous with loading/error UX.
- Feature state that must survive unload needs an explicit bounded cache or
  durable service rather than accidental singleton lifetime.
- Cross-feature flows require ports/orchestration instead of convenient imports.
- CSS/theme extraction and registry redesign touch broad UI surfaces.
- Build/dependency graph analyzers become required CI inputs.

### Relationship to prior decisions

ADR-012's native-host versus external-agent surface distinction remains. Feature
descriptors express the required surface/capability, and loaders select only an
implementation valid for that host. ADR-020 does not turn filesystem authority
into upload or local-path fallback.

H03 owns incremental chat stream/render behavior. H09 may split its frontend
owners and loading boundary but cannot define a competing stream protocol. H11
applies this decision to Rust/Cargo/sidecars/packages and owns DEP-001 closure.

## Verification

P08/P09 in the [performance budgets](../../../architecture/hardening/verification/performance-budgets.md)
govern startup and optional workload composition. CI retains Vite manifest
closure, first-use traces, runtime import graph/SCC inventory, CSS ownership,
feature lifecycle leak tests, and daemon/package feature graphs.

## Code anchors

- `apps/medousa-home/src/lib/components/layout/AppShell.svelte`
- `apps/medousa-home/src/lib/stores/*.svelte.ts`
- Markdown/Liquid registries and hydrators
- `apps/medousa-home/src/app.postcss`
- `apps/medousa-home/tailwind.config.ts`
- Home/Tauri/daemon feature and command composition
