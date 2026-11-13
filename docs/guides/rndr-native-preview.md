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
sidebar; narrow windows retain Previous/Next section controls. **Choose settings
view** directly opens any section at either width and remembers the last section
for this window. Tab/Shift-Tab and arrows traverse the chooser; Home/End jump to
the first/last section, Enter/Space activate, and Escape dismisses without changing
the route and returns focus to the chooser. Sidebar scrolling keeps all sections
reachable. Primary rail doors scroll while Activity/Settings/You stay docked.
The rail retains production destination names;
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

**Activity** opens a read-only snapshot of up to 100 workspace work cards,
including terminal cards. It is useful job/work-item metadata, not the full durable
job ledger or the unified global Activity feed. Rows show board state, identity,
created/updated UTC and snapshot revision. Prompt-derived titles, raw status prose,
outputs and diagnostics are withheld. Board state is not proof of execution
success. No cancel, retry, approval, archive or execution controls are provided.

**Notes** now opens a bounded Library summary browser for the responding daemon's
current vault. It requests only the first page (up to 100 summaries **before**
exclusions), not a complete inventory or filesystem listing. Browse title,
inert vault-relative Markdown path and modified UTC; filter those local labels,
open summary details, and return with Back. Note bodies, tags, backlinks, raw
kind strings and pagination cursors are not displayed. System/developer, hidden
and chat/session/profile-linked notes are excluded with no opt-in here. Unsafe
or duplicate identifiers and reset-required pages fail closed; no path is opened.
Profile/session associations are not authorization proof: this conservative preview
can omit ordinary notes that were automatically tagged on a non-default profile.
An empty visible first page does **not** prove the vault is empty. Refresh reloads
the head page; pagination, root selection, bodies, attachments and all content
writes remain deferred. Use the existing app for reading full notes or editing.

Administrative and Notes lists support local filtering, details, Back and explicit refresh.
Back restores the results viewport and row focus; filter and navigation changes
start at the top. A refresh clears positional detail selection before new data.
Typing filters never requests a service or persists configuration. Loading,
permission denial, errors, offline, empty and no-match results are distinct.
Snapshots are not live subscriptions. Activity caps at 100 work cards; Notes
caps its first page at 100 summaries before exclusions; other services cap displayed
rows at 500. Fixture, live-read snapshot and retained offline snapshot labels
remain distinct; none claims continuous service liveness.

For read-only service data, follow the [launch instructions](../../apps/rndr-home/README.md)
and pass the existing **loopback** daemon origin and local data directory with
`--daemon` and `--data-dir`. Existing first-party credentials are read through the
existing local credential boundary. Do not copy tokens or pass them on a command
line. The preview never offers credential entry. It does not refresh providers,
validate credentials with inference, grant permissions or mutate admin configuration.

Sign-in/out, review-account setup, pairing, channels, package inventory/install,
engine start/stop, vault linking, SSH and permission changes remain in the existing
app. Packages is an honest unsupported-host-service page, not a fake installed
inventory. Project/review editing, full note reading/editing, Files/Artifacts,
identity, Create, Calendar, Web, Context, Peers, Agents, Scripts and Flows remain
deferred. Notes summary browsing is a bounded preview, not complete Library parity.
The wider unified Activity feed is also deferred.
See the [updated 18-destination matrix](../../apps/rndr-home/docs/CONTENT_SURFACES.md)
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

# Smaller Notes-only matrix (9 states × 4 sizes × 3 themes):
cargo run --manifest-path apps/rndr-home/Cargo.toml -j 2 -- \
  --export-notes ./notes-renders
```

This creates synthetic GPU PNGs at 480×760, 1120×820, 480×650 and 1120×650
in dark, light and high contrast. The full desktop command includes every settings
section, the open chooser and administrative examples; Notes-only exports nine
Notes states, not the rest of the desktop.
Images carry a **FIXTURE** / **SYNTHETIC GPU RENDER** label. No credentials are
loaded and no network is contacted. These are not native-window observations or
proof of physical input. Existing `--export` remains the chat fixture exporter.

See [Notes implementation evidence](../../apps/rndr-home/docs/content-evidence/REPORT.md)
for the current checks, exact inspected PNG list and remaining human checks.
[Prior shell/Activity evidence](../../apps/rndr-home/docs/shell-activity-evidence/REPORT.md)
remains historical evidence for that integrated increment.
