# Workshops in the native desktop app

The RNDR client can keep several workshop daemons available at once. There is
**no active-workshop switch**: selecting a source for a new chat or one view does
not redirect existing chats, other views, streams, or pending writes.

## First setup

Open **Workshops / setup** above the workspace. An existing saved local Personal
connection can attach immediately. If its daemon is not running, **Set up / start
Personal** deliberately uses the existing bundled-daemon startup/credential path.
It does not silently download workloads, install another daemon, or borrow
credentials from a different data directory.

To connect another machine, enter its HTTP(S) address and paste the trusted
pairing link generated on that workshop. Choose **Pair and save workshop**, then
**Connect** on its saved row. This uses the same signed-link verification,
per-workshop saved credentials, refresh, and LAN/Iroh routes as the Tauri client.
Sharing-only peer connections are not full client workspaces.

## Chats and destinations

The sidebar combines the connected workshops' chat indexes. Each row shows its
source label, even when two workshops use the same chat id or title. Tabs and
splits retain their owning authority. Opening, renaming, deleting, sending,
replying, approvals, attachments, streams, and chat caches follow that owner.
They do not follow the destination selector.

The header's **New chat destination** controls only newly created conversations.
With one saved workshop it is initialized automatically. With several saved
workshops, choose any unresolved destination explicitly: an offline workshop
must not cause a silent selection of a different daemon.

## View-local sources

The header shows the source of the current view. Notes, Create, Reviews and Code
can use different sources simultaneously. Changing one view does not change the
others. Notes and canvas reads/writes use the view's source, not a client-local
filesystem or another daemon's vault.

A source change is refused while that view has unsaved edits, a pending or
uncertain mutation, text composition, a review draft, or an owned editing/shell
session that must be preserved. Save/discard or finish the work first. Old read
responses are fenced by the captured source and a view epoch, including switching
away and back to the same source. Canvas texture keys also include the authority.

Calendar, Peers and Settings currently retain their initial view binding for the
window; they can make an explicit initial choice when no binding exists. Native
appearance and window layout are device-local, not selected through the legacy
Tauri active-workshop field. The existing PTY service remains local-workshop
only; a paired source never falls back to another workshop's terminal.

## Offline and recovery

Use **Reconnect / refresh** for that saved source. Another workshop remains usable
while this source is offline. A loaded chat stays owned by its original authority;
failed actions are not redirected or automatically replayed. Reconnect retries
reads/catalog refresh, not an uncertain mutation. Pairing credentials are reloaded
by the exact saved workshop id and checked against the pinned device identity.

Native saves only source preferences (per-view destinations, new-chat destination,
and verified identity aliases) in `native-sources.json` in its client config
directory. It does not replicate daemon content. Restoring these preferences does
not substitute another daemon when a saved source is unavailable. An endpoint
advertising a different authority is rejected for existing items; add a deliberate
replacement as a distinct trusted workshop.

Pasting a pairing link is supported. Camera/QR-image import and automatic daemon
installation are not part of this flow. A real remote LAN/Iroh pairing round trip
still depends on the workshop's network and trust setup.
