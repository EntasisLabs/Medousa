# Security abuse verification matrix

> **Status:** Draft baseline contract
> **Program:** [Medousa hardening](../README.md)
> **Primary findings:** SEC-001, SEC-002, SEC-003, DESKTOP-001
> **Required decisions:** ADR-013, ADR-014, ADR-018 (planned)

This document defines the evidence required to close Gate A. It describes
tests that must exist and pass; it does not claim the current implementation
passes them. Current known failures are recorded explicitly.

## Verification principles

1. **Default deny is observed at the real boundary.** Unit tests of a policy
   helper are supporting evidence, not proof that the assembled router or
   packaged app enforces the policy.
2. **Enumerate authority automatically.** Route and Tauri command inventories
   come from the assembled application. A handwritten allowlist cannot prove
   that a newly added endpoint is protected.
3. **Test hostile syntax and hostile state.** Path checks include encoding,
   aliases, symlinks/reparse points, replacement races, and destructive calls.
4. **Use an attacker-controlled origin.** Browser/CORS and remote-webview tests
   serve content from a distinct origin whose scripts attempt prohibited work.
5. **Prove positive and negative behavior.** A secure system that rejects the
   legitimate paired client is not validated.
6. **Retain evidence.** CI stores the discovered inventories, request/response
   transcript, app logs, platform identity, and test binary revision.

## Test topology

The suite provisions disposable roots and never points destructive cases at a
developer's real data directory.

```text
isolated test process
├── temporary MEDOUSA_DATA_DIR
├── temporary vault root
│   ├── ordinary notes
│   ├── links/junctions into a separate canary root
│   └── deletion sentinels
├── assembled daemon on loopback
├── assembled daemon on a non-loopback test interface
├── trusted client origin
├── attacker HTTP origin
└── packaged Tauri app + attacker pages
```

The non-loopback test must use a real socket and the production router stack.
Calling handlers directly misses bind, proxy, origin, and middleware mistakes.

## Actors and credentials

| Actor | Credential | Intended authority |
| --- | --- | --- |
| `anonymous` | No authorization header/cookie | Bootstrap endpoints explicitly designated public; health only if ADR-013 permits it |
| `invalid` | Malformed, expired, revoked, or unknown token | None beyond the anonymous bootstrap surface |
| `portal` | Valid portal credential bound to one workshop/member/device | Explicit portal route capabilities only |
| `peer` | Valid peer credential | Peer/share/heartbeat surface only |
| `root` | Valid administrative credential | Administrative surface allowed by route policy |
| `local_app` | Verified co-located app transport, if retained by ADR-013 | Explicit local-app capability; loopback IP alone is insufficient |
| `trusted_origin` | Approved app/development origin plus valid credential | Same API authority as its credential |
| `attacker_origin` | Arbitrary HTTP/HTTPS origin | No browser-readable or mutating API access |
| `remote_webview` | Arbitrary visited page inside the Web surface | No ambient Tauri core/plugin/application IPC |

Shared and Personal modes are separate matrix dimensions. “Personal” changes
tenancy/product behavior; it must not mean anonymous administrative access.

## Route inventory contract

The test harness emits one row for every assembled route/method combination:

```text
method, normalized_path_template, route_group, required_capability,
bootstrap_public, csrf_policy, body_limit, rate_limit_class
```

The suite fails if:

- a route has no declared route group or capability policy;
- an unknown route inherits a permissive fallback;
- the discovered inventory differs from the reviewed generated contract;
- `OPTIONS` or alternate methods bypass authentication;
- a nested Forge, vault, package, model, browser, session, workspace, pairing,
  runtime, approval, or maintenance router loses the parent policy; or
- a public bootstrap route can reach general application state by parameters,
  redirects, proxying, or content negotiation.

## Daemon authentication matrix

Every route group is exercised with each actor in both loopback and
non-loopback configurations. `allow` below means “subject to the narrower
capability and ownership policy,” never unconditional success.

| Route group | Anonymous | Invalid/revoked | Portal | Peer | Root/local app |
| --- | --- | --- | --- | --- | --- |
| Health/version bootstrap | ADR-013 decision | Same or stricter | Allow | Allow | Allow |
| Pairing bootstrap | Only rate-limited ceremony endpoints | Deny | As ceremony requires | As ceremony requires | Allow |
| Interactive turns/sessions | Deny | Deny | Scoped allow | Deny | Allow |
| Vault/artifact/media | Deny | Deny | Scoped allow | Explicit share endpoints only | Allow |
| Forge/Coder/shell/process | Deny | Deny | Explicit governed capability only | Deny | Allow |
| Packages/models/runtime config | Deny | Deny | Deny unless separately granted | Deny | Allow |
| Workspace/jobs/recurring | Deny | Deny | Scoped allow | Deny | Allow |
| Browser control/snapshot | Deny | Deny | Explicit surface capability only | Deny | Allow |
| Peer/share/inbox | Deny | Deny | Bound workshop policy | Scoped allow | Allow |
| Identity/profile/admin/pairing revoke | Deny | Deny | Own/member-safe subset | Deny | Allow |
| Preview reverse proxy | Valid short-lived preview token only | Deny | Token policy | Deny | Token policy |
| Unknown/new route | Deny | Deny | Deny | Deny | Deny until declared |

### Required cases

| ID | Case | Expected evidence |
| --- | --- | --- |
| DA-001 | Start Personal mode on non-loopback; call every non-bootstrap route anonymously | Every call denied before handler side effects; current code is expected to fail this case |
| DA-002 | Repeat DA-001 in Shared mode | Same default-deny behavior |
| DA-003 | Present malformed, wrong-signature, expired, and revoked credentials | Uniform denial; no identity enrichment fallback |
| DA-004 | Use peer token against every non-peer route | Denial with no mutation, file access, subprocess, or model work |
| DA-005 | Use portal token across administrative/runtime/package routes | Only explicitly granted capabilities succeed |
| DA-006 | Revoke a live credential, reuse existing HTTP/SSE connections, and reconnect | New work denied at the documented revocation boundary |
| DA-007 | Rotate signing/session material during active clients | Old/new credential behavior matches migration decision; no anonymous grace path |
| DA-008 | Send missing/duplicate authorization headers and conflicting cookie/header identity | Reject ambiguous identity; never select the more privileged value |
| DA-009 | Exercise method override, `HEAD`, `OPTIONS`, alternate content types, trailing slashes, repeated separators, and encoded path forms | No policy bypass or route alias with weaker protection |
| DA-010 | Route through Iroh/proxy transport with forged loopback-looking metadata | Transport identity, not peer IP, determines trust |
| DA-011 | Saturate pairing/bootstrap attempts from one and many identities | Rate limit and bounded resource behavior; existing sessions remain responsive |
| DA-012 | Add a synthetic undeclared route in a test build | Inventory/policy build or test fails closed |

## Browser origin, CORS, and request-forgery matrix

The attacker origin runs real browser JavaScript against loopback and
non-loopback daemon URLs. Raw `curl` headers alone do not prove browser behavior.

| ID | Attack | Expected result |
| --- | --- | --- |
| WEB-001 | Cross-origin simple `GET` to readable state | No readable response without an explicitly approved origin and credential |
| WEB-002 | Preflighted mutating request with JSON/custom headers | Preflight denied for attacker origin; mutation never executes |
| WEB-003 | “Simple” form/text request designed to avoid preflight | CSRF/origin/capability check denies mutation |
| WEB-004 | `Origin: null`, local file, sandboxed iframe, extension-like, and malformed origins | Denied unless explicitly and narrowly supported |
| WEB-005 | Approved origin with no/invalid credential | Denied; CORS approval never substitutes for authentication |
| WEB-006 | Valid credential from attacker origin | Origin/CSRF policy behaves as ADR-013 specifies; credentials are not silently exposed |
| WEB-007 | DNS rebinding/host-header variation and private-network request | Host/origin policy rejects unauthorized transition |
| WEB-008 | Redirect from approved to attacker origin | Authorization material is not forwarded or exposed |
| WEB-009 | SSE/WebSocket connect and reconnect from attacker origin | Denied before stream/work allocation; no event leakage |
| WEB-010 | Error responses and preflights | No secret-bearing headers/body; response timing does not enumerate credentials materially |

The evidence bundle includes browser console/network logs and a daemon
side-effect ledger proving denied requests did not enter handlers.

## Identifier and filesystem authority matrix

Test every externally supplied identifier that contributes to a path, with
special coverage for session, artifact, media, verification, extraction,
ledger, vault, trash, overlay, Forge work/attempt, and export identifiers.

### Input corpus

| Class | Examples/requirement |
| --- | --- |
| Empty/alias | empty, whitespace, `.`, repeated separators, trailing separator |
| Parent traversal | `..`, `../x`, nested and percent/double-percent encoded forms |
| Absolute paths | POSIX root, Windows drive, UNC, verbatim/device paths |
| Separators | `/`, `\\`, mixed separators, Unicode slash lookalikes |
| Platform aliases | Windows trailing dot/space, reserved device names, alternate data streams, case aliases |
| Control/Unicode | NUL where representable, control characters, normalization variants, bidi marks |
| Resource abuse | overlong IDs, many segments, collision-producing encodings |
| Link state | symlink, hard link where relevant, junction/reparse point, mount/bind point |
| Race state | ancestor or leaf replaced between validation and open/rename/delete |

### Operations

| ID | Operation | Required result |
| --- | --- | --- |
| FS-001 | Create/write/append | Invalid IDs rejected before filesystem effect; valid operation stays beneath its authority root |
| FS-002 | Read/list/search/index | No read or metadata leak outside the authority root |
| FS-003 | Update with precondition | Target identity cannot change between validation and commit |
| FS-004 | Rename/move/trash/restore | Both source and destination confined using no-follow/handle-relative semantics |
| FS-005 | File deletion | Only the exact validated object is removed |
| FS-006 | Recursive deletion | Target must be a typed store-owned directory handle; hostile ID cannot broaden it |
| FS-007 | Startup migration | Invalid legacy names are quarantined/reported without reinterpretation collisions |
| FS-008 | Cleanup after partial failure | Cleanup cannot follow attacker-controlled links or delete a replacement target |

Run FS-001–008 on Linux, macOS, and Windows. Unix symlink tests are not a
substitute for Windows junction/reparse and path-alias tests.

Each destructive test creates canaries immediately inside and outside the
allowed root. Success requires the intended object state and byte-identical
outside canaries after a fresh process inspects them.

## Tauri remote-webview matrix

Tests use the packaged application and the locked Tauri version. A dev server
or Rust unit test cannot prove generated ACL/capability behavior.

### Inventories

The evidence bundle records:

- generated capability files and ACL manifests;
- every registered application command;
- every core/plugin permission granted to each window/webview label;
- actual webview label and current origin; and
- locked Tauri/Wry/WebKit/WebView2 versions.

### Required cases

| ID | Case | Required result |
| --- | --- | --- |
| TV-001 | Attacker page invokes every discovered application command | All denied except an explicitly generated minimal bridge allowlist |
| TV-002 | Attacker page invokes every core/plugin command in generated ACL inventory | All denied; remote browser page has no `core:default` |
| TV-003 | Page listens/emits to shell event names | No observation, injection, or cross-window control |
| TV-004 | Page attempts window/webview/menu/tray/resource/image/path operations | Denied before native side effect |
| TV-005 | Legitimate location/title/favicon/new-window/hotkey bridge messages | Work only from the bound browser webview and permitted origin/context |
| TV-006 | Snapshot/find/action requests run concurrently across embed and pop-out surfaces | Responses correlate to request ID, webview, and origin; no global-slot cross-talk |
| TV-007 | Page forges a bridge response without an outstanding request or with wrong nonce/ID | Ignored and logged at a bounded rate |
| TV-008 | Page navigates between request and response | Stale response rejected or tied to the documented navigation generation |
| TV-009 | Oversized/deep snapshot and malformed payload | Rejected by byte/depth/schema limits without app memory spike |
| TV-010 | `http`, `https`, redirect, iframe, popup, `about:blank`, data/blob, and error pages | Capability remains least-authority across all navigation states |
| TV-011 | Trusted main shell injection fixture | CSP and narrow asset scope prevent or contain the fixture; no arbitrary home-file read |
| TV-012 | Upgrade locked Tauri patch/minor version in a test branch | Matrix detects permission-model regression before release |

TV-001/002 must deliberately attempt sensitive representatives such as daemon
configuration, filesystem/open-path, packages, terminal, credentials, vault,
browser control, and window management—not merely a harmless command.

## Secret and log handling

All cases additionally assert:

- tokens, pairing secrets, OAuth material, filesystem contents, and request
  bodies do not appear in normal/error logs;
- denial messages contain a stable reason class without exposing whether a
  secret exists;
- metrics label route group/reason, not raw identity or path; and
- retained test artifacts use synthetic credentials and disposable data only.

## Harness deliverables

Planned implementation artifacts:

| Deliverable | Responsibility |
| --- | --- |
| Assembled-router inventory exporter | Enumerate route/method/policy metadata from production composition |
| Black-box daemon abuse runner | DA/WEB cases against loopback and non-loopback processes |
| Filesystem corpus crate/module | Shared cross-platform hostile path/link/race fixtures |
| Packaged Tauri attacker fixture | Serve malicious pages and record IPC allow/deny behavior |
| Side-effect sentinel | Record handler entry, subprocess/tool work, and filesystem mutation in test builds |
| Evidence packager | Emit one manifest with revision, platform, versions, inventories, results, and artifact hashes |

Names and implementation language are decided in H01/H02/H08. This document
owns behavior and evidence, not directory naming.

## Evidence manifest

Every run emits machine-readable metadata containing at least:

```text
schema_version
git_revision, dirty_state
build_profile, feature_set
os, kernel, architecture
tauri, wry, system_webview versions
daemon bind and mode
route/command/capability inventory hashes
test fixture and credential generation IDs
case results with duration and denial reason
stdout/stderr/network/app-log artifact hashes
outside-canary verification
```

No real credential or home path is included.

## Exit criteria

Gate A is validated only when:

- every discovered daemon route has an explicit reviewed policy and the full
  DA/WEB matrix passes in Personal and Shared modes;
- anonymous, invalid, peer, portal, root, and co-located identities have the
  same outcome on every supported transport unless an ADR documents otherwise;
- FS-001–008 pass on Linux, macOS, and Windows, including replacement races;
- TV-001–012 pass in packaged apps on macOS, Windows, and Linux;
- legitimate pairing, portal, peer, Iroh, browser, and local-app flows pass;
- denials produce no handler side effect or secret leakage; and
- the inventory/evidence bundle is retained by required CI or release gating.

Any skipped platform or route group keeps the associated finding open. A
manual security review may supplement this matrix but cannot replace its
repeatable cases.
