# Medousa for Chromium browsers

Medousa for Chromium browsers is a page-aware companion, not a second browser.
The side panel owns the conversation while the current tab supplies bounded
context only after an explicit user action.

## Development

```bash
cd integrations/browser
npm install
npm run build
```

Load the generated `dist/` directory through `chrome://extensions` or
`edge://extensions` with **Developer mode → Load unpacked**. The browser
extension uses the local daemon at `http://127.0.0.1:7419` by default.

Use the gear in the side panel to configure a paired workshop URL and optional
bearer token. Remote workshop origins are requested only when the user saves
that connection.

If the context header still says only **Current page**, reload the unpacked
extension and open the panel by clicking the Medousa toolbar icon once. That
user gesture grants Chrome's temporary page access; the **Refresh context**
control can then request the current site's optional host permission when the
temporary grant is unavailable.

## Current slice

- Chromium Manifest V3 side panel for Chrome and Edge;
- page and selection context captured from an explicit toolbar, shortcut, or
  context-menu action;
- bounded page title, URL, selection, and readable page text context;
- daemon-owned sessions with restore, history, new conversation, rename, and
  delete flows;
- streaming answers, tool progress, budget/permission prompts, cancellation,
  workshop handoff recovery, and durable history reconciliation;
- safe Markdown/code rendering with copy actions;
- a registered `browser_page_snapshot` client tool: the daemon can request a
  fresh active-tab snapshot through the side panel's long-poll bridge;
- local/remote workshop settings with bearer-token storage in extension storage.

The companion intentionally does not advertise `supports_browser_host`:
`browser_page_snapshot` is a read-only client-owned tool, while full
agent-controlled browser actions remain a separate capability that can be added
behind explicit approval after the conversation and permission model is proven.
