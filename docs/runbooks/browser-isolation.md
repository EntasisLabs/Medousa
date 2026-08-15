# Desktop browser isolation operations

Use this runbook when a packaged desktop build reports browser bridge denial,
CSP failures, missing local previews, or a dependency upgrade changes Tauri
authority.

## Release gate

From `apps/medousa-home`:

```bash
npm ci
npm run check:browser-capabilities
npm run check
npm run build
cargo test --manifest-path src-tauri/Cargo.toml request_broker_tests:: --lib
cargo test --manifest-path src-tauri/Cargo.toml authorized_resource::tests:: --lib
```

The capability check must run before every packaged platform build. It compares
the complete application-command inventory, remote/trusted label classes,
report-only bridge permission, production CSP, disabled asset protocol, and
locked Tauri/Wry versions. When generated Tauri schemas are present it also
compares the generated remote capability and plugin ACL.

For the retained packaged-system-webview evidence, build/install the candidate,
run `npm run test:browser-attacker`, and open the printed loopback URL inside
the candidate's **Web** surface. The page attempts every inventoried application
command and every command discovered from the generated core/plugin ACL. Keep
the reported JSON with the release evidence. A pass requires every forbidden
attempt to be ACL-denied while the sole report bridge is admitted and rejects
its deliberately invalid closed-schema envelope. Run this on macOS/WebKit,
Windows/WebView2, and Linux/WebKitGTK; the static CI job does not replace this
packaged runtime evidence.

Do not regenerate the inventory merely to make CI green. Review the command,
capability, generated ACL, CSP, and dependency diff first, then run:

```bash
npm run check:browser-capabilities -- --write
```

## Failure classes

- **ACL mismatch:** inspect `capabilities/default.json`,
  `capabilities/browser-tab-webviews.json`, and generated
  `gen/schemas/{capabilities,acl-manifests}.json`. A remote content label must
  receive exactly `browser-bridge:allow-report`.
- **Bridge denied/unavailable:** confirm the concrete label is
  `browser-content-embed-*` or `browser-content-popout`, the URL is HTTP(S), and
  the report matches the pending webview, surface, kind, navigation generation,
  and request ID. Do not restore a broad permission.
- **Stuck request:** inspect payload-free
  `human_browser_request_diagnostics`. Navigation, control takeover, close,
  timeout, and shutdown should increase cancellation or stale counters and
  leave no retained pending request.
- **CSP violation:** use the effective directive and safe source class emitted
  as `[medousa-security] CSP blocked content`. Never add a remote script source
  or `unsafe-eval`; bundle executable code. Do not paste blocked URLs or queries
  into logs/issues.
- **Resource denied:** verify the workshop is co-located, the object is inside
  the active vault, is a supported raster MIME, is at most 8 MiB, and the same
  trusted webview consumes its one-use ID before the two-minute expiry. Do not
  re-enable `protocol-asset` or broaden filesystem scope.
- **Dependency upgrade:** regenerate Tauri schemas with the proposed lockfile,
  inspect all ACL/default-set changes, run the three-platform CI matrix, then
  update the reviewed inventory. A downgrade across the validated ACL boundary
  is not a rollback.

Safe feature disablement may turn off snapshot/action/find or open a URL in the
system browser. It may not restore `core:default`, application commands, general
plugin permissions, parent-window grants, broad asset paths, or disabled CSP to
remote content.
