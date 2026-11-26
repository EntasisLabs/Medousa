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
metadata preserve the selected document and its scroll position. **Reload
preview** starts it fresh or retries a failed load. Leaving Artifacts retires the
preview; returning loads the selected artifact again.
The source chat opens in its original workshop, even if your other chats use a
different workshop. HTML export uses a **Save As** picker on this device; it does
not write to the workshop's filesystem or change the saved original. Export and
preview both enforce the 2 MiB HTML limit. Preview may be unavailable in your
build; the document area explains that fallback, and browsing, source-chat
navigation and HTML export do not require WebKit. Native previews remove the
legacy Medousa-injected scroll-lock stylesheet, like Tauri; authored layout/CSS,
the saved artifact and exported bytes are unchanged.

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
Inline scripts, inline styles and data images can work. External networking,
storage, navigation, forms, popups, downloads and native commands are blocked.
Artifacts that depend on remote fonts, scripts or other external assets will not
render the same way as they do in an unrestricted browser.
