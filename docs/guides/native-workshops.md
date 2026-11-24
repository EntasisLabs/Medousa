# Workshops in the native desktop app

The RNDR client keeps several workshop daemons available together. There is
**no global workshop switch** and no source toolbar above chat or other views.
Opening the app should open your work, not ask you to choose a daemon again.

## Configured launch

Existing saved connections attach concurrently using their own credentials.
Native destinations already saved in `native-sources.json` take precedence.
For destinations not yet saved, the previously configured workshop seeds the
new-chat destination and each view's initial binding. The seed is captured once
at launch; it is not a global runtime selection. A faster Personal connection
never replaces a configured remote destination. No reselection or re-pairing is
needed for a valid saved connection.

Settings reads follow their view's captured workshop. Your daemon's runtime
settings remain on that daemon; connecting does not rewrite them. Appearance
prefers the native device-local choice, then the prior workshop's saved theme,
then the built-in default. Reading this launch seed does not modify the registry.
Window layout remains device-local.

An offline default stays pinned. History from other available workshops can still
appear, but writes do not move to those workshops. An invalid/missing saved
identity is not replaced with Personal. Open **Settings → Connection** to inspect
or reconnect it. Corrupt native source preferences are reported there and are not
overwritten by a successful connection.

## Optional connection setup

Use **Settings → Connection** only when you want to manage connections or change
a view's source. A saved Personal connection can attach immediately. If its daemon
is not running, **Set up / start Personal** deliberately uses the existing bundled
startup/credential path. Launch does not download workloads, install another
daemon, start an offline daemon, or borrow credentials from a different directory.

To add another machine, enter its HTTP(S) address and paste the trusted pairing
link generated on that workshop. Choose **Pair and save workshop**, then
**Connect** on its saved row. This uses shared signed-link verification,
per-workshop saved credentials, refresh, and LAN/Iroh routes. Sharing-only peer
connections are not full client workspaces. Pasting a pairing link is supported;
camera/QR-image import and automatic daemon installation are not included.

## Chats and view-local sources

The sidebar combines connected workshops' chat indexes. When several sources are
present, quiet secondary labels distinguish their rows, including identical chat
IDs or titles. Single-source rows do not need extra provenance. Tabs and splits
retain their owning authority. Opening, renaming, deleting, sending, replying,
approvals, attachments, streams, and caches follow that owner.

New chat works directly with the restored destination. To change it deliberately,
use **Settings → Connection**, choose **Chat**, then a saved source. This affects
only future conversations, never retained chats or other views.

The same optional settings control can change **Notes**, **Create**, **Reviews**,
or **Code** independently. There is no always-on source selector in those views.
Notes and canvas reads/writes use the view's source, not a client-local filesystem
or another daemon's vault.

A source change is refused while that view has unsaved edits, pending or uncertain
mutations, text composition, a review draft, or owned editing/shell sessions.
Save/discard or finish that work first. Old read responses are fenced by the
captured authority and view epoch, including switching away and back to the same
source. Canvas texture identities also include the authority.

Calendar, Peers, and Settings retain their initial view binding for the window;
they can make an explicit initial choice if no binding exists. PTYs remain local
workshop only; a paired Code view never borrows another workshop's terminal.

## Offline and recovery

In **Settings → Connection**, use **Reconnect / refresh** for the saved source.
Other workshops remain usable while it is offline. Reconnect refreshes
reads/catalogs, not uncertain mutations. Failed actions are not redirected or
automatically replayed. Pairing credentials reload by exact saved workshop ID and
are checked against the pinned device identity. An endpoint advertising a changed
authority is rejected for existing items; add a deliberate replacement as a
distinct trusted workshop instead.

Native saves only device-local source preferences (per-view destinations,
new-chat destination, and verified identity aliases) in `native-sources.json`.
It does not replicate content or change the legacy active-workshop registry field.
Real LAN/Iroh pairing and credential-refresh round trips still require the target
workshop's network and trust setup; hermetic tests are not proof of live pairing.
