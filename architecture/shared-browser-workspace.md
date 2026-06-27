# Shared Browser Workspace

Medousa Home exposes a **human-first browser** embedded in the Web surface (desktop + mobile native webview). Agent integration reattaches on top of the human webview without reversing the human-first rendering model.

## Desktop (human-first + agent metadata)

### Architecture

```
WorkshopShell Web surface
├── HumanBrowserPanel (Svelte chrome, 156px band)
│   ├── tabs + URL bar + BrowserControlHandoff + BrowserCaptchaBanner
│   └── BrowserChromeActions (star / save)
└── main-browser-content (native webview, Rust-positioned below chrome)
```

All embed layout is **Rust-only** ([`human_browser.rs`](Medousa/apps/medousa-home/src-tauri/src/human_browser.rs)): fixed chrome height (156px), content fills remainder. No DOM overlay on the native webview.

### State

| Layer | Role |
|-------|------|
| [`humanBrowser.svelte.ts`](Medousa/apps/medousa-home/src/lib/stores/humanBrowser.svelte.ts) | Rendering source of truth — tabs, URL, history, native navigate |
| [`browser.svelte.ts`](Medousa/apps/medousa-home/src/lib/stores/browser.svelte.ts) | Agent metadata — tab group, control handoff, work card linkage |
| [`agentBrowserCoord.ts`](Medousa/apps/medousa-home/src/lib/utils/agentBrowserCoord.ts) | Fan-out `human-browser-navigated` → human + agent stores |
| [`openInBrowser.ts`](Medousa/apps/medousa-home/src/lib/utils/openInBrowser.ts) | Single entry for agent SSE and user links |
| `human-browser-navigated` event | Native webview → store sync |

### Entry

- Nav **Web** → `openBrowserWindow()` → embed show + layout
- Agent SSE / links → `openInBrowser(url, { openedBy, sessionId, workCardId })`
- CAPTCHA → `browser.setControl("awaiting_operator")` + open challenge URL
- Resume → `resumeBrowserChallenge()` snapshots human webview → `completeBrowserSession`

### Activity panel (main window)

[`browserContext.svelte.ts`](Medousa/apps/medousa-home/src/lib/stores/browserContext.svelte.ts) listens for `human-browser-navigated` to show current URL in the activity rail.

## Mobile

Web lives under the **Web tab** ([`BrowserPanel`](Medousa/apps/medousa-home/src/lib/components/browser/BrowserPanel.svelte)). Agent handoff + CAPTCHA strip in bottom chrome. [`MobileBrowserWorkshop`](Medousa/apps/medousa-home/src/lib/components/mobile/MobileBrowserWorkshop.svelte) for scoped chat.

## Agent backend

```
BrowserBridge (medousa-browser-bridge crate) — tab group metadata
BrowserHost (:7422) — search/fetch/snapshot; prefers human webview when URL matches
Daemon SSE: browser_challenge, browser_navigated
```

### Agent reattach (done)

1. Agent navigate → `openInBrowser` → `humanBrowser.navigate` + `browser` metadata
2. Control handoff → `BrowserControlHandoff` in Web chrome + workshop header
3. CAPTCHA → `BrowserCaptchaBanner` + `resumeBrowserChallenge` (webview HTML → daemon complete)
4. Workshop → desktop `BrowserWorkshop` floating panel; mobile bottom sheet
5. Tool snapshots → `human_browser_snapshot_*` when active URL matches

**Principle:** agent integrates into the working human browser; human browser never imports agent stores.

## Steve Jobs test

User opens **Web** → full-width browser with crisp chrome. Agent researches → same tab updates; handoff shows who is driving. CAPTCHA solved in-place; agent continues with shared cookies. Chat workshop floats over the page like vault note workshop — never competes with the page for width.
