# ADR-018: Untrusted webview isolation and minimal browser bridge

> **Status:** Proposed
>
> **Date:** 2026-08-13
>
> **Decision owners:** Medousa desktop and browser maintainers
>
> **Related:** [ADR-013](adr-013-daemon-trust-zones-and-auth.md), [ADR-014](adr-014-identifier-and-filesystem-authority.md), [ADR-017](adr-017-request-scoped-runtime-context.md), [H08 execution plan](../../../architecture/hardening/08-desktop-browser-isolation.md)

## Context

Medousa's desktop Web surface deliberately renders arbitrary `http` and
`https` pages in child webviews. Those pages are attacker-controlled even when
the user chose the URL. They can execute JavaScript, redirect, embed frames,
open popups, and attempt any native IPC object exposed to their JavaScript
realm.

The current `browser-tab-webviews` capability grants matching remote webviews
`core:default`. Tauri expands that default into broad core event, window,
webview, application, resource, image, path, menu, and tray permissions. Some
legacy browser labels also appear in the trusted desktop capability. Tauri
combines permissions when a window/webview matches multiple capabilities, so
label overlap can erase a narrower boundary.

Browser scripts call custom `#[tauri::command]` handlers through
`window.__TAURI_INTERNALS__.invoke` for navigation/title/favicon/new-window/
hotkey reports and for snapshot/action/find replies. The desktop crate has one
large application command handler. The build script defines no application
command manifest for a deliberately remote bridge.

This behavior is version-sensitive. The locked Tauri 2.11.2 contains the 2.11.1
security change that always ACL-checks application commands from remote
origins. The existing custom callbacks should therefore be denied while the
broad core permissions remain. Older affected versions could bypass custom
command ACL without an application manifest. A timeout is not a security model,
and granting the whole application command surface to fix it would be critical.

The trusted shell has independent weaknesses: CSP is disabled and the asset
protocol scope covers broad home/document/download/desktop/tmp paths. A shell
injection should not become an arbitrary local-file reader.

## Decision

### 1. Browser content is a separate hostile principal

Trusted Medousa chrome and untrusted browser content use disjoint webview labels,
capabilities, storage identities, and code paths. Remote content receives no
Tauri core, event, window, webview, menu, tray, path, resource, image, opener,
dialog, notification, filesystem, shell, or general application permission.

No untrusted content label may appear in a trusted window/webview capability.
Generated capability resolution is tested for each concrete and wildcard
label. `about:blank`, redirects, error pages, popups, and special URL schemes do
not transition a content webview into a trusted class.

Remote content may receive exactly one dedicated report-only bridge permission
when platform-native hooks cannot implement the feature. This permission is
owned by a small browser-bridge plugin/module whose manifest exposes one typed
report command; it is not an allowlist over the giant application handler.

### 2. Native observation precedes page IPC

Use webview-native hooks for navigation start/finish, current URL,
document-title changes, new-window decisions, close, and download decisions.
These facts do not require a hostile page to report them.

Popups are denied by default. An allowed `http`/`https` popup request becomes a
trusted-shell request to create/navigate another isolated browser tab; the
remote page never creates a privileged window. Top-level navigation permits
only normalized product-approved schemes. `file`, Tauri/custom protocols,
`javascript`, `data`, and other authority-bearing schemes are denied or opened
through a separate explicit trusted workflow.

If reliable native hooks cannot capture a feature such as SPA history,
favicon, or browser hotkeys, the feature is either an untrusted UI hint through
the minimal bridge or is disabled on that platform. It is never grounds for
granting `core:default`. Page-reported URL is compared with the native current
URL/navigation generation and cannot authorize navigation, origin, filesystem,
or shell action.

### 3. The bridge can report but cannot command

The remote bridge accepts a closed tagged response enum. It can complete an
already registered snapshot/action/find/navigation query or submit a bounded
non-authoritative metadata hint. It cannot initiate filesystem/daemon/package/
terminal/window operations, emit/listen on the application event bus, choose a
target surface, create a request, or select a native action.

The command receives Tauri's actual invoking `Webview` as a command argument.
Rust derives the concrete label/instance and native current URL, looks up the
H05 `BrowserSurfaceId`/navigation generation, validates message kind and size,
and completes only the exact outstanding request. Payload-supplied surface,
origin, or URL is never trusted.

Request IDs and injected nonces provide correlation and replay resistance, not
authority: hostile page JavaScript shares the realm and may observe or forge
them. The security boundary is the report-only native API, actual invoking
webview binding, outstanding trusted request, strict state machine, and absence
of authority-bearing effects. Snapshot/DOM/action results are explicitly
untrusted page observations.

### 4. Page execution is bounded and generation-scoped

Trusted shell/daemon code initiates snapshot, find, or DOM action against an
exact surface instance, native origin, and navigation generation. The H05
broker registers the request before evaluating a constant script template with
serialized data. Navigation, close, timeout, cancellation, origin change, or
surface recreation invalidates it.

Messages have variant-specific byte/depth/count limits before expensive
deserialization/allocation. Snapshot capture is bounded in the page and again
at native IPC. There is no unlimited `outerHTML` crossing IPC. Unknown fields,
wrong variants, duplicates, unsolicited reports, and stale generations are
rejected at a rate-limited diagnostic boundary.

### 5. Browser-control authority stays outside the page

User/agent control lease, high-risk approval, request admission, and action
selection live in trusted Medousa state. The page only receives a specific
already-approved DOM operation. Password/credential entry, payments, destructive
submission, permission prompts, downloads, clipboard, camera, microphone,
geolocation, and external-protocol launch are denied by default or require a
trusted, origin- and generation-bound user approval.

Page selectors/text and DOM success reports are untrusted. A page can lie or
change between observation and action, so sensitive approvals bind the native
origin/navigation generation and an action summary, expire quickly, and are
invalidated by navigation or user takeover.

### 6. Trusted shell receives defense in depth

The trusted app frontend uses an explicit production CSP with least-permissive
script, connection, image, style, frame, worker, and object/base/form policies
compatible with measured features. Production does not permit arbitrary remote
scripts or `unsafe-eval`. Development relaxations are separate and cannot ship.

Remove the home-wide asset protocol scope. Co-located local previews use an
H02-authorized resource service/protocol keyed by short-lived root/object
handles, MIME/size policy, and exact paths selected by trusted UI. Remote
workshops fetch daemon-owned content through authenticated daemon APIs. A raw
absolute path or `convertFileSrc` is never a cross-workshop capability.

## Consequences

### Positive

- A visited website cannot reach Medousa's native command/event/window/file
  surface.
- Browser features work through a reviewable report-only contract rather than
  Tauri defaults or version accidents.
- Permission merging, dependency upgrades, and packaged behavior are tested.
- Compromising trusted frontend content has a narrower script/file boundary.
- Agent browser control remains possible without treating a page as trusted.

### Costs

- Browser hooks/bridge behavior require platform-specific packaged tests.
- Some hotkey/favicon/SPA metadata behavior may be reduced until a safe native
  or report-only implementation exists.
- Snapshot and action scripts need bounded encoders and explicit failure UX.
- Local asset previews migrate from broad asset paths to authorized handles.
- Tauri upgrades become security-sensitive compatibility work.

### Relationship to existing browser decisions

`agent-browser-host.md` and `shared-browser-workspace.md` retain the human-first
shared browser, cookie/session reattachment, client capability advertisement,
and control-handoff product model. ADR-018 supersedes any implementation
assumption that arbitrary page JavaScript may use general Tauri IPC or that a
surface name supplied by the page proves identity.

ADR-017/H05 owns request correlation, surface generations, cancellation, and
pending-request lifecycle. ADR-018 owns which native capabilities the remote
webview can reach. ADR-013/H01 owns authentication/CORS for loopback daemon and
BrowserHost endpoints; a remote page's ability to send network requests must
not grant control-plane authority.

## Verification

The packaged application must pass TV-001–TV-012 and BR-001–BR-007 in the
[security abuse matrix](../../../architecture/hardening/verification/security-abuse-matrix.md)
on supported desktop platforms. Generated ACL/capability inventories and locked
Tauri/Wry/system-webview versions are retained as evidence.

## Code anchors

- `apps/medousa-home/src-tauri/capabilities/browser-tab-webviews.json`
- `apps/medousa-home/src-tauri/capabilities/default.json`
- `apps/medousa-home/src-tauri/tauri.conf.json`
- `apps/medousa-home/src-tauri/build.rs`
- `apps/medousa-home/src-tauri/src/lib.rs`
- `apps/medousa-home/src-tauri/src/human_browser.rs`
- mobile browser bridge modules and trusted local resource helpers
