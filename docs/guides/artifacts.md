# Show saved artifacts in chat

Ask Medousa to show something it already created, for example:

> Show me the homelab topology diagram again.

Medousa can find saved HTML artifacts from this chat or earlier chats on the
active workshop and display the latest saved version in your conversation.
Showing it again keeps the original artifact; it does not create another copy
or revision.

The default is an inline card. You can also ask to open it in a side panel or
full screen. These choices affect how it appears in the chat, rather than
changing the saved artifact.

You can still browse and open it from **Artifacts** on desktop, or
**You → Library → Artifacts** on mobile. Deleted or unavailable artifacts cannot
be shown again.

## Native artifact library

In the RNDR-based native app, choose **Artifacts** in the navigation rail.
Its workshop selector controls this destination independently of the current
chat. The library lists saved HTML from all conversations in that workshop,
newest first, with titles, dates and sizes.

Type to filter the loaded list; press **Enter** or **Search workshop** to search
across the workshop. The list is bounded to the latest 500 matches; search for a
more specific title or artifact ID to find older items. **Clear** restores the
unfiltered list and **Refresh** reloads it. Connection errors keep the loaded
list available and offer an explicit retry.

Select an artifact to render it automatically in the document area. A compact
toolbar in the window’s titlebar keeps the title, file details, reload, source
chat and HTML export within reach, with no separate destination or document
header below it. In narrow windows the actions
use icons with hover hints and accessible names; hover the truncated title to
read it in full. The library
sidebar and **Open source chat** / **Export HTML…** actions remain available.
Scroll inside the preview to read long artifacts; interactive controls stay in
that isolated document. Resizing, filtering the list and updating library
metadata preserve the selected document and its scroll position. Hover hints
and nonmodal popovers cover only their local area; they do not hide or reload
the whole artifact. Modal dialogs temporarily hide the guest to keep native
focus and input with the dialog. **Reload
preview** starts it fresh or retries a failed load. Leaving Artifacts retires the
preview; returning loads the selected artifact again.
The source chat opens in its original workshop, even if your other chats use a
different workshop. HTML export uses a **Save As** picker on this device; it does
not write to the workshop's filesystem or change the saved original. Export and
preview both enforce the 2 MiB HTML limit. Preview may be unavailable in your
build; the document area explains that fallback, and browsing, source-chat
navigation and HTML export do not require WebKit. Native previews remove the
legacy Medousa-injected scroll-lock stylesheet, like Tauri; authored layout/CSS,
the saved artifact and exported bytes are unchanged. Native previews also contain
root-level overscroll at document edges, preventing elastic scrolling beyond the
document's bounds. Normal document and nested scrolling remain available; this
presentation rule does not alter the saved or exported HTML.

PNG/PDF export and sharing are not yet available in this native destination.

## Native app preview

In the RNDR-based native app, HTML artifacts appear as **Open HTML artifact**
controls in chat, including saved artifacts shown again and successful tool
outputs. Opening one uses a full-window isolated preview with a **Close preview**
button. Opening another starts a fresh document; closing the preview or leaving
the conversation retires it. The saved presentation choice is not changed.

HTML preview is enabled by default in Medousa on macOS:

```bash
cargo run --manifest-path apps/medousa-native/Cargo.toml
```

Medousa enables RNDR’s optional isolated artifact backend by default; RNDR’s own
library defaults are unchanged. To build Medousa without native HTML preview:

```bash
cargo run --manifest-path apps/medousa-native/Cargo.toml --no-default-features
```

The macOS backend uses private WebKit SPI and is **not App Store/public-API-only
support**. Builds with preview disabled and unsupported platforms show
**Preview unavailable**; they do
not open the HTML in an ordinary browser as a fallback. Preview HTML is limited
to 2 MiB, loaded using the conversation's authenticated workshop connection.
The preview renders as the main WebKit document, rather than a scrolling inner
frame. Its sandbox is enforced by an immutable response policy; the backend
checks that policy before admitting artifact content and refuses the preview if
isolation cannot be verified. This avoids the inner-frame rendering path without
limiting wheel speed or changing the saved HTML.
Inline scripts, inline styles and data images can work. External networking,
storage, navigation, forms, popups, downloads and native commands are blocked.
Artifacts that depend on remote fonts, scripts or other external assets will not
render the same way as they do in an unrestricted browser.

### Diagnose fast-wheel flashes in a native development build

For a physical-mouse investigation on macOS, the development checkout includes
an opt-in RNDR artifact probe. It records native wheel delivery and view lifecycle
metadata without changing scroll speed, guest styling or sandbox policy. It does
not record document HTML or ordinary typed keys, and needs no global input
monitoring permission. A wheel callback is **not** evidence that WebKit painted
the destination; the probe alone does not establish a tile-cache failure.

From the Medousa repository root:

```sh
scripts/native-artifact-probe.sh build
# Quit the existing native app normally before starting the diagnostic instance.
scripts/native-artifact-probe.sh start
```

Open the affected artifact. Try a short slow-scroll baseline, a fast jump across
the document, then rapid direction changes. Immediately after a visible flash,
press **Cmd+Shift+M** with the artifact window focused to mark the approximate
observation time. The marker follows your reaction, not the exact faulty frame.
The probe observes but does not consume this shortcut. From another terminal,
`mark` also requests a marker without moving app focus.

When finished, run `scripts/native-artifact-probe.sh stop` in another terminal.
Wait for the flushed-summary confirmation before quitting; abrupt process exit
can lose the buffered tail. This stops logging without quitting the app. Use
`status` to find the private run directory under `.tmp/artifact-probes/`; a new `start` creates a new
run. The launch terminal stays attached, with app output in `console.log`.
Opening an artifact initializes the RNDR JSONL log; an absent log is not a clean
result. Share the run location and which marked attempts flashed. Collection stops after ten minutes or 20,000 records, whichever comes first;
use a short, repeatable session. Normal launches do not
enable it. This is diagnostic instrumentation, **not a rendering fix**.
