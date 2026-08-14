# H01 — Daemon trust zones and authentication

> **Status:** Implemented; supported-platform release validation pending
>
> **Accountable owner:** daemon maintainers
>
> **Reviewers:** Home/Tauri, SDK, security, release engineering
>
> **Audit finding:** SEC-001 (Critical)
>
> **Release gate:** Gate A — contain authority
>
> **Required decision:** [ADR-013](../../docs/architecture/decisions/adr-013-daemon-trust-zones-and-auth.md)
>
> **Verification:** [Security abuse matrix](verification/security-abuse-matrix.md)

## Outcome

All daemon requests cross an explicit trust boundary. Remote access is never
anonymous merely because the workshop is Personal, local access is never
trusted merely because the peer address is loopback, and new routes cannot
silently inherit authority. Home still launches and chats without a manual
self-pairing ceremony.

This plan owns SEC-001 only. Identifier/path confinement is H02 and remote
webview capability isolation is H08; H01 must not claim their evidence.

## Baseline evidence

Before H01, the production composition had four interacting failure modes:

1. `src/portal_acl.rs` permits Personal-mode requests without a bearer after
   excluding only peer-token misuse.
2. `src/bin/medousa_daemon.rs` installs `peer_scope_middleware` only when a
   `PairingService` exists. `MEDOUSA_PAIRING_DISABLE` or pairing initialization
   failure can therefore remove protection from the full merged router.
3. `src/daemon/router.rs` applies `CorsLayer::permissive()` to the application
   router.
4. `src/remote_trust.rs` treats loopback plus absence of the internal Iroh
   marker as trusted local. This distinguishes Iroh from a direct socket, but
   does not authenticate the calling process or browser origin.

The baseline client picture drove the migration:

- Paired Home connections already load a session token and attach it to both
  LAN and Iroh requests in `workshop_transport.rs` and
  `medousa-sdk-iroh`.
- The default `personal` local workshop intentionally has no pairing record or
  session token.
- JSON, SSE, multipart, raw PUT, health checks, CLI, and TUI do not all pass
  through one credential-injecting implementation.
- Pairing session tokens are random UUIDs, stored hashed server-side, expire
  after 24 hours, and are checked for revocation. Lookup scans stored records;
  performance/storage redesign is allowed but not required to establish the
  boundary.
- Pairing initialization has a per-source rate limit, but public QR/code
  visibility, global pressure, body limits, and pairing-window activation are
  not yet one coherent boundary.

## Assets and attackers

### Protected assets

- vault, artifact, media, session, profile, and workspace contents;
- model/provider credentials and environment/configuration state;
- permission approvals and runtime/package/model controls;
- Forge/Coder, shell, process, browser, MCP, and maintenance capabilities;
- member identity bindings, pairing records, Iroh tickets, and session tokens;
- compute, memory, file descriptors, queues, and long-lived streams.

### Attacker classes

- a LAN or public-network caller reaching a non-loopback bind;
- arbitrary JavaScript served from an attacker origin reaching loopback;
- an unprivileged local process owned by the same or another OS user;
- a paired peer trying portal/admin paths;
- a portal member trying root/admin or another member's data;
- a revoked/expired client reusing HTTP connections or streams;
- a caller forging transport, forwarded, host, origin, or method metadata;
- an accidental route addition that lacks policy metadata.

OS-user compromise, kernel compromise, and memory extraction from the daemon
process are outside this plan. H01 still avoids making those attacks easier by
logging or persisting plaintext credentials unnecessarily.

## Security invariants

1. Personal mode is never an authentication bypass.
2. Pairing-service absence never yields a less-protected application router.
3. Source IP, loopback, CORS success, TLS, and Iroh transport are not caller
   credentials.
4. Every reachable route/method has reviewed policy metadata.
5. Authentication occurs once; handlers receive a typed principal.
6. Invalid or ambiguous credentials do not fall back to local or anonymous
   authority.
7. Anonymous bootstrap cannot read general state or allocate general work.
8. Peer credentials cannot acquire portal/admin authority through aliases,
   nesting, content types, or methods.
9. Revocation prevents new work and terminates long-lived authority within a
   declared bound.
10. The webview never receives the local daemon credential.
11. Denied requests enter no business handler and create no durable side effect.
12. Secrets never appear in URLs, normal logs, metrics labels, or error bodies.

## Non-goals

- SSO, accounts, org login, or internet identity federation;
- replacing the existing Ed25519 pairing ceremony in the first slice;
- per-member vault partitioning beyond existing Shared-mode ownership rules;
- treating CORS as a defense for native/non-browser clients;
- designing general reverse-proxy authentication;
- solving remote webview Tauri authority (H08) or filesystem confinement (H02).

## Target request flow

```text
socket / Iroh gateway
  -> normalize trusted transport metadata
  -> host and browser-origin policy
  -> body/concurrency/rate limits
  -> route policy lookup
  -> authenticate exactly one credential
  -> authorize principal capability + resource scope
  -> handler receives RequestPrincipal
  -> audit outcome without secrets
```

Bootstrap uses a physically separate router branch. It is not “protected
router with a boolean skip.” Authentication failures stop before identity,
store, model, filesystem, process, or stream allocation.

## Principal and capability contract

Introduce a request-scoped type with no raw secret:

```rust
struct RequestPrincipal {
    kind: PrincipalKind,
    credential_id: CredentialId,
    profile_id: Option<ProfileId>,
    capabilities: CapabilitySet,
    transport: TransportClass,
    revocation_generation: u64,
}
```

Suggested initial capabilities are deliberately broader than individual paths
but narrower than roles:

| Capability | Representative operations |
| --- | --- |
| `workshop.read` | scoped health, catalogs, sessions, workspace reads |
| `workshop.interact` | turns, cancellation, scoped interactive streams |
| `content.read` / `content.write` | vault, artifact, media within ownership policy |
| `workspace.write` | cards, jobs, recurring work within ownership policy |
| `peer.exchange` | peer/share/inbox/mesh contract only |
| `profile.self` | own/member-safe profile operations |
| `preview.read:{id}` | one short-lived preview resource |
| `admin.identity` | pairing, members, root/profile administration |
| `admin.runtime` | packages, models, providers, environment, maintenance |
| `admin.execute` | shell/process/Forge/Coder and approval administration |

Roles issue a default capability set, but handlers authorize capabilities and
resource ownership, not role names alone. Root is not an automatic fallback;
it is an authenticated principal with explicit admin capabilities.

`apply_bearer_identity` and similar header-parsing helpers are deleted after
handlers consume `RequestPrincipal.profile_id`. Duplicate `Authorization`
headers, multiple credential schemes, and cookie/header conflicts are rejected.

## Router ownership

### Route groups

| Group | Anonymous | Portal | Peer | Root/local app | Limits |
| --- | --- | --- | --- | --- | --- |
| Minimal liveness | Allow | Allow | Allow | Allow | tiny response; read rate |
| Pairing ceremony | Active window only | Ceremony-specific | Ceremony-specific | Allow | strict body, source/global rate, concurrency |
| Portal application | Deny | capability + owner | Deny | capability | operation-specific |
| Peer exchange | Deny | explicit policy | `peer.exchange` | capability | body/queue limits |
| Administration | Deny | explicit grant only | Deny | admin capability | strict audit/rate limits |
| Preview | token only | token policy | Deny | token policy | resource/TTL bound |

The minimal public health response should contain only protocol/liveness and a
coarse version compatibility field. Existing detailed health, pairing status,
device metadata, Iroh tickets, QR creation/rotation/images, and operational
counts move behind authentication.

### Policy declaration

Replace `is_public_path`, `is_admin_path`, and `path_allowed_for_peer` as the
authority source with route-group assembly that requires metadata:

```text
method, path template, group, capability, bootstrap flag,
browser policy, body limit, rate-limit class
```

The same definition feeds router construction and a generated inventory. Tests
compare the inventory from the fully assembled production router to the
reviewed contract. It must be impossible to merge a router into the protected
surface without selecting a policy group.

Keep policy helpers only as derived compatibility shims during migration.
Delete them once no production authorization path consumes raw strings.

## Local-app credential

### Provisioning

On first secure daemon startup:

1. Generate at least 256 bits of randomness.
2. Create a named `home-local` credential record with root/local-app
   capabilities and a stable opaque identifier.
3. Store the verifier/hash in daemon-owned state.
4. Put the secret in the platform credential store where available; otherwise
   use an owner-only file beneath the daemon data directory with atomic create,
   restrictive permissions, and no backup/export by default.
5. Have the Tauri native layer load and attach it to every local JSON, raw,
   multipart, health, and stream request.

Do not pass the secret through command-line arguments, URLs, webview state,
localStorage, events, logs, or broadly inherited environment variables.

CLI and TUI enroll as separately named local clients or use an explicit native
credential discovery command. Separate credentials make revocation and audit
useful and avoid turning one copied Home secret into a permanent master key.

### Local compatibility window

The one-release compatibility window is closed. The version gate, compatibility
telemetry, and synthetic legacy-local principal have been deleted. Protected
requests without a credential receive `401` on loopback and non-loopback
transports alike. There is no production `no_auth` flag.

## Pairing and remote credentials

Keep the existing signed challenge ceremony initially, with these boundary
changes:

- pairing endpoints are reachable anonymously only while an operator-created
  window is active and unexpired;
- QR generation/rotation is authenticated local/root work;
- `/pair/code` does not reveal the active short code to arbitrary callers;
- initiation and verification share per-source, global, concurrency, and body
  limits;
- challenges remain single-use and failures do not reveal device/token state;
- role and profile binding are fixed at issuance and represented in the
  resulting principal;
- token lookup is constant-time with respect to secret comparison and avoids a
  full record scan as the store evolves;
- expiry and rotation behavior is surfaced to clients as re-pair/renewal state,
  never anonymous fallback.

The existing 24-hour TTL is a baseline, not a permanent product decision. Any
refresh protocol must be authenticated by the current credential, rotate the
secret, and preserve revocation.

## CORS, origin, host, and request forgery

Remove global `CorsLayer::permissive()`.

Production native Home requires no browser CORS because Tauri performs daemon
requests in Rust. A supported browser/dev client must opt into exact origins in
configuration. For each origin declare:

- exact scheme, host, and port;
- allowed methods and headers;
- whether read-only or mutation is permitted; and
- environment/lifetime (for example, a development session only).

Rules:

- deny wildcard/reflected origins, `Origin: null`, and malformed origins;
- never use cookies for daemon authentication;
- require `Authorization` and the route credential even when origin is allowed;
- require JSON/custom content type on mutation routes and reject simple
  form/text bodies;
- validate `Host` against listener/configured names to resist DNS rebinding;
- apply policy before SSE/WebSocket or handler allocation;
- do not forward authorization across cross-origin redirects; and
- return CORS headers on permitted error responses without exposing secret
  details.

## Transport and proxy rules

- The Iroh gateway continues stripping caller-supplied
  `x-medousa-transport` and adding its internal marker.
- Direct socket requests cannot assert local-app authority with that or any
  forwarding header.
- Iroh requests always require a paired credential; an Iroh path that appears
  loopback inside the process receives no local bypass.
- `Forwarded`, `X-Forwarded-For`, and similar headers are ignored unless the
  immediate socket peer matches an explicit trusted-proxy configuration.
- Trusted proxy configuration affects client address/rate-limit attribution,
  not authentication requirements.
- A non-loopback bind fails startup if protected authentication, credential
  verification, route policy, or host policy cannot initialize.

## Revocation and long-lived work

Every new request resolves current credential state. SSE and future WebSocket
connections capture the credential ID and revocation generation, then either:

- subscribe to a credential-revocation broadcast; or
- revalidate at an interval no longer than 30 seconds and before accepting a
  mutating message/reconnect.

Revocation closes the stream with a generic authentication event/status and
cancels only work whose authority contract says the connection owns it. Turn
cancellation semantics remain H05/H03 work; H01 must ensure revoked clients
cannot start, attach to, or control new work.

## Resource limits

Before business handlers:

| Surface | Required bound |
| --- | --- |
| Liveness | constant response; per-source read rate |
| Pair init/verify | request bytes, JSON depth, pending sessions, global/source rate |
| Authentication | header count/bytes, bounded verification work and cache |
| JSON mutation | per-route body bytes and content type |
| Multipart/raw | per-route bytes, parts, filenames, streaming deadline |
| SSE/WebSocket | connections per credential/source, idle/maximum lifetime |
| Denial logging | sampled/bounded by reason and route group |

Exact production values are set from measurement and abuse tests, checked into
the route policy contract, and documented in canonical configuration docs when
shipped.

## Delivery slices

### H01.0 — Fail-safe remote exposure

- Make protected middleware unconditional in final router assembly.
- Refuse non-loopback startup when the credential/policy service is absent.
- Separate minimal bootstrap from the application router.
- Add black-box DA-001/DA-002 coverage for Personal and Shared modes.

This is the first code change because it closes the catastrophic configuration
path without waiting for the local credential migration.

### H01.1 — Principal and capability core

- Add typed `RequestPrincipal`, credential IDs, transport class, and capability
  set.
- Authenticate once and inject request extensions.
- Convert profile/identity binding to the principal.
- Standardize `401`/`403` responses and secret redaction.
- Add invalid/duplicate/expired/revoked credential tests.

### H01.2 — Policy-owned router composition

- Define route groups and required metadata.
- Split bootstrap, portal, peer, admin, and preview composition.
- Export the assembled route inventory.
- Migrate nested routers and remove string-prefix authorization.
- Add synthetic undeclared-route failure coverage.

### H01.3 — Local-app credential and first-party migration

- Provision and store the local credential.
- Inject it in all Home request paths, including SDK, raw, multipart, health,
  SSE, and route probes.
- Add CLI/TUI enrollment/discovery and credential injection.
- Ship bounded loopback compatibility telemetry.
- Remove raw loopback trust after the supported migration release.

### H01.4 — Pairing/bootstrap and browser boundary

- Require an active pairing window and protect QR/status/ticket operations.
- Add global/concurrency/body limits and non-enumerating responses.
- Replace permissive CORS with exact opt-in origins.
- Add host, content-type, preflight, origin, DNS-rebinding, and browser-driven
  WEB-001–WEB-010 tests.

### H01.5 — Revocation, rotation, and operations

- Add revocation generation/broadcast to long-lived connections.
- Implement local credential list/rotate/revoke diagnostics.
- Add bounded audit events and metrics.
- Exercise connection reuse, stream closure, rotation, and restart.

### H01.6 — Delete compatibility and ship docs

- Delete loopback bypass, handwritten route classifiers, duplicate header
  parsing, and global permissive CORS.
- Remove/deprecate unsafe exposure wording and legacy flags.
- Run the complete security abuse matrix on supported platforms.
- Update canonical engine, SDK, configuration, LAN/pairing, Home, CLI, and TUI
  documentation.

## Compatibility and rollout

| Stage | Loopback Personal | Paired LAN/Iroh | Non-loopback without auth |
| --- | --- | --- | --- |
| Safety patch | legacy local allowed with warning | bearer required | startup refusal/full API unavailable |
| Client migration | local capability preferred; legacy measured | bearer required | refused |
| **Enforcement (current)** | **local capability required** | **bearer required** | **refused** |

Rollout state is persisted locally and observable; it is not chosen by an
untrusted request. A failed upgrade may roll back to the previous binary and
credential record backup, but rollback must not restore anonymous non-loopback
access. Credential schema migration is atomic and retains the last readable
version until the new version has started successfully.

## Observability and operator diagnostics

Emit bounded structured events and counters for:

- startup listener class and enforcement mode;
- authentication result class, principal kind, route group, and transport;
- authorization denial capability/reason class;
- pairing-window open/close, saturation, issuance, rotation, and revocation;
- credentialless denial class by declared route group;
- active streams closed by expiry/revocation; and
- route inventory hash/version.

Never label metrics or logs with raw tokens, authorization headers, request
bodies, arbitrary paths, profile names, phone IDs, or full source addresses.
The diagnostic command reports whether auth is enforced, credential-store
health, inventory version, and safe remediation—not credential contents.

## Verification and exit criteria

H01 may move to **Validated** only when:

- DA-001 through DA-012 and WEB-001 through WEB-010 pass against production
  router composition;
- tests cover loopback and a real non-loopback socket in Personal and Shared
  modes;
- paired LAN and Iroh positive paths pass for portal and peer credentials;
- all first-party Home JSON/raw/multipart/SSE paths pass with the local or
  paired credential;
- CLI and TUI supported workflows pass with named local credentials;
- the generated route inventory contains every method/path and no undeclared
  policy;
- revoked credentials cannot start new work and live authority closes within
  the declared bound;
- attacker-origin browser tests prove no readable response or mutation;
- normal and error logs pass secret scanning; and
- the supported-platform CI and documentation checks are green.

H01 becomes **Shipped**, and SEC-001 closes, only after compatibility removal,
release packaging, rollback verification, and canonical documentation land.

## Canonical documents changed at ship time

- `docs/engine/http-api.md` and relevant `docs/engine/` route guides;
- `docs/sdk/` authentication and transport guidance;
- `docs/configuration-reference.md` listener, trusted proxy, and origin config;
- LAN/Iroh pairing and getting-started guides under `docs/guides/` and
  `docs/cookbook/`;
- Home, CLI, and TUI application docs; and
- `scripts/verify-docs.sh` contract/link checks where generated auth metadata is
  exposed.

The implementation and canonical documentation are current. H01 remains short
of **Shipped** until supported-platform packaging, rollback, and full external
security-matrix evidence are recorded.

## Removal ledger

Removed during H01 implementation:

- Personal-mode anonymous allowance in `src/portal_acl.rs`;
- `is_public_path`, `is_admin_path`, and `path_allowed_for_peer` as production
  authority sources;
- request-handler parsing of `Authorization` for profile identity;
- loopback-IP authorization in `src/remote_trust.rs` and pairing revocation;
- conditional installation of protected auth middleware;
- global `CorsLayer::permissive()`;
- any legacy-local compatibility state and its warning;
- obsolete unsafe `--public`/pairing documentation and tests; and
- duplicate credential injection paths superseded by the shared transport.
