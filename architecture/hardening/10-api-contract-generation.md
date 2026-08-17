# H10 — Generated daemon API and client contract

> **Status:** Implementing — Slice 3 (generated clients consume operation tables; Tauri streams are real). Slice 1 inventory shadow remains mergeable. Production still registers `DeclaredRouter::route(policy, handler)`, which also records `OperationSpec`. `sdk-contract/manifest.yaml` and `export_type!` stay until Slice 4. CONTRACT-001 stays **Proposed**. ADR-019 stays **Proposed** until `operation()` is the production registration path and generated clients own remaining Home/forge transports.
>
> **Accountable owner:** daemon API and SDK maintainers
>
> **Reviewers:** security, Rust/Python SDKs, Home/Tauri, streaming, docs, CI/release
>
> **Audit finding:** CONTRACT-001 (High)
>
> **Release gate:** Gate D — enforced architecture
>
> **Required decision:** [ADR-019](../../docs/architecture/decisions/adr-019-generated-api-contract.md)
>
> **Dependencies:** H01 route policy and authentication; H02 identifier/path semantics; H03 stream v2

## Outcome

Every production daemon operation is registered once with a stable operation
ID, typed wire schemas, exact HTTP encoding, H01 policy, limits, feature
availability, response/error matrix, and stream semantics. That declaration
builds the Axum router and deterministically emits the public OpenAPI contract,
client transport code, Tauri bridge vocabulary, fixtures, and reviewed route
inventory.

Rust, Python async/sync, and Home cannot handwrite verbs or `/v1` paths. CI
compares the fully assembled production router, generated artifacts, clients,
documentation, feature profiles, and black-box behavior by exact operation set.
A route cannot ship because a function with a similar name happens to exist.

H10 owns protocol declaration, generation, compatibility, and conformance. H01
owns the meaning of authentication/capabilities and supplies the policy fields.
H02 owns identifier and filesystem validation behind encoded parameters. H03
owns the v2 event state machine and replay semantics; H10 serializes and
generates clients for that contract.

## Current evidence

Authority is Axum `DeclaredRouter` / `ContractRouter` plus `medousa-types`.
Generation publishes [`sdk-contract/openapi.json`](../../sdk-contract/openapi.json)
and [`sdk-contract/route-inventory.json`](../../sdk-contract/route-inventory.json)
from that inventory. Production inventory is **369** routes without pairing and
**381** with pairing (`combined_declared_inventory_matches_optional_pairing_composition`).
CI regenerates the contract and requires byte equality plus exact-set equality
against the declared router.

`sdk-contract/manifest.yaml` is deleted. Named wire schemas in the production
registry must appear in `medousa-types.schema.json` unless they are the documented
uncatalogued exceptions (`schema_catalog_covers_named_contract_bindings`).
`PARITY_ROUTES` stays deleted; uniqueness/SSE checks run against generated ops.

Remaining copies that still shrink:

| Surface | Current evidence | Failure mode |
| --- | ---: | --- |
| Declared production inventory | 369 / 381 method+path rows | real server behavior; H01 policy; generated operation IDs |
| Generated OpenAPI + inventory | checked-in artifacts from declared inventory | named DTO/stream bindings plus catalog components; remaining ops stay opaque/`deferred` |
| Rust / Python / TS / Tauri op tables | generated from IR; helpers expand `op_path` / `operationPath` | helper no-literal tests in SDK crates; not yet required architecture CI |
| Shared golden cases | `sdk-contract/golden/client-cases.json` | wrong verb/path/stream flag fails Rust and Python unit tests |
| Browser compatibility adapter | unprefixed `BROWSER_COMPATIBILITY_MOUNTS`; `/v1` copies on `browser_surface` | unprefixed aliases stay off 369/381; `/v1` copies use explicit `ExactOrigin` |
| Stasis dashboard adapter | raw `dashboard_router` + frozen `DASHBOARD_COMPATIBILITY_MOUNTS` | third-party; no method/path descriptors; reviewed freeze stays off declared inventory |
| Tauri daemon proxy modules | closed `daemon_unary` / real `daemon_stream_start`+cancel; endpoint-shaped shims remain | live SSE uses generated stream paths; remaining proxies still own some URLs |
| Home `$lib/daemon/*` | generated ops for session stream, LSP, code intelligence, forge, browser sessions | Home no-literal CI except generated tables, tests, and local `browserBridge` |

`scripts/check-api-contract.sh` runs IR tests, production inventory equality,
released-baseline `diff_contracts`, helper/Home no-literal scans, and the
architecture check that production `/v1` string literals cannot call raw Axum
`.route`/`.nest` outside reviewed adapters. `scripts/check-sdk-contract.sh`
wraps that gate.

`medousa-types` already derives `serde` and optional `schemars::JsonSchema` for
many wire types. `medousa-types-schema` exports schemars for names bound by the
daemon contract. CI requires those named non-opaque schemas to appear in
`medousa-types.schema.json` unless they are documented uncatalogued exceptions.

## Invariants

1. One typed operation declaration supplies router registration and all derived
   artifacts; a path or verb has no second editable owner.
2. Every reachable production method has one globally stable operation ID and
   reviewed H01 route policy. Unknown policy is a build error, never a default.
3. The assembled router operation set equals the generated contract exactly for
   every supported feature profile. No minimum counts or allowlisted omissions.
4. Path, query, header, cookie, and body parameters have distinct typed
   representations and generated encoders. Query text is never embedded in a
   path template.
5. Path values are encoded per segment. Catch-all path parameters are explicit,
   use H02 validated domain types, and cannot be produced by a generic string
   substitution helper.
6. Every success and expected failure status has a media type and schema. Raw
   ad-hoc string/HTML errors are not part of `/v1`.
7. Wire schemas match `serde` behavior, including names, tags, defaults,
   optional versus nullable fields, unknown-field policy, and numeric bounds.
8. Streams are HTTP GET operations with `text/event-stream`, a typed event
   union, framing, replay, heartbeat, terminal, gap, cancellation, and resource
   limits. `SSE` is not an invented verb.
9. Generated low-level clients own URL construction, serialization,
   deserialization, status mapping, and stream parsing. Handwritten ergonomic
   wrappers cannot contain daemon route literals or raw HTTP methods.
10. Rust async, Python async/sync, and Home/native transports consume the same
    operation semantics and golden fixtures.
11. Generated artifacts are deterministic, formatted, provenance-marked, and
    reproducible with the locked workspace toolchain and no network access.
12. Compatibility is checked semantically. A breaking change cannot hide in a
    reordered JSON file, regenerated client diff, or unchanged URL.
13. Contract generation never executes application initialization, opens user
    storage, loads credentials, contacts providers, or starts optional runtime
    workloads.
14. Secrets and realistic user data never enter checked-in examples, fixtures,
    generated docs, logs, or contract-diff output.

## Non-goals

- generating business handlers, state ownership, authorization decisions, or
  database implementations;
- claiming generated code makes a semantically bad API good;
- exposing internal Rust structs merely because they derive `JsonSchema`;
- preserving every current SDK method name or Tauri command indefinitely;
- treating OpenAPI validation as a substitute for H01/H02 abuse tests;
- making a contract hash mismatch fatal when two builds remain compatible; or
- representing planned routes as if a client could call them.

## Canonical ownership model

### Typed route registry

Introduce a dependency-light `medousa-api-contract` crate for the protocol IR,
schema traits, and generator. An Axum adapter in the daemon owns
`ContractRouter`. Each feature exports an `ApiModule` that registers
`OperationSpec` values and method routers through that one builder:

```rust
fn register(api: &mut ContractRouter<AppState>) {
    api.operation(
        operation::GET_SESSION_HISTORY,
        get(get_session_history),
    );
}
```

`ContractRouter::operation` both adds the Axum route and records the exact
descriptor. Direct `.route`, `.route_service`, `.nest`, or untracked merge on a
production API router is forbidden by an architecture check. State-only
composition may merge `ApiModule` results whose inventories are preserved.
Third-party routers such as the dashboard require an explicit reviewed adapter
that supplies an inventory; they are not opaque exemptions.

`ContractRouter` keeps its inner Axum router private and accepts no raw `Router`
merge. Only the root composition can consume it into an Axum service after all
modules and outer policies are installed. Framework-generated `HEAD`, CORS
preflight `OPTIONS`, fallback, redirect, and method-not-allowed behavior are
declared centrally and included in policy/conformance evidence; they cannot
become unreviewed side doors merely because they are not SDK operations.

An operation descriptor includes at least:

```text
operation_id, stability, feature/profile, audience
method, path template, path/query/header/cookie parameters
request bodies by media type and byte/depth/item limits
responses by status/media type, shared error codes, retry semantics
H01 trust group, credential scheme, capabilities, resource scope
browser-origin policy, rate/concurrency class, idempotency/preconditions
stream item schema and H03 lifecycle, when applicable
deprecation/replacement/removal metadata
```

IDs are semantic and never derived from Rust module/function names. Renaming a
handler does not change `sessions.getHistory`; changing an operation ID is a
reviewed compatibility event. Paths use one canonical parameter name; generated
language APIs may adapt casing without changing the wire name.

The declaration is the editable authority. A deterministic generator emits:

- `sdk-contract/openapi.json`, using OpenAPI 3.2 and JSON Schema 2020-12;
- `sdk-contract/route-inventory.json`, including H01 and feature-profile data;
- Rust route constants/low-level SDK operations;
- Python async and sync low-level operations and models;
- TypeScript models, operation inputs/results, and native bridge calls;
- conformance cases, serialization examples, error/SSE fixtures, and doc data.

The checked-in JSON artifacts are reviewable publication and compatibility
baselines, not hand-editable competing sources. Generated headers point to the
owning declarations and command. CI regenerates into a temporary directory and
requires a byte-for-byte clean diff.

### Wire schema authority

Public DTOs remain explicit `medousa-types` wire types with aligned `serde` and
JSON Schema annotations. The operation registry references only request,
response, parameter, error-detail, and stream-event types that implement the
contract schema trait. The generator walks referenced types; there is no manual
global `export_type!` list.

Generation fails for duplicate component names, unresolved references,
unconstrained `serde_json::Value`, ambiguous maps, unsupported custom
serialization, untagged unions without unambiguous validation, or schema/serde
features the generators cannot reproduce. An intentionally opaque payload uses
a named bounded wrapper with documented semantics, never silent `JsonValue`.

Schemas declare object closure policy, required/default/null behavior, formats,
minimum/maximum lengths and counts, discriminators, and sensitive/write-only
fields. Synthetic examples cover awkward Unicode and boundaries without real
paths, prompts, tokens, profile data, or model output.

Schema derivation alone cannot prove custom serialization. Each referenced wire
type gets round-trip fixtures, and custom/manual serializers get focused golden
and negative tests. Representative generated instances are validated against
the schema after serialization and before deserialization in every language.

## HTTP and error contract

Generated encoders use a structured URL builder. Each path segment is
percent-encoded once; query parameters use declared style/explode rules and
omit/default semantics; headers are typed separately. Tests include `/`, `%`,
`?`, `#`, spaces, empty strings, composed/decomposed Unicode, emoji, dot
segments, Windows-looking separators, and maximum lengths.

Catch-all vault/media paths are never generic SDK `String` parameters. Their
operation metadata names H02's validated relative-path type and serialization
rules. Server extraction validates before filesystem access; a client encoder
cannot confer authority.

Adopt one versioned JSON error envelope for `/v1`:

```json
{
  "schema_version": 1,
  "code": "invalid_parameter",
  "message": "request parameter was rejected",
  "request_id": "req_...",
  "details": {},
  "retry_after_ms": null
}
```

`code` is a stable machine vocabulary with a generated exhaustive core and a
documented forward-compatible unknown case in clients. `message` is bounded,
safe for users, and not parsed by clients. Details use a code-specific schema
and contain no secrets/internal paths. Each operation enumerates possible
statuses/codes, including authentication, authorization, conflict/generation,
rate/concurrency, payload-too-large, unavailable feature, and internal failure.
Unexpected panics/framework rejections are normalized at the outer API layer.

Request IDs cross daemon, SDK error, Tauri bridge, and diagnostics. Retry
metadata is explicit. Generated clients do not retry unsafe operations merely
because a transport failed; idempotency and retry policy are declared per
operation.

## Streaming contract

OpenAPI 3.2 models each stream as a normal GET response with
`text/event-stream` and an `itemSchema`. Medousa adds a versioned
`x-medousa-stream` extension because OpenAPI does not define application-level
terminal/replay semantics. The generator owns and validates the extension
schema; unknown fields fail generation.

For every stream it records:

- tagged item union and `event` name mapping;
- required SSE `id`, JSON `data`, optional `retry`, and comment heartbeat rules;
- event/frame/line byte limits and decoder depth/item limits;
- canonical resume input (`Last-Event-ID` or typed query parameter), initial
  sequence, replay window, live-join fence, duplicate handling, and gap/reset;
- terminal success/failure/cancel variants and whether reconnect is allowed;
- authentication/revocation recheck and disconnect/cancellation behavior; and
- idle timeout, subscriber/replay admission, and backoff bounds.

Generate one incremental SSE codec/state machine per language target, shared by
all operations. Fixtures split UTF-8 and CRLF boundaries across arbitrary
chunks, use multiline data, comments, empty fields, duplicate IDs, malformed
JSON, oversized frames, unknown future variants, reconnect/replay overlap, gaps,
terminal events, and cancellation. H03 remains the semantic authority for turn
stream v2; H10 prevents its encoders/decoders from drifting.

## Generated clients and Home bridge

### SDK layers

Each SDK has three layers:

1. handwritten transport/session policy: base URL, H01 credential injection,
   TLS/Iroh adapter, timeouts, request IDs, cancellation, and observability;
2. generated low-level operations/models/encoders/decoders; and
3. optional handwritten ergonomic helpers that call generated operations only.

The Rust SDK may reuse `medousa-types` directly where feature/dependency policy
allows. Python models and async/sync surfaces are emitted from the same IR. The
sync Python client delegates to generated sync transport operations; it is not a
second copied SDK. Public helper compatibility is tested against a generated
mapping during migration.

A lint rejects daemon route literals and raw HTTP method construction outside
the generator, daemon registry, approved generic transports, tests, and
compatibility shims with removal tickets. It also rejects manual URL formatting
in generated-client call sites.

### Tauri and TypeScript

The native layer keeps H01 credentials out of the webview. Replace endpoint-
shaped handwritten Tauri commands with a small native transport vocabulary:

- `daemon_unary` accepting a generated closed operation enum and generated
  operation input;
- `daemon_stream_start` accepting a generated stream operation enum/input and
  returning a request-correlated handle;
- `daemon_stream_cancel` for that exact handle; and
- connection/capability commands that are native state, not daemon HTTP copies.

The Rust dispatcher matches the generated closed enum to generated SDK calls;
it never accepts an arbitrary verb or URL from JavaScript. The TypeScript
generator exposes typed per-operation functions, so application code does not
construct the generic envelope or use raw `invoke` names. Streams emit generated
typed events correlated by handle and connection generation. H08 capabilities
permit these commands only to the trusted shell; remote browser webviews receive
none.

Native-only features remain separately declared Tauri commands and are not
misrepresented as daemon operations. The generated inventories distinguish
daemon HTTP, trusted native transport, and native-only application authority.

## Feature profiles and discovery

The registry describes the canonical superset, while the builder records the
exact enabled profile. Core, optional workload, platform, development, and
test-only modules cannot silently change the surface. CI generates and compares
every supported release profile; development-only diagnostics are absent from
release contracts and still require explicit policy.

An absent optional feature means its routes are not mounted. Discovery reports
feature/capability availability separately from route existence; `planned`
operations never enter the callable contract. Home uses H09/H11 capability
metadata to offer installation or degraded UX.

Minimal authenticated compatibility metadata reports API major/minor,
contract digest, build profile, and supported capabilities. A digest difference
is diagnostic, not automatic incompatibility. Clients negotiate API major and
required capabilities; compatible additive changes continue to work.

## Compatibility policy

Maintain a machine-readable released baseline and classify semantic diffs:

| Change | Default classification |
| --- | --- |
| New optional operation/status/event variant with forward-compatible client | additive |
| New optional response field | additive only when clients ignore unknown fields |
| New optional request field with server default | additive |
| New required field/parameter, narrower bound, removed status/variant | breaking |
| Verb/path/auth/capability/resource-scope change | breaking and security review |
| Optional → required, nullable removal, type/format/discriminator change | breaking |
| Error code addition | additive only through generated unknown-code handling |
| Stream ordering/replay/terminal semantic change | breaking unless explicitly compatible |

Breaking public changes require a new API major/path or an approved bounded
migration with simultaneous old/new operations, deprecation metadata, telemetry,
removal release, and canonical docs. Regeneration is never approval by itself.
The diff report names affected operation IDs, clients, bridge calls, fixtures,
and docs.

## Verification

### Static and generation gates

- build every supported API feature profile without application side effects;
- reject duplicate method/path pairs, operation IDs, schema names, missing H01
  policy, unbounded inputs, missing responses/errors, and undocumented streams;
- generate twice in clean temporary directories and require identical bytes;
- require checked-in generated artifacts to match regeneration;
- compare assembled router inventory to contract by exact operation ID, method,
  path, policy, and profile set;
- run semantic compatibility diff against the released baseline;
- compile/type-check generated Rust, Python async/sync, TypeScript, and Tauri
  dispatcher outputs; and
- prohibit route literals/manual transports outside reviewed owners.

The router equality check must observe the production assembly boundary. It
combines the sealed `ContractRouter` construction invariant, a source check for
raw router escapes, and black-box probes through the final layered service. It
cannot compare two artifacts emitted from the same incomplete list and call
that proof.

### Black-box conformance

Start a real daemon router on an ephemeral listener with isolated temporary
state and deterministic fake provider/runtime ports. Generated cases exercise
every operation with valid, boundary, malformed, unauthenticated, wrong-
capability, and unsupported-feature inputs. Tests assert status, content type,
body schema, error code/request ID, side-effect admission, and response bounds.

Run the same golden server suite through Rust, Python async, Python sync, and
the trusted native/TypeScript bridge. It must catch deliberately mutated verb,
path, parameter location/encoding, auth policy, request schema, response status,
error shape, and stream event. Keep mutation sentinels so a test that ceases to
detect drift fails CI.

Response-schema validation runs in tests and sampled non-production diagnostics,
not as unbounded production hot-path work. Fuzz/property suites target URL
encoding, serde/schema equivalence, error normalization, and incremental SSE
framing with explicit corpus/time/memory limits.

### Documentation gates

Generate the operation index, parameter/status tables, and SDK method reference
from the contract. Handwritten guides explain workflows and link stable
operation IDs; they do not copy full tables. `scripts/verify-docs.sh` checks
links, generated markers, examples, and SDK snippets against compilable fixtures.

## Observability and diagnostics

Bounded metrics use operation ID, result class, transport, and feature profile;
never raw path, identifier, URL, error message, or credential. Record contract
major/digest, unknown response/event/error variants, encoding/validation
failures, bridge correlation failures, and client/server capability mismatch.

Development diagnostics can return the authenticated route inventory and
contract digest, but production does not expose admin route/policy details to
anonymous callers. Generator failures identify source declaration and field;
they never dump synthetic or captured secret-bearing bodies.

## Migration and rollout

### Slice 1 — inventory without behavior change

1. Add `medousa-api-contract`, the operation/stream/error IR, deterministic
   generator, OpenAPI 3.2 validation, and source provenance.
2. Import every real production route from Axum composition, including
   third-party/optional modules, with H01 policy marked explicitly.
3. Reconcile `manifest.yaml`, `PARITY_ROUTES`, SDKs, docs, and actual router into
   one discrepancy report. Resolve behavior from code and reviewed product
   intent; never silently pick the largest list.
4. Generate contract/inventory alongside existing clients and run shadow exact-
   set plus black-box tests. Do not delete old paths yet.

### Slice 2 — schemas, errors, and streams

1. Reference wire DTOs from operations and replace the manual schema export
   list with reachable-schema traversal.
2. Introduce the versioned error envelope at the outer boundary and migrate
   operations by feature with compatibility tests.
3. Encode all existing streams accurately, then bind H03 stream v2 and generated
   codecs. Reject the fake `SSE` verb convention.
4. Publish the generated OpenAPI and documentation tables.

### Slice 3 — generated clients

1. Generate Rust low-level operations, migrate helpers, and prohibit literals.
2. Generate Python async/sync surfaces and retain intentional public aliases
   through a bounded deprecation map.
3. Generate TypeScript models/operations and the closed native dispatcher;
   migrate feature by feature from endpoint-shaped Tauri commands.
4. Run old/new clients against the same conformance server until results match.

### Slice 4 — make drift impossible

Delete `sdk-contract/manifest.yaml`, `PARITY_ROUTES`, regex/function-existence
parity tests, manual `export_type!` inventory, copied SDK routes, endpoint-shaped
Tauri proxy commands, copied `daemon.ts` DTOs/wrappers, and obsolete generated
compatibility shims. Enable exact generation, router, semantic-diff, black-box,
and no-literal gates as required CI.

Rollout may retain an old generated API major, never an old handwritten source
of truth. Rollback ships the previous generator input/artifacts/router/client set
together. Mixing a reverted daemon with newer incompatible generated clients is
not a rollback plan.

## Exit criteria

CONTRACT-001 is eligible for **Validated** only when:

- every supported production profile has exact router/contract equality and no
  raw route registration outside the registry;
- the generated OpenAPI passes its pinned schema/lints and contains policy,
  limits, status/error, and stream metadata for every operation;
- Rust, Python async/sync, and trusted Home/native clients contain no daemon
  verb/path copies and pass the shared black-box suite;
- schema/serde, awkward-parameter, auth matrix, error, SSE fragmentation/replay,
  feature-profile, semantic-diff, deterministic-generation, and mutation
  sentinel tests pass;
- the old manifest, parity table/checker, manual schema list, endpoint proxy
  duplication, and copied docs tables are deleted; and
- canonical SDK/engine docs and contributor instructions ship with a retained
  released contract baseline and rollback artifact.

## Canonical documentation changed at ship time

- `docs/engine/http-api.md` and authentication/error/streaming references;
- `docs/sdk/overview.md`, `docs/sdk/api-reference.md`, transport and Python docs;
- Home app/connection documentation for native credential transport;
- `docs/README.md`, contributor/CI instructions, and SDK compatibility policy;
- generated OpenAPI/reference publication and release notes for migrations.

## Code anchors

- `src/daemon/router.rs` and feature router modules
- `crates/medousa-types/` and `crates/medousa-types-schema/`
- `sdk-contract/manifest.yaml`
- `crates/medousa-sdk/src/` and `crates/medousa-sdk/tests/contract_parity.rs`
- `python/medousa-sdk/src/medousa/` and Python parity tests
- `apps/medousa-home/src-tauri/src/daemon/`
- `apps/medousa-home/src/lib/daemon.ts`
- `scripts/check-sdk-contract.sh` and `scripts/verify-docs.sh`

## Standards basis

- [OpenAPI Specification 3.2.0](https://spec.openapis.org/oas/v3.2.0.html)
- [OpenAPI Server-Sent Events media-type guidance](https://spec.openapis.org/registry/media-type/sse)
