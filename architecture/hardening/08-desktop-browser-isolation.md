# H08 — Desktop browser isolation and minimal native bridge

> **Status:** Implemented; packaged-platform release validation pending
>
> **Accountable owner:** Medousa desktop maintainers
>
> **Reviewers:** browser runtime, Tauri/platform, application security, Home shell, agent tools, release engineering
>
> **Audit finding:** DESKTOP-001 (Critical)
>
> **Release gate:** Gate A — contain authority
>
> **Required decision:** [ADR-018](../../docs/architecture/decisions/adr-018-untrusted-webview-isolation.md)
>
> **Dependencies:** H01 authenticated loopback services; H02 resource authority; H05 correlated browser broker
>
> **Verification:** [Security abuse matrix TV-001–TV-012](verification/security-abuse-matrix.md), [browser correlation matrix](verification/crash-concurrency-matrix.md)

## Outcome

Every arbitrary website rendered by Medousa has zero ambient shell authority
and, where necessary, one report-only typed bridge. Trusted browser chrome stays
in a distinct app-origin webview. Native hooks own navigation/title/popup/
download lifecycle. Snapshot/find/action replies are bounded, correlated to the
actual invoking webview and native navigation generation, and incapable of
initiating a privileged side effect.

The packaged app proves this boundary against the complete generated Tauri ACL
and application command inventory on macOS, Windows, and Linux. The trusted
shell also ships with CSP and an authorized local-resource path instead of
home-wide asset access.

## Baseline authority graph (removed)

```text
attacker-controlled http(s) page
  -> browser-tab-webviews capability
     -> core:default
        -> event/window/webview/app/resource/image/path/menu/tray defaults
  -> window.__TAURI_INTERNALS__.invoke(custom command name)
     -> giant application invoke handler

trusted main capability
  -> broad core/plugins/opener paths
  -> legacy browser labels may overlap narrower browser capability

trusted shell injection
  -> CSP disabled
  -> asset protocol scoped across broad home/document/download/desktop/tmp
```

At the baseline, Tauri 2.11.2 ACL-checked remote application commands, so the
custom browser reports were undeclared and timed out while `core:default`
remained exposed. That was neither safe compatibility nor working
functionality. The generated handler count remains inventory input, not a
permission boundary.

## Implementation status

H08.0–H08.5 are implemented on the desktop code path. Remote content labels are
disjoint from trusted shell webviews and receive only the generated
`browser-bridge:allow-report` permission. Native hooks own lifecycle; the typed
report bridge and bounded request broker bind the actual webview and navigation
generation; snapshot/action work is bounded and revoked on control/lifecycle
changes; trusted-shell CSP and opaque vault-resource delivery replaced disabled
CSP and the broad asset protocol.

The reviewed command/ACL/CSP/dependency inventory runs on Linux, macOS, and
Windows CI and immediately before every desktop package build. Canonical app,
user, and operator docs are linked from `docs/README.md`. Final audit closure
still requires retained packaged-system-webview evidence from that release
matrix; source/unit validation alone is not marked `Validated` or `Shipped`.

## Threat model

Treat all of the following as attacker-controlled:

- top-level page, iframe, service worker, redirect target, popup URL, favicon,
  title, URL fragments/history state, DOM, and downloaded bytes;
- any JavaScript injected into the page realm, including its arguments,
  globals, request IDs, and nonces;
- callback timing, duplication, omission, size, nesting, and claimed surface/
  URL/origin; and
- a page sending direct loopback network requests to daemon/BrowserHost ports.

Assume compromise of one remote page must not affect trusted chrome, another
tab/surface, native files/secrets/processes, daemon authority, or a turn other
than through the exact trusted request already admitted for that page.

Also test a trusted-shell injection fixture. CSP/resource scope is defense in
depth, not a reason to relax remote-webview isolation.

## Security invariants

1. Remote content labels match no trusted capability, even through wildcards or
   window-child permission inheritance/merging.
2. A remote content webview has no core or general plugin/application permission.
3. The only optional remote permission is one dedicated browser-report command.
4. The report command's handler cannot perform or request an authority-bearing
   native operation.
5. Native code derives webview label/instance/current URL; payload identity is
   untrusted data.
6. A response completes only an outstanding trusted request with matching
   surface, kind, origin, and navigation generation.
7. Request ID/nonce is not treated as a secret or authorization proof.
8. Native hooks own navigation, title, new-window, close, and download policy
   wherever the platform supports them.
9. Untrusted events never enter the general Tauri application event bus.
10. Payloads are rejected before unbounded allocation, DOM serialization, or
    expensive recursive deserialization.
11. Navigation/surface destruction revokes every pending request and approval.
12. Agent action authority is held by trusted control/approval state; page
    reports cannot grant or extend it.
13. Browser storage/cookies may be shared among intended browser tabs but never
    with trusted app-origin storage or IPC authority.
14. Trusted shell CSP and resource delivery do not expose arbitrary user paths.
15. Dependency/capability changes fail closed in packaged security tests.

## Non-goals

- sanitizing or making arbitrary websites trustworthy;
- preventing a page from lying about its DOM or action result;
- using a nonce hidden in page JavaScript as a security boundary;
- granting remote origins all application commands and validating inside each;
- preserving every current browser metadata/hotkey convenience at any cost;
- relying on dev-server/unit behavior to prove packaged ACL enforcement;
- using CSP as compensation for native over-privilege; or
- solving daemon/BrowserHost network authentication owned by H01.

## Webview classes and labels

Define typed constructors with fixed capability class:

| Class | Content | Labels | Permissions |
| --- | --- | --- | --- |
| Trusted shell | bundled app routes/chrome | explicit `shell-*` / `browser-chrome-*` | reviewed shell-only capabilities |
| Remote browser | arbitrary web content | generated `browser-content-{instance}` | report-only bridge or empty |
| Local resource preview | trusted renderer over authorized bytes | explicit `resource-preview-*` | narrow renderer capability, never remote navigation |

Delete legacy labels such as `main-browser-content`/`browser-content` where they
can ambiguously match windows, webviews, or multiple capabilities. Labels are
generated by native code from an opaque instance ID; page/tab input never
selects a label. The surface registry records class, parent trusted window,
instance generation, active native URL/origin, navigation generation, storage
profile, and close state.

Capability files list trusted `windows` and trusted/remote `webviews`
deliberately. CI expands every wildcard against constructed labels and fails on
multiple capability membership for remote content. Review the actual generated
ACL rather than only source JSON because Tauri merges matching permissions.

Remote webviews use a browser-specific data directory/store identifier. Embed
and pop-out tabs may share that browser profile to preserve the product's human
session model. Trusted shell webviews do not use it. Incognito/private profile
is a separate explicit mode with bounded deletion diagnostics.

## Navigation and browser lifecycle

`RemoteBrowserBuilder` installs native callbacks before the first navigation:

- `on_navigation`: normalize and allow approved `http`/`https`; handle initial
  `about:blank` as an untrusted transitional state; deny authority-bearing and
  unsupported top-level schemes;
- `on_page_load`: advance native navigation state, emit trusted shell state,
  invalidate previous-generation requests, and schedule bounded optional probes;
- `on_document_title_changed`: update title as untrusted display text with size
  and control-character limits;
- `on_new_window`: deny native creation, validate URL, and ask trusted chrome to
  open an isolated tab if policy allows;
- `on_download`: deny by default or route through a trusted confirmation and
  bounded download service; and
- close/recreate: cancel H05 broker entries, revoke approvals, unregister exact
  instance, and clear browser-owned references.

Do not navigate a remote content webview to bundled app/custom-protocol chrome.
Do not navigate a trusted chrome webview to arbitrary remote content. Redirects
pass through the same scheme/origin and generation policy.

Iframe, blob, data, error-page, and inherited `about:blank` cases are exercised
in packaged tests. They do not gain a different capability because capability
class is bound to webview instance, not the current page's claimed origin.

## Minimal bridge

### Packaging and ACL

Prefer a dedicated in-tree Tauri plugin/module named for the browser bridge.
Its generated manifest exposes only `allow-report`. The remote capability is:

```text
webviews = browser-content-*
remote.urls = explicitly supported http/https patterns
permissions = browser-bridge:allow-report
```

It contains no `core:default`, event, plugin default, opener, dialog, path,
window, or application-handler permission. If an application command is used
instead, `tauri_build::AppManifest` must inventory commands and generate an
individual permission for the single report command; remote capabilities still
receive only that permission. The complete trusted command manifest is
reviewed separately. Never create a remote “default commands” permission.

The bridge JavaScript exposes a closure-local `report(message)` function only
to the constant injected request/probe scripts when possible. Hiding it is
defense in depth: the page can observe/replace realm objects and invoke the
allowed report command directly, which must remain harmless.

### Message contract

Use one closed enum and reject unknown versions/kinds:

```text
BrowserPageReportV1 =
  RequestResult {
    request_id, navigation_generation,
    result: Snapshot | Find | Action | NavQuery
  }
  | MetadataHint {
    navigation_generation,
    hint: SpaLocation | FaviconCandidate
  }
```

Hotkey reporting is not included unless a platform lacks native/window
accelerators and security review accepts a strictly enumerated browser-chrome
request. A page-generated hotkey has no trustworthy user-gesture proof. It may
at most request a browser-local UI action subject to focus, rate, surface, and
native-current-tab validation; it cannot open paths, invoke arbitrary commands,
or grant agent control. Prefer disabling the shortcut to weakening the bridge.

The Rust handler accepts `tauri::Webview` as an injected argument and obtains:

- actual label and registered `BrowserSurfaceId`/instance generation;
- native current URL/origin and navigation generation;
- expected pending request kind/deadline/size from H05; and
- trusted control/approval state where relevant.

Payload `surface`, `url`, `origin`, `tab_id`, and native command/action names are
not accepted as routing authority. Metadata hints update only the invoking
surface after native URL comparison. They are display hints, never security or
navigation authority.

### Bounds

Enforce both page-side truncation and native pre-deserialization/request limits:

| Variant | Initial bound |
| --- | --- |
| Complete bridge message | 2 MiB encoded hard maximum |
| Snapshot HTML | 2 MiB UTF-8 plus `truncated`/original-byte estimate |
| Metadata URL/title/favicon | 8 KiB / 2 KiB / 16 KiB; validated URL schemes |
| Find/nav/action result | 64 KiB encoded; fixed-depth typed fields |
| Selector/text action input | 8 KiB selector, 64 KiB text, enumerated keys/actions |
| Reports | per-surface token bucket; valid pending result is single-shot |
| Pending requests | H05 limit: 8 per surface, 64 per process |

Tauri dispatch may parse command arguments before user code; verify the framework
enforces a transport/body limit early enough. If not, use a lower-level custom
protocol/channel or patch/configuration that rejects oversized IPC before JSON
allocation. A post-deserialization `String::len()` alone does not close TV-009.

Snapshot script traverses/serializes under node/byte/deadline caps rather than
materializing unlimited `documentElement.outerHTML`. It returns explicit
truncation. Consumers must not treat truncated snapshot/search output as complete.

## Agent action and approvals

Trusted state owns `User`, `Agent`, and `AwaitingOperator` control modes under an
exact tab/surface/navigation generation. User navigation/focus takeover revokes
agent action admission. The bridge never changes control mode.

Classify actions before evaluation:

- ordinary page-local click/type/scroll/select/wait under active agent lease;
- sensitive form submit, account/security, destructive, checkout/payment,
  download/upload, external protocol, and permission-affecting action requiring
  explicit trusted user approval; and
- forbidden password/secret extraction or entry, browser permission changes,
  native file path access, and bypass of origin controls.

Approval binds origin, navigation generation, action type, bounded human-readable
target summary, and expiry. Navigation/DOM-generation invalidation requires a
new approval. Selector-name heuristics are supplemental only; pages control
their DOM. For high-impact actions, present a trusted confirmation immediately
before execution and surface uncertainty rather than claiming success from a
page-provided boolean.

## Trusted shell CSP and resources

Generate separate production/development CSPs. Production begins with
`default-src 'self'`, denies `object-src`, constrains `base-uri`, `frame-src`,
`worker-src`, `form-action`, `connect-src`, `img-src`, fonts/styles/scripts, and
contains no arbitrary remote script or `unsafe-eval`. Any required inline style
or data/blob source is individually justified and covered by a fixture. Daemon
connect sources are the configured authenticated endpoints, not `*`.

Run the production built shell under CSP in tests; a header string that breaks
features and is disabled later is not completion. Add violation capture in
development/test without leaking URLs/query secrets.

Replace broad asset protocol scope (`$HOME/**`, `/Users/**`, documents,
downloads, desktop, temp) with an authorized resource service:

1. trusted co-located UI obtains an H02 root/object handle through a user or
   domain-authorized selection;
2. renderer requests a short-lived opaque resource ID, not an absolute path;
3. native service opens handle-relative/no-follow, checks MIME/size/range, and
   returns bytes with safe headers;
4. ID binds trusted webview, object generation, purpose, and expiry; and
5. remote workshops use authenticated daemon content endpoints instead.

Remote browser webviews cannot load this trusted resource scheme. SVG/HTML and
other active formats render in an isolated preview or as download/text, not in
the privileged shell origin.

## Network and platform boundaries

Remote pages can attempt requests to loopback services independently of Tauri
IPC. H01 must authenticate and apply strict CORS/origin/route exposure to daemon
and BrowserHost control endpoints. H08 tests that browser pages receive no token
through URL, JS global, cookie shared with page origins, referrer, or error text.
BrowserHost uses an unguessable authenticated channel or native in-process API;
port `7422` is not authority by itself.

Platform permission prompts (camera, microphone, geolocation, notifications,
clipboard, MIDI/USB, screen capture) are denied by default. Additions require a
separate capability decision and trusted origin-specific UI. Autofill, password
manager, clipboard access, devtools, extensions, downloads, and external schemes
receive explicit per-platform settings/tests; unspecified platform defaults are
not accepted security policy.

## Observability

Record without page payloads or full URLs:

- resolved capabilities/permissions per concrete webview label and class;
- surface/origin hash, navigation generation, bridge kind, size bucket, result;
- ACL denied, unsolicited, wrong-instance/kind/origin/generation, duplicate,
  oversized, malformed, rate-limited, timeout, and cancelled counts;
- popup/navigation/download/scheme/permission decision classes;
- control/approval state transitions and invalidation reason, without form text;
- CSP directive/blocked-source class and authorized-resource decision; and
- Tauri/Wry/system-webview version plus packaged test matrix revision.

Do not log HTML, DOM selectors containing user text, typed form data, cookies,
headers, credentials, full URLs/query strings, local paths, resource IDs, or IPC
payload bodies. Rate-limit denial logs per surface/origin/reason.

## Migration plan

### H08.0 — Immediate containment and inventory

- Remove `core:default` and every other broad permission from remote labels.
- Split/rename ambiguous trusted chrome and remote content labels; fail CI on
  capability overlap/permission merge.
- Generate complete application/core/plugin permission inventory from the
  locked build and preserve it as a test fixture.
- Until the minimal bridge works, fail affected browser features promptly with
  explicit unavailable UI; do not restore broad permissions to avoid timeouts.

### H08.1 — Native lifecycle hooks

- Move navigation/title/new-window/close/download state to builder/native hooks.
- Enforce scheme/popup/resource/permission policy at first navigation.
- Bind surface registry identity/generation/storage profile before loading.
- Remove corresponding injected command reports and payload-supplied surface.

### H08.2 — Minimal report bridge and H05 broker

- Create the dedicated plugin/application manifest and one `allow-report`
  permission; remote capability grants only it.
- Implement typed report enum, actual `Webview` derivation, native URL/generation
  validation, preparse transport bounds, and rate limits.
- Correlate snapshot/find/action/nav-query through H05 exact request broker.
- Delete singleton senders and uncorrelated scripts/callbacks.
- Disable or narrowly redesign hotkey/favicon/SPA conveniences that cannot meet
  the invariant.

### H08.3 — Bounded snapshots and agent-control policy

- Replace unlimited `outerHTML` with bounded capture/truncation semantics.
- Enforce action input limits, exact surface lease, navigation invalidation, and
  trusted approval/forbidden categories.
- Wire user takeover, close, timeout, and shutdown revocation.
- Test hostile DOM, forged results, reentrancy, redirect, and approval expiry.

### H08.4 — Trusted shell CSP and resource service

- Build CSP from observed production requirements, add violation fixtures, and
  remove `csp: null`.
- Implement H02-authorized resource IDs/protocol/API and migrate local previews.
- Remove broad asset scope and raw absolute path/`convertFileSrc` flows outside
  explicitly co-located authorized adapters.
- Isolate active local formats and test trusted shell injection.

### H08.5 — Packaged cross-platform gate

- Build packaged attacker server/harness and enumerate every generated command.
- Run TV-001–TV-012 and BR-001–BR-007 on macOS/WebKit, Windows/WebView2, and
  Linux/WebKitGTK with exact shipped capabilities/config.
- Add dependency-upgrade job that regenerates/diffs ACL and runs the matrix.
- Remove compatibility flags/scripts/legacy capabilities and ship docs.

H08.0 is release-boundary containment and does not wait for full feature parity.
H08.1/H08.2 coordinate with H05. CSP/resource work can proceed independently
after H02 handles exist. Mobile native overlays require their own equivalent
platform bridge review; desktop success does not automatically prove iOS/
Android, and remote mobile pages receive no broader fallback.

## Rollout and rollback

Roll out fail-closed by feature: first strip remote authority, then enable each
bridge variant only after its packaged positive/negative tests pass. A runtime
diagnostic reports bridge unavailable/ACL denied rather than waiting for a
generic timeout.

Rollback may disable snapshot/action/find or open a page in the external system
browser. It may not restore `core:default`, capability overlap, broad custom
command permission, disabled CSP, or home-wide asset scope. Pin the last
security-validated Tauri/Wry versions and treat downgrade across the remote-ACL
security fix as forbidden.

## Verification and exit criteria

DESKTOP-001 is validated only when:

- generated capabilities show each remote content label in exactly one class
  with no core/default/general plugin/application permission;
- TV-001 invokes every discovered application command and all are denied except
  the single report-only bridge;
- TV-002–TV-004 prove no event/window/webview/menu/tray/resource/path/plugin
  side effect from attacker pages;
- TV-005 proves legitimate native metadata/minimal hints work only on their
  actual bound surface without granting authority;
- TV-006–TV-010 and BR-001–BR-007 prove exact correlation, navigation invalidation,
  size/depth bounds, special-scheme behavior, and leak-free cleanup;
- TV-011 proves production CSP and resource IDs contain trusted-shell injection
  and arbitrary home files remain unreadable;
- TV-012 fails on an intentionally broadened capability/changed ACL and passes
  the proposed dependency upgrade;
- high-risk action, permission, download, external-scheme, control-takeover, and
  approval invalidation tests pass;
- normal/error logs contain no HTML, secrets, form data, cookies, full URLs, or
  local paths; and
- the matrix passes in packaged supported desktop builds with retained ACL and
  system-webview evidence.

Shipped additionally requires rollout, rollback removal, operator diagnostics,
mobile-equivalent review for enabled mobile bridges, and canonical docs.

## Canonical documentation at ship time

- Home app/browser reference: isolation model, cookies/private mode, downloads,
  permissions, control handoff, snapshots, actions, and degraded behavior;
- security architecture: trusted shell vs hostile webview capability inventory;
- contributor docs: adding commands/capabilities, label rules, bridge schema,
  CSP/resource process, and mandatory packaged tests;
- operator runbook: ACL mismatch, bridge denial, CSP violation, stuck request,
  resource denial, dependency upgrade, and safe feature disablement; and
- privacy docs for page snapshot/action data and retention boundaries.

## Superseded code and configuration to delete

- `core:default` and broad `remote.urls` authority in
  `browser-tab-webviews.json`;
- remote labels from `default.json` or any trusted capability;
- ambiguous legacy content/chrome labels;
- page-supplied surface/URL/origin routing authority;
- general `window.__TAURI_INTERNALS__.invoke` use from remote page scripts;
- injected location/title/new-window reports where native hooks exist;
- page-generated hotkey authority without accepted narrow policy;
- unlimited `documentElement.outerHTML` snapshot;
- uncorrelated report commands and H05 singleton response slots;
- any remote permission over the giant application handler;
- `csp: null` in production;
- broad home/document/download/desktop/tmp asset protocol scope; and
- unsafe compatibility flags after packaged release evidence.

## Code anchors

- `apps/medousa-home/src-tauri/capabilities/*.json`
- `apps/medousa-home/src-tauri/tauri.conf.json`
- `apps/medousa-home/src-tauri/build.rs`
- `apps/medousa-home/src-tauri/src/lib.rs`
- `apps/medousa-home/src-tauri/src/human_browser.rs`
- `apps/medousa-home/src-tauri/src/human_browser_ios.rs`
- `apps/medousa-home/src-tauri/src/human_browser_android.rs`
- BrowserHost and browser session/bridge modules
- Home browser compositor/stores/control-handoff and local resource helpers
