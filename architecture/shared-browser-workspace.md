# Shared Browser Workspace

Medousa exposes one Browser surface with two explicit identity lanes:

| Source | Owner and identity | Default use |
|---|---|---|
| **Workshop** | Selected daemon; ephemeral or named persistent profile | Primary lane for agent-created browser work and remote execution |
| **Device** | Home's native WebView; this device's cookies and passkeys | Direct human browsing and compatibility fallback |

The source control is visible on desktop and mobile. Medousa never silently
moves a URL, action, cookie jar, or authenticated session between these lanes.
The sections below describe the Device implementation; the Workshop attachment
uses the same product surface but remains governed and executed by the daemon.

## Workshop source (daemon-owned + attached)

`GovernedBrowserStore` discovers isolated worlds on the active workshop and
keeps source preference workshop-scoped. A newly attached agent world can
surface in Browser; a deliberate source choice remains explicit. Home polls a
bounded redacted frame, retaining only the latest image, and slows the cadence
when the view is idle or hidden.

The attached frame is interactive on desktop and mobile. Home maps pointer,
wheel, touch, and keyboard input into the observation's CSS viewport and sends
the document id plus observation revision. The daemon admits that request as
human intent, takes control to fence pending agent work, revalidates the permit
beside CDP, and then dispatches input. Detaching Home does not stop the world.

## Device source (human-first + agent metadata)

The Device source is a **human-first browser** embedded in the Web surface
(desktop + mobile native webview). Agent integration reattaches on top of the
human webview without reversing the human-first rendering model.

## Desktop (human-first + agent metadata)

### Architecture

```
WorkshopShell Web surface
├── HumanBrowserPanel (Svelte chrome — tabs, URL bar, find bar, banners)
│   └── [data-browser-embed-host] measured by BrowserCompositor
└── main-browser-content (native webview, DOM-measured bounds via embed_set_bounds)
```

Embed layout is owned by **[`browserCompositor.ts`](Medousa/apps/medousa-home/src/lib/utils/browserCompositor.ts)**: measures `[data-browser-embed-host]` with `ResizeObserver`, batches `humanBrowserEmbedSetBounds` + show/hide via rAF. Rust ([`human_browser.rs`](Medousa/apps/medousa-home/src-tauri/src/human_browser.rs)) stores placement as `EmbedPlacement::Freeform` and re-applies on show after hide (placement is preserved across hide).

### State

| Layer | Role |
|-------|------|
| [`humanBrowser.svelte.ts`](Medousa/apps/medousa-home/src/lib/stores/humanBrowser.svelte.ts) | Rendering source of truth — tabs, URL, history, native navigate |
| [`browser.svelte.ts`](Medousa/apps/medousa-home/src/lib/stores/browser.svelte.ts) | Agent metadata — tab group, control handoff, work card linkage |
| [`agentBrowserCoord.ts`](Medousa/apps/medousa-home/src/lib/utils/agentBrowserCoord.ts) | Fan-out `human-browser-navigated` + `human-browser-new-window` → human + agent stores |
| [`openInBrowser.ts`](Medousa/apps/medousa-home/src/lib/utils/openInBrowser.ts) | Single entry for agent SSE and user links |
| `human-browser-navigated` event | Native webview → store sync |

### Entry

- Nav **Web** → `openBrowserWindow()` → compositor attach + embed show
- Agent SSE / links → `openInBrowser(url, { openedBy, sessionId, workCardId })`
- CAPTCHA → `browser.setControl("awaiting_operator")` + open challenge URL
- Resume → `resumeBrowserChallenge()` snapshots human webview → `completeBrowserSession`

### Activity panel (main window)

[`browserContext.svelte.ts`](Medousa/apps/medousa-home/src/lib/stores/browserContext.svelte.ts) listens for `human-browser-navigated` to show current URL in the activity rail.

## Mobile

Web lives under the **Web tab** ([`BrowserPanel`](Medousa/apps/medousa-home/src/lib/components/browser/BrowserPanel.svelte)). Same **BrowserCompositor** drives native embed on iOS and Android (mobile mode: `human_browser_embed_apply_mobile_layout` + measured panel bounds). Agent handoff + CAPTCHA strip in bottom chrome. [`MobileBrowserWorkshop`](Medousa/apps/medousa-home/src/lib/components/mobile/MobileBrowserWorkshop.svelte) for scoped chat.

### iOS native overlay

Tauri 2 `Window::add_child` is desktop/Android-only. On **iOS**, the human browser uses a **UIKit WKWebView overlay** ([`human_browser_ios.rs`](Medousa/apps/medousa-home/src-tauri/src/human_browser_ios.rs)) — same invoke surface as desktop. Compositor passes `content_bounds` from DOM; snapshots run through `evaluateJavaScript`.

### Android native overlay

On **Android**, native embed uses Tauri `add_child` on the main window ([`human_browser_android.rs`](Medousa/apps/medousa-home/src-tauri/src/human_browser_android.rs) re-exports [`human_browser.rs`](Medousa/apps/medousa-home/src-tauri/src/human_browser.rs)). Same compositor mobile path as iOS. Iframe fallback remains for web dev (`!isTauri()`).

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
