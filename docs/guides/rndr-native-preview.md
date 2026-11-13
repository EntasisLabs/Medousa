# Try the native rndr desktop preview

This developer preview is a **second** Medousa app in `apps/rndr-home`.
The normal Tauri app, home-first install and production configuration are unchanged.
It requires the existing rndr checkout used by the repository. It does not install,
launch, repair or deploy the daemon or pair remote hosts.

From the repository root, browse without any connection or credential discovery:

```sh
cargo run --manifest-path apps/rndr-home/Cargo.toml -j 2 -- --offline
```

## Beyond chat

Open **Settings** in the shared rail. The sections and order match the existing
app: Preferences, Accounts, Medousa Agent, Runtime Controls, Sharing, External
Agents, Packages, MCP and Connection. Wide windows have the grouped settings
sidebar; narrow windows have Previous/Next section controls. Sidebar scrolling
keeps all sections reachable. The rail retains production destination names;
unported editors open a clear port-status page, not fabricated content.

- **Preferences** changes light/dark/high contrast in this window only. Nothing is
  persisted, imported from production or inferred from the operating system.
- **Accounts / External Agents** show daemon-reported external runtime availability
  and account status. Unknown is not signed out. Presence is not authentication.
- **Medousa Agent** exposes the cached model catalog and links to agent modes and
  local engine status. Catalog entries do not prove working inference.
- **Runtime Controls** links to health/version, worker capacity, external runtimes,
  schedules and the local engine. **Sharing** reads shared workshop mode.
- **MCP** shows the gateway and connected/disconnected servers, without invoking
  tools. **Connection** reads the responding daemon's authority/version descriptor.
- **Accounts → Hosted review providers** lists discovered providers, not logged-in
  review accounts. **Schedules** in the rail opens the read-only recurring list.

Administrative lists support local filtering, details, Back and explicit refresh.
Typing filters never requests a service or persists configuration. Loading,
permission denial, errors, offline, empty and no-match results are distinct.
Snapshots are not live subscriptions and are capped at 500 entries per service.

For read-only service data, follow the [launch instructions](../../apps/rndr-home/README.md)
and pass the existing **loopback** daemon origin and local data directory with
`--daemon` and `--data-dir`. Existing first-party credentials are read through the
existing local credential boundary. Do not copy tokens or pass them on a command
line. The preview never offers credential entry. It does not refresh providers,
validate credentials with inference, grant permissions or mutate admin configuration.

Sign-in/out, review-account setup, pairing, channels, package inventory/install,
engine start/stop, vault linking, SSH and permission changes remain in the existing
app. Packages is an honest unsupported-host-service page, not a fake installed
inventory. Project/review editing, notes/files/artifacts, identity, Create, Calendar,
Web, Context, Peers, Activity, Agents, Scripts and Flows are not ported in this pass.
See the [destination parity matrix](../../apps/rndr-home/docs/DESKTOP_PARITY.md)
for exact implementation and remaining-view status.

## Chat and native input limitations

**Send, Restore, New, Recent and Reload remain disabled** pending native input
safety proof. Settings/navigation/local filtering do not depend on that gate.
The existing transcript, virtualization, draft preservation and Jump to latest
increment remain. This preview does not repair rndr's IME concern or qualify
native emptiness across frames. Do not use old launch instructions as authority
to enable input-sensitive actions.

Physical IME, keyboard, accessibility, device scaling and native-window behavior
are final user-verification items, not demonstrated production readiness.
High contrast is a palette option, not accessibility certification.

## Reproduce visual evidence without a live connection

```sh
cargo run --manifest-path apps/rndr-home/Cargo.toml -j 2 -- \
  --export-desktop ./desktop-renders
```

This creates synthetic GPU PNGs at 480×760 and 1120×820 in dark, light and
high contrast, including every settings section and administrative state examples.
Images carry a **FIXTURE** / **SYNTHETIC GPU RENDER** label. No credentials are
loaded and no network is contacted. These are not native-window observations or
proof of physical input. Existing `--export` remains the chat fixture exporter.

See [implementation evidence](../../apps/rndr-home/docs/DESKTOP_IMPLEMENTATION.md)
for exact checks, source references, limitations and the compact human checklist.
