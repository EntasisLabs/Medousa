# ADR-019: Route-owned generated API and client contract

> **Status:** Proposed
>
> **Date:** 2026-08-13
>
> **Decision owners:** daemon API, SDK, and Home transport maintainers
>
> **Related:** [ADR-013](adr-013-daemon-trust-zones-and-auth.md), [ADR-014](adr-014-identifier-and-filesystem-authority.md), [ADR-015](adr-015-bounded-durable-turn-pipeline.md), [H10 execution plan](../../../architecture/hardening/10-api-contract-generation.md)

## Context

Medousa currently describes overlapping portions of its daemon API in Axum
router literals, `sdk-contract/manifest.yaml`, a handwritten Rust parity table,
Rust SDK modules, Python async/sync modules, Tauri proxy commands, TypeScript
wrappers/types, and documentation. The alleged contract is not an input to the
server or clients. Its checker greps for similarly named functions and cannot
detect the wrong verb, path, encoding, body, response, error, authentication, or
stream behavior.

The copies already disagree. The parity table includes callable session and
agent methods absent from the YAML manifest, uses different placeholder names,
embeds query syntax in one path, and treats SSE as a pseudo method. The schema
exporter has another manual type list unrelated to route/status reachability.

A hand-edited OpenAPI document would improve vocabulary but remain a second
source beside the router and Rust wire types. Deriving a document only from
handlers would omit product/security semantics that Rust signatures do not
contain. Generating clients without black-box verification could faithfully
reproduce a wrong or incomplete declaration.

Medousa also has constraints a generic client generator does not own: H01 trust
groups/capabilities and input limits, H02 validated catch-all paths, H03
terminal/replay SSE semantics, optional workload feature profiles, native
credential custody, and a hostile-webview boundary.

OpenAPI 3.2 is the publication format because it uses JSON Schema 2020-12 and
can model sequential `text/event-stream` items. It deliberately does not define
Medousa's application-level stream lifecycle, so checked versioned extensions
are still required.

## Decision

### 1. Route declaration and registration have one owner

Create a dependency-light Rust contract crate for the protocol IR/schema traits
and a daemon-owned Axum `ContractRouter` adapter. Each API feature registers a
stable typed `OperationSpec` together with its Axum method router. That one call
both assembles the production router and records its contract. The adapter keeps
the raw router private; direct untracked route/nest/service registration and raw
router merges on production API routers are forbidden.

The operation declaration includes stable operation ID, method/path, typed
parameters and bodies, responses/errors, content types, H01 policy/capability,
limits, idempotency/retry behavior, feature/profile, deprecation, and stream
metadata. There is no permissive default for omitted security or limits.

Third-party/optional routers must use a reviewed inventory adapter. The fully
assembled release router and generated operation set must be exactly equal for
each supported build profile. Framework-provided `HEAD`, CORS `OPTIONS`,
fallback, redirect, and method-not-allowed behavior is declared and tested at
the outer boundary even when it is not exposed as an SDK operation.

### 2. Rust wire types own schemas; reachable operations own inventory

Operations reference explicit `serde` + JSON Schema wire types. Generation
walks only reachable request, response, parameter, error-detail, and stream
types. The manual global type-export list is removed.

Generation rejects ambiguous or unsupported serialization, duplicate names,
unbounded opaque values, and incomplete metadata. Round-trip/golden tests cover
custom serializers and prove representative serialized values against their
schemas. Derivation is not assumed to prove runtime equivalence by itself.

### 3. OpenAPI is generated publication, not editable authority

The locked Rust generator deterministically emits a checked-in OpenAPI 3.2 JSON
document, reviewed route/policy inventory, clients/models, native bridge
vocabulary, fixtures, and documentation data. Generated files contain
provenance and are never edited directly. CI regenerates offline into a clean
directory and requires byte equality.

Use standard OpenAPI/JSON Schema fields wherever they express the contract.
Versioned, schema-checked `x-medousa-*` extensions cover H01 policy/limits,
feature profiles, sensitive handling, and H03 stream lifecycle that OpenAPI does
not model. Clients are generated from Medousa's validated intermediate contract,
not by trusting arbitrary third-party OpenAPI input or generator defaults.

### 4. Clients own no wire literals

Generate low-level Rust, Python async/sync, and TypeScript operations, URL
encoders, models, errors, and incremental SSE codecs. Handwritten transports own
credential injection, connection adapters, timeouts/cancellation, and metrics.
Ergonomic wrappers may call generated operations but cannot contain daemon
verbs, `/v1` paths, or alternative serializers.

The Tauri native layer retains local credentials. Endpoint-shaped proxy
commands are replaced with generated closed unary/stream operation enums behind
a small native dispatcher; JavaScript cannot submit an arbitrary method or URL.
Generated typed TypeScript functions hide the generic envelope. Native-only
application commands remain a separate authority inventory, and H08 denies all
daemon transport commands to remote content webviews.

### 5. Conformance includes behavior and compatibility

Exact static equality is necessary but insufficient. A real assembled router
runs on an ephemeral listener with deterministic fake service ports. The same
generated cases exercise it through Rust, Python async/sync, and Home/native
transports and validate encoding, auth/capabilities, statuses/media types,
errors, schemas, feature absence, and SSE fragmentation/replay/terminal rules.
Mutation sentinels prove the suite detects intentional drift.

Semantic contract diff classifies breaking changes. Breaking public changes
require a new API major or a bounded dual-operation deprecation migration. A
contract digest is diagnostic; compatibility is negotiated by API major and
required capabilities, not raw hash equality.

## Consequences

### Positive

- A new reachable route cannot omit reviewed auth, bounds, schemas, errors, or
  client/docs impact.
- Router, SDK, Home bridge, and docs stop copying methods and paths.
- Parameter encoding and stream parsing are implemented and tested once per
  language rather than once per endpoint.
- Optional features produce explicit profile differences instead of mysterious
  runtime 404s or planned fake methods.
- Semantic diffs and black-box mutation tests catch errors that generated-file
  equality alone cannot.

### Costs

- Every existing route and wire type must be inventoried and discrepancies
  resolved before enforcement can turn on.
- `ContractRouter` adapters are required for third-party routers and unusual
  multipart/raw/SSE endpoints.
- Schema annotations, uniform errors, operation IDs, and compatibility policy
  become maintained public API.
- Generated outputs add toolchain and review surface; deterministic offline
  generation and pinned schemas/generators become release dependencies.
- Migrating Tauri proxies and public SDK helpers requires a staged compatibility
  window rather than a flag-day rewrite.

### Rejected alternatives

- **Keep YAML plus stronger grep tests:** it remains disconnected from router
  construction and cannot express or prove behavior.
- **Hand-author OpenAPI as the source:** it duplicates Axum binding and Rust
  serialization, moving rather than eliminating drift.
- **Infer everything from handler signatures:** signatures do not encode H01
  policy, all statuses/errors, limits, feature profiles, or stream lifecycle.
- **Generate clients and trust them:** generator agreement can reproduce the
  same bad declaration; black-box and mutation evidence remain mandatory.
- **Expose one arbitrary Tauri HTTP proxy:** it would move URL drift into
  JavaScript and create an unnecessarily broad native capability.

## Relationship to prior decisions

ADR-013 supplies mandatory route groups, principals, capabilities, browser
policy, and admission metadata. ADR-019 makes those fields structurally required
and generated; it does not redefine their security meaning.

ADR-014 supplies validated identifiers and handle-relative path authority. A
generated percent-encoder improves correctness but never makes an unvalidated
catch-all path safe.

ADR-015/H03 owns bounded durable stream v2, sequence, replay, terminal, and gap
semantics. ADR-019 publishes that state machine and generates codecs/fixtures;
it does not create another stream protocol.

ADR-019 replaces the handwritten SDK parity convention represented by
`sdk-contract/manifest.yaml`, `PARITY_ROUTES`, function-name grep checks, manual
schema inventory, copied client routes, and endpoint-shaped Home proxy copies.

## Verification

H10 Slice 1 is inventory shadow: IR generation, exact declared-router equality,
and architecture grep for raw `/v1` Axum literals. `sdk-contract/manifest.yaml`
remains until generated clients own SDK accessors. `PARITY_ROUTES` is deleted in
favor of uniqueness checks on generated ops. CONTRACT-001 is not Mitigated until
Slice 4 gates (regen-and-diff, no-literal lint, black-box suite, old SoT deleted
after those proofs) land.

## Code anchors

- `src/daemon/router.rs`, `src/daemon/route_policy.rs`, `src/daemon/contract.rs`
- `crates/medousa-api-contract/`
- `crates/medousa-types/` and `crates/medousa-types-schema/`
- `sdk-contract/openapi.json` and `sdk-contract/route-inventory.json`
- `crates/medousa-sdk/` and `python/medousa-sdk/`
- `apps/medousa-home/src-tauri/src/daemon/contract_bridge.rs`
- `apps/medousa-home/src/lib/daemon/`
- `scripts/check-api-contract.sh`

## Standards basis

- [OpenAPI Specification 3.2.0](https://spec.openapis.org/oas/v3.2.0.html)
- [OpenAPI Server-Sent Events media-type guidance](https://spec.openapis.org/registry/media-type/sse)
