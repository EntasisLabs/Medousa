# Browse the web inside Medousa

The **Web** surface keeps a website beside chat. Its browser-source control
shows where the browser runs and which identity it uses:

- **Workshop** runs on the selected workshop daemon. This is where
  agent-created browser work appears. Choose a private browser for temporary
  identity or a saved browser when that workshop should retain its cookies.
- **Device** is Medousa's native browser on the device in your hand. It uses
  that device's cookies and passkeys.

These are separate browser identities. Medousa never moves a login or action
between them automatically when one becomes unavailable.

Every website remains untrusted: it cannot use Medousa's native files,
windows, menus, events, or commands.

## Use a workshop browser

1. Open **Web**, open the browser-source control, and choose an existing
   workshop browser or create a private or saved one.
2. Enter an address. The page runs on the selected workshop even if you close
   the view or disconnect your phone.
3. When Medousa is driving, choose **Take control** before interacting. A tap,
   click, scroll, or typed character also gives you priority and stops queued
   agent actions from continuing against the page.
4. Choose **Hand back** when Medousa may continue.

Workshop browser frames are bounded and sensitive fields are redacted before
they reach Home. On mobile, use **Type** over the page to open the software
keyboard.

## Use the device browser

1. Open **Web** and navigate to an `http://` or `https://` address.
2. Choose **Device** in the browser-source control and browse normally while
   the control indicator says **You**.
3. Hand control to the agent only for the tab and task you want it to work on.
4. Take control back at any time. Navigation, tab replacement, closing the tab,
   timeout, or taking control back cancels pending agent actions.

Websites cannot open popups, start downloads, launch external protocols, or ask
for camera, microphone, location, notification, clipboard, or similar native
permissions through the embedded browser. Open those flows in your system
browser when you explicitly want them.

## What the agent can inspect and change

When a turn needs the current page, Medousa may request a bounded snapshot of
the active tab. A snapshot can include visible or rendered page text and normal
DOM attributes, so treat it like context you deliberately shared with the
turn. Capture is capped and marked when truncated. The native broker holds it
only for the matching request; it does not log page HTML, selectors, form text,
cookies, full URLs, or headers.

Under **Agent** control, ordinary page-local click, type, key, scroll, select,
and wait actions are allowed. Password and file fields, payment/autofill fields,
sensitive form submission, downloads, and active/external links are blocked.
The page's own success message is not treated as trusted proof.

If a page needs a login, payment, upload, permission prompt, download, or another
high-impact step, take control and perform that step yourself. Hand control back
only after the page has reached the state you want the agent to use.

## Local and remote files

Web pages never receive local filesystem paths. In a co-located workshop,
Medousa can preview safe raster images already inside the active vault through a
short-lived, one-use resource handle. Active formats such as SVG and HTML and
files outside the vault are not rendered inline. With a paired or remote
workshop, preview bytes come from that workshop's authenticated daemon instead
of your local disk.

If a browser operation becomes unavailable, use the system browser or take
control and retry. Medousa does not restore broader native permissions as a
fallback.
