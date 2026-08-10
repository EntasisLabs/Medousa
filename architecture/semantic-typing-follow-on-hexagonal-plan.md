# Semantic typing follow-on: hexagonal extraction plan

> **Prerequisite:** [Semantic typing and construction epic](semantic-typing-and-construction-plan.md)
>
> **Purpose:** Move cohesive application services and outbound ports after
> semantic boundaries are stable. This plan deliberately does not prescribe a
> repository-wide directory reshuffle.

## Readiness decision

The semantic pass has exposed stable seams, but it has not justified moving
every tool module. The next phase should extract one application service at a
time, keep the current tool registration and Stasis adapters at the edge, and
prove behavior at each seam.

The first extraction targets are the runtime and recurring paths. They now
have invariant-owning specs, direct typed conversions, and a focused
`RuntimeCompositionExt`; those properties make the infrastructure boundary
visible without moving policy into Stasis.

The audit remains a guardrail rather than a completion score. The current
snapshot is:

| Signal | S0 snapshot | Current snapshot | Reading |
|---|---:|---:|---|
| `trim()` calls | 1,423 | 1,416 | Small reduction; domain-specific normalization remains intentional |
| lenient helper symbols | 375 | 14 | Remaining symbols are centralized compatibility definitions/re-exports |
| direct `PortFailure(err.to_string())` matches | 155 | 163 | Still above S0; continue typed error rollout |
| runtime backend mentions | 160 | 71 | Common tool dispatch now uses the runtime extension |
| `NewJob` literals | 11 | 5 | Remaining literals are the shared builder and tests |
| `RecurringDefinition` literals | 9 | 1 | Construction is centralized in `RecurringScheduleSpec` |

The direct error-mapping increase is explicit follow-on work, not a reason to
hide the metric or add a blanket allowance.

## Stable seams

| Application seam | Current source of truth | Outbound port to extract | Infrastructure adapters |
|---|---|---|---|
| Typed tool invocation and catalog | `src/typed_tools/`, `src/tool_bootstrap.rs`, `src/tui/runtime_services.rs` | Contract registration, invocation, and catalog projection ports | Stasis registry, provider schema adapter, mode projection |
| Job and attempt execution | `src/runtime_job_spec.rs`, `src/runtime_composition_ext.rs`, `src/runtime_tools.rs` | `RuntimeJobPort` for enqueue/read/attempt operations | In-memory and Surreal Stasis runtimes |
| Recurring scheduling and outbox | `src/recurring_schedule.rs`, `src/runtime_composition_ext.rs`, recurring paths in `src/runtime_tools.rs` | `RecurringSchedulerPort` and `OutboxPort` | Stasis recurring/outbox stores and scheduler |
| Recurring feed bindings | `src/recurring_feed.rs` | `FeedBindingStore` plus typed binding service | In-memory and Surreal feed stores |
| Recurring delivery resolution | `src/recurring_delivery.rs`, `src/channel_delivery.rs` | `DeliveryResolver` and `DeliveryBindingStore` | Channel-session mappings, product policy, in-memory/Surreal store |
| Artifact and vault commands | `src/artifact_command_runtime.rs`, `src/artifact_store.rs`, `src/vault/`, `src/vault_tools.rs` | Artifact read/write and vault-note ports | Filesystem, vault root, and artifact persistence adapters |
| Environment and UI composition | `src/environment_tools.rs`, `src/custom_view_tools.rs`, `src/ui_present_tools.rs`, `src/component_runtime_store.rs` | Component/feed/navigation composition ports | Environment store, component runtime store, presentation persistence |
| Turn and worker execution | `src/interactive_turn_runtime.rs`, `src/agent_runtime/`, `src/turn_continuation.rs` | Turn execution, continuation, and worker lifecycle ports | Stasis jobs, session stores, model/provider and channel adapters |

These are extraction targets, not new service names that must be introduced
all at once. A target is ready when its command/spec owns invariants, its
handler has no wire parsing, and its side effects are already expressed by a
focused existing trait or extension operation.

## Extraction order

### H1 — runtime application ports

1. Define an application-owned runtime port around the operations already
   covered by `RuntimeCompositionExt`.
2. Adapt `RuntimeComposition` to that port in one infrastructure module.
3. Move `runtime_job_spec` and `recurring_schedule` beside the application
   service, keeping constructors pure and clock/id inputs explicit.
4. Leave continuation binding, authorization, and lane policy in the caller.

Exit: runtime tools depend on the application port for common operations, and
no Stasis type crosses the application boundary except through an explicit
adapter DTO.

### H2 — recurring binding services

Extract feed binding and delivery resolution as separate services. Their
stores already have focused traits, and their policies differ: feed binding
validates feed ids and payload mode, while delivery resolution applies channel
identity and product policy. They must not be merged into one generic binding
service.

Exit: recurring registration and workflow scheduling share one schedule
service, while feed and delivery policy remain independently testable.

### H3 — artifact/vault and environment/UI ports

Extract read/write ports only after the runtime H1/H2 adapters establish the
dependency direction. Keep content-preserving HTML, source, prompt, and shell
values in application commands; filesystem paths and provider payloads stay
at their respective adapters.

Exit: custom-view and artifact handlers orchestrate commands and ports without
assembling public optional DTO fields or reaching through a runtime variant.

### H4 — turn and worker execution

Treat turn execution as the final extraction wave. It crosses the most policy
boundaries—continuation authority, capability checks, model routing, session
state, delivery, and worker lanes. Extract only parameter objects and ports
whose ownership is already clear from H1–H3.

Exit: a worker or interactive turn can be tested with fakes for session,
runtime, model, and delivery dependencies without putting runtime-owned values
into model-visible input DTOs.

## Upstream candidates

Only generic behavior belongs in the Stasis upstream review:

- typed tool macro/adapter support after the local contract fixture remains
  equivalent;
- generic runtime-composition operations when their signatures and semantics
  are independent of Medousa policy; and
- narrowly scoped error/context primitives only if they do not encode Medousa
  tool ids, modes, delivery policy, or retry decisions.

The following remain Medousa-owned: `ToolId`, placement and mode metadata,
`ToolErrorKind`, `ToolJobSpec`, `RecurringScheduleSpec`, feed and delivery
policy, Grapheme/browser opaque payload wrappers, and all product-specific
event or authority decisions.

## Gates for each extraction

- contract, schema, aliases, defaults, and invalid-input fixtures are unchanged;
- direct command tests cover missing, null, wrong-type, blank, clamp, and
  content-preserving cases;
- the service has one focused outbound port per dependency family;
- no internal typed path serializes to JSON solely to reuse a parser;
- error categories and operation/tool context reach the Stasis boundary;
- approval, lane, continuation, idempotency, and delivery policy stay at the
  application boundary; and
- focused tests, the full library suite, audit guard, and representative
  performance measurements pass before moving the next seam.

The extraction phase is complete when H1–H4 can be reviewed independently and
the next module move is a cohesive application service rather than a parser,
constructor, or backend-dispatch cleanup.
