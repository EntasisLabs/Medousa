# Shared Browser Workspace

Medousa Home exposes a **human-first desktop browser** in a dedicated window, plus a **mobile iframe browser** under You → Web. Agent integration is intentionally **deferred** until the human browsing experience is solid.

## Desktop (human-first)

### Architecture

```
browser WebviewWindow
├── browser-content (child webview): external URLs — primary rendering surface
├── browser-chrome (child webview): /popout/browser-chrome — tabs + URL bar
└── main webview: about:blank (Tauri requirement, hidden behind children)
```

All layout is **Rust-only** ([`human_browser.rs`](Medousa/apps/medousa-home/src-tauri/src/human_browser.rs)): fixed chrome height (~96px), content fills remainder, window resize handler updates bounds. No DOM `getBoundingClientRect`, no overlay sync.

### State

| Layer | Role |
|-------|------|
| [`humanBrowser.svelte.ts`](Medousa/apps/medousa-home/src/lib/stores/humanBrowser.svelte.ts) | Local tabs, URL draft, history — chrome webview only |
| [`human_browser_*` commands](Medousa/apps/medousa-home/src-tauri/src/human_browser.rs) | Navigate, back, forward, reload on content webview |
| `human-browser-navigated` event | Content → chrome URL/tab sync |

### Entry

- Nav **Web** → `openBrowserWindow()` → `window_show_browser`
- External links / agent (temporary) → `openInBrowser(url)` → show browser + `human_browser_navigate`

### Activity panel (main window)

[`browserContext.svelte.ts`](Medousa/apps/medousa-home/src/lib/stores/browserContext.svelte.ts) listens for `human-browser-navigated` to show current URL in the activity rail.

## Mobile (unchanged)

Web lives under **You → Web** as a full-screen **iframe** browser ([`BrowserPanel`](Medousa/apps/medousa-home/src/lib/components/browser/BrowserPanel.svelte) `mobile={true}`). Agent chat opens as a **bottom sheet** (`MobileBrowserWorkshop`).

## Agent backend (dormant for browser UI)

These remain for daemon/tools but are **not wired to the desktop browser window**:

```
BrowserBridge (medousa-browser-bridge crate)
BrowserHost (Home Tauri, :7422)
Daemon SSE: browser_challenge, browser_navigated
```

### Agent reattach (future)

When human browser passes validation:

1. Agent navigate → `human_browser_navigate` (same as human)
2. Tab groups → optional sync layer on `humanBrowser` tabs
3. CAPTCHA → re-enable banner in chrome route
4. Workshop → separate window or chrome overlay

**Principle:** agent integrates into working human browser; human browser never depends on agent.

## Steve Jobs test

User opens **Web** → dedicated window with Safari-quality rendering. Tabs and URL bar in a fixed chrome strip. No floating white card, no bridge tab-group IDs, no workshop competing with the page.
