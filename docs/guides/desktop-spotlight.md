# Desktop Spotlight and work windows

With Medousa running, press **⌘ Shift Space** on macOS or **Ctrl Shift Space** on
Windows/Linux to show Desktop Spotlight over your current app. Press the shortcut
again, press **Escape**, or click outside it to dismiss it. Closing the main
Medousa window leaves the launcher available. Use the tray or menu-bar **Quit**
action to quit Medousa completely.

You can also choose **Spotlight** from Medousa’s tray/menu-bar menu or search for
**Desktop Spotlight** in the app’s command center. **⌘/Ctrl K** still opens the
in-app command center. If another app owns the global shortcut, Medousa reports
the registration failure in Desktop Spotlight; the menu entry remains available.

## Open your work

Type to search recent conversations, Create canvases, and Code projects. Press
**Enter** or click a result to open it in its own native window. **Chat**,
**Create**, and **Code** open their respective browsers; **New chat** creates a
conversation. **Open Medousa** returns to the full workspace and its tabs.

Opening the same conversation, canvas, or Code project again focuses and restores
its existing window. Files requested for an open Code project open inside that
project’s existing window. Different workshops keep separate window identities.

These are the same editors and daemon-backed resources used in the main app.
Main-window tabs and detached navigation remain independent. Saved Create edits
refresh in clean views, Code uses its existing project-change reconciliation, and
chat receipts and unchanged composer drafts synchronize between windows.
Unsaved edits are preserved when another view changes the same resource; the
existing revision/digest checks still protect saves from overwriting newer work.

## Closing and workshop ownership

Closing a work window saves pending canvas edits or modified Code files before
releasing its renderer. Chat composer drafts persist too. A failed save keeps the
window open so you can resolve it or explicitly close without saving. Closing a work window detaches its stream consumers; admitted daemon
jobs continue running.

Switching workshops detaches the main window’s live subscriptions without
cancelling accepted turns, jobs, or development servers. Running work does not
require a switch confirmation. Returning to a workshop reloads its saved session
and reattaches to any live turn. Unsaved notes still save before switching, and
Code checkpoints unsaved buffer drafts without writing them into source files. A checkpoint/save failure keeps
you in the current workshop.

A work window keeps addressing its original workshop when the main app switches
workshops. Chat actions, agent approvals, attachments, artifacts, profiles, workshop
preferences,
and Code SSH setup use the workshop that owns their window. They continue to work
when the main window selects another workshop. Sign-in stays bound to its original
workshop for the whole flow. Device setup and maintenance remain in the main window.

Desktop Spotlight is a desktop-only feature. The existing mobile and browser tab
workflows are unchanged.
