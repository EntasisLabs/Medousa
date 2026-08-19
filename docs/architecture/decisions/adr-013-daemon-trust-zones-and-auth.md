# ADR-013: Daemon trust zones and mandatory authentication

> **Status:** Accepted
>
> **Date:** 2026-08-13
>
> **Decision owners:** daemon and Home maintainers
>
> **Related:** [ADR-003](adr-003-multi-workshop-connections.md), [ADR-011](adr-011-shared-mode-portal-and-mesh.md), [H01 execution plan](../../../architecture/hardening/01-daemon-trust-and-auth.md)

## Context

`medousa_daemon` is an authority boundary, not a read-only status service. Its
assembled Axum router can start turns, read and mutate vault data, manage
packages and models, approve operations, control runtime state, and reach
process- and filesystem-adjacent features.

Before this decision was implemented, the boundary did not match that authority:

- Personal mode permits unauthenticated non-loopback requests to almost the
  entire router.
- Authentication middleware is installed only when the pairing service is
  available. Disabling or failing to initialize pairing can therefore remove
  protection instead of failing closed.
- A handwritten path classifier determines route authority after independent
  routers have been merged. A new or aliased route can acquire unintended
  access by omission.
- Loopback peer IP is treated as proof of a trusted local caller. Any local
  process, browser origin, or port-forwarded request can also reach loopback.
- `CorsLayer::permissive()` is applied to the full router. CORS is not
  authentication, but this setting makes browser-based probing and misuse of
  any ambient authority easier.
- Pairing credentials are resolved repeatedly from headers by middleware and
  handlers, which permits identity decisions to drift.

Iroh marks proxied traffic so it is not mistaken for loopback, but a transport
marker is not a caller credential. Personal versus Shared mode is a product
and tenancy choice; it is not an authentication mode.

The default local experience must remain Home-first: a desktop user launches
Medousa and can chat without manually pairing the app to its own daemon. That
constraint requires an automatic local credential, not anonymous local HTTP.

## Decision

### 1. Authentication is independent of product mode and pairing availability

Every non-bootstrap daemon request must produce an authenticated principal
before its handler runs. This rule applies in Personal and Shared mode, over
LAN, Iroh, reverse proxy, and loopback.

The protected router always has authentication and authorization middleware.
Pairing disabled, unavailable, or corrupt must never expose it anonymously.
If a non-loopback listener cannot initialize the required security services,
startup fails. A loopback recovery mode may expose only a separately assembled
bootstrap/diagnostic router; it does not inherit the application router.

### 2. The daemon recognizes explicit principals

Authentication produces exactly one request principal:

| Principal | Credential source | Intended authority |
| --- | --- | --- |
| `local_app` | Per-install local capability | Explicit local operator capabilities |
| `portal` | Valid paired portal bearer | Workshop/member-scoped portal capabilities |
| `peer` | Valid paired peer bearer | Peer, share, mesh, and heartbeat capabilities |
| `root` | Administrative local or paired credential | Explicit administrative capabilities |
| `preview` | Short-lived resource-specific token | One preview/resource surface only |
| `anonymous` | No credential | Active bootstrap ceremony and minimal liveness only |

An invalid, expired, revoked, duplicated, or ambiguous credential authenticates
as nobody. The daemon does not fall back to a less-specific or loopback
identity after credential failure.

The authenticated principal, transport class, credential identifier, bound
profile, capabilities, and revocation generation are stored once in request
extensions. Handlers consume that typed context; they do not parse
`Authorization` or independently infer identity.

### 3. Local-app authority uses a credential, not source IP

The daemon creates a random local capability during installation/first secure
startup and stores only the material needed to verify it. Home obtains the
credential through the daemon-owned platform credential store
(`com.entasislabs.medousa.secrets.daemon`) or an owner-only file under the
Medousa data directory. Pairing tokens for remote workshops live in a separate
client service (`com.entasislabs.medousa.secrets.client`). Provider, bot, STT,
and Surreal material is written only by the daemon (generated
`/v1/integrations*` and `/v1/auth/chatgpt*` ops); Home never receives secret
values on the HTTP wire. macOS Keychain therefore prompts at most twice on
first write (daemon service + client service), then stays silent. The local
credential is attached by the Tauri/native transport, never exposed to the
webview JavaScript runtime.

Other same-user native clients may enroll for their own named local capability.
Credentials are individually revocable and rotatable. File permissions are a
defense in depth and enrollment mechanism, not the request authentication
decision itself.

Raw loopback trust was permitted only as a bounded migration state while
first-party clients adopted named credentials. That compatibility path is now
deleted: protected loopback requests without a credential receive `401`, and no
permanent `--no-auth` production mode exists.

### 4. Routes declare policy where they are assembled

The daemon is composed from separately named route groups:

- `bootstrap`: minimal liveness and an explicitly active pairing ceremony;
- `portal`: ordinary interactive, session, vault, artifact, and workspace
  operations with ownership checks;
- `peer`: share, inbox, mesh, and heartbeat operations;
- `admin`: packages, models, runtime configuration, maintenance, profile/root,
  approval, shell/process, and similarly privileged operations; and
- `preview`: narrowly tokenized resource delivery.

Each method/path template carries policy metadata: required capability,
bootstrap visibility, browser request policy, body limit, and rate-limit class.
The production assembly exports this inventory for contract tests. A route
without policy metadata fails a build/test check and remains unreachable in
the default router. Authorization is capability-based; a path-prefix string
classifier is not the source of truth.

Unknown routes and undeclared methods fail closed. `HEAD`, `OPTIONS`, trailing
slash aliases, nested routers, and alternate content types do not acquire a
weaker policy than the declared operation.

### 5. Bootstrap is small, conditional, and bounded

Unauthenticated access is limited to:

- a constant-size liveness/version response with no configuration, paths,
  identities, provider state, or workload counts; and
- the minimum pairing initiation and verification operations while an explicit,
  expiring pairing window is active.

QR generation, rotation, status, device lists, Iroh ticket inspection,
revocation, and detailed health are authenticated operations. Pairing
bootstrap has request/body/concurrency limits, per-source and global rate
limits, expiring single-use challenges, and non-enumerating errors.

### 6. Transport identity never grants application authority

LAN, Iroh, loopback, and explicitly configured trusted proxies are transport
classes recorded for policy and diagnostics. They do not replace a caller
credential. The Iroh gateway strips caller-supplied transport metadata and
adds internal metadata, but the protected router still authenticates the
request.

Forwarded headers are ignored unless the immediate peer is an explicitly
configured trusted proxy. Host validation prevents DNS-rebinding and accidental
virtual-host exposure. TLS or Iroh encryption protects transport; it does not
change route capabilities.

### 7. Browser access is explicit and non-ambient

The full router no longer uses permissive CORS. Browser clients are disabled by
default. Supported development or browser origins use an exact origin, method,
and header allowlist; wildcard origins, reflected origins, `Origin: null`, and
credentialed wildcard responses are forbidden.

All browser requests still require a credential. Mutating browser requests also
pass origin/host checks and use a non-ambient authorization header; cookies are
not a daemon authentication mechanism. Simple content types are rejected for
JSON mutation routes so a form post cannot bypass preflight and request policy.
SSE and future WebSocket handshakes apply the same origin and principal checks
before allocating stream work.

The desktop webview talks to the daemon through Tauri/native commands. Local
daemon credentials remain outside JavaScript and browser storage.

### 8. Credential lifecycle is part of authorization

Bearer and local credentials are random, hashed at rest where the verifier
permits, redacted from logs, never accepted in URLs, and scoped to a device and
capability set. Issuance, expiry, rotation, revocation, and last-use auditing
are explicit operations.

New HTTP work checks current revocation state. Long-lived streams revalidate at
a documented bounded interval or subscribe to a revocation generation and
close before accepting more work. Revocation has no anonymous grace path.

Missing or invalid credentials return `401`; authenticated principals lacking
a capability return `403`. Responses expose a stable reason class without
confirming whether a specific secret or identity exists.

## Consequences

### Positive

- Binding beyond loopback no longer converts the daemon into an anonymous
  remote-control service.
- Adding a route requires an explicit, reviewable authority decision.
- Personal and Shared mode share one comprehensible security boundary.
- Authentication and identity enrichment happen once per request.
- Home keeps automatic local startup without treating every local process and
  browser as Medousa.

### Costs and migration

- Home, CLI, TUI, SDK transports, SSE, multipart, and raw-upload paths must all
  learn the local or paired credential contract.
- Legacy loopback clients must migrate to a named local-app or paired credential;
  credentialless protected requests now fail with `401`.
- Route composition changes are broad and require an assembled-router inventory
  test, not only policy-helper unit tests.
- Credential rotation and live-stream revocation add state and operational work.
- Exact development origins need explicit configuration.

### Superseded or narrowed decisions

- ADR-003's statement that local workshops require no pairing is preserved only
  as a user ceremony statement. Local workshops require an automatically
  provisioned local-app credential even though users do not scan a QR.
- ADR-003's paired session-token model remains valid and becomes mandatory for
  remote portal traffic.
- ADR-011's consequence “Personal daemons unchanged until Shared mode is
  enabled” is superseded for authentication and network exposure. Personal
  mode retains its product behavior, but it is not anonymously accessible.
- ADR-011's Iroh transport-marker rule remains necessary but is insufficient on
  its own; all Iroh application requests also authenticate.

## Verification

Acceptance and implementation are governed by the [security abuse matrix](../../../architecture/hardening/verification/security-abuse-matrix.md),
especially DA-001 through DA-012 and WEB-001 through WEB-010. Closure requires
tests against the production-assembled router on loopback and a real
non-loopback socket, plus browser-driven origin tests.

## Code anchors

- `src/bin/medousa_daemon.rs` — listener and final router assembly
- `src/daemon/router.rs`, `src/daemon/request_boundary.rs` — application router,
  exact browser-origin policy, and socket-edge Host validation
- `src/peer_scope.rs`, `src/portal_acl.rs` — current conditional middleware and
  handwritten route classification
- `src/remote_trust.rs`, `src/iroh_transport/gateway.rs` — transport metadata
- `src/pairing/service.rs`, `src/pairing_handlers.rs` — credential and ceremony
- `src/credential_lifecycle.rs`, `src/local_credential_handlers.rs` —
  generation revocation, stream leases, audit evidence, and local operations
- `apps/medousa-home/src-tauri/src/daemon/` — Home daemon client
- `apps/medousa-home/src-tauri/src/workshop_transport.rs` — LAN/Iroh auth
- `crates/medousa-sdk/`, `crates/medousa-sdk-iroh/` — shared transports
