# Liquid markdown × HTML-in-Canvas — fit investigation

**Generated:** 2026-07-23  
**Scope:** Research / spike only — no runtime behavior changes  
**Audience:** Home UI + Liquid authors  
**Status:** Watch / defer for production; prototype-worthy on Windows flag builds only

---

## Executive summary

**HTML-in-Canvas** (WICG / Chromium origin trial) lets you draw real DOM children into 2D / WebGL / WebGPU canvases while keeping hit-testing, a11y, find-in-page, and text selection. That is a genuine unlock for *shader-warped Liquid*, *3D slides*, and *export without html2canvas*.

**It does not fit Medousa Home’s production Tauri 2.0 surface today.**

| Platform | Webview | HTML-in-Canvas outlook |
|----------|---------|------------------------|
| Windows | Evergreen **WebView2** (Chromium) | Possible *after* the API ships stable in Edge/WebView2 — not in origin trial / Canary-only form |
| macOS / iOS | **WKWebView** (WebKit) | No signal; WebKit would need its own impl |
| Linux | **webkit2gtk** | Same WebKit gap |
| Android | System Chromium WebView | Follows Android WebView Chromium lag, not Canary flags |

Liquid already owns a strong DOM/SVG path (fences → placeholders → Svelte archetypes). The right move is a **layered motion roadmap**: deepen CSS/SVG Liquid motion now, keep Konva/Fabric deferred (ADR-010), and treat HTML-in-Canvas as a **capability-gated experiments lane** once WebView2 exposes `drawElementImage` without flags.

---

## What Liquid is today

Liquid markdown is **not** a canvas editor. Pipeline:

```
fence (```card / ```chart / …)
  → preprocessLiquidEmbeds (inert placeholders)
  → marked + DOMPurify
  → {@html} in MarkdownContent / vault preview
  → hydrateMarkdownContainer
       → highlight.js, mermaid SVG, vault images
       → hydrateLiquidEmbeds → mount LiquidMdHost → Svelte archetypes
```

**Motion today (DOM/CSS):**

- Host enter: `.liquid-md-enter` (~320ms fade/slide); stagger on tabs / steps / accordion / tree
- Chart mount keyframes (scatter / radial / radar / pie); `prefers-reduced-motion` respected
- Slides: declarative `motion: none | fade | fade-up` (ADR-010)
- Dashboard / skeleton shimmer for streaming scene nodes
- Card detail sheet slide

**Graphics:** charts are **LayerCake + SVG**, not `<canvas>`.  
**Export:** hydrate then rasterize via `html2pdf.js` / `html2canvas`-style capture (`vaultExportPrep.ts`).  
**Deferred:** Konva / Fabric freeform canvas (ADR-010).

Canonical docs: [`docs/cookbook/liquid-markdown.md`](../../../docs/cookbook/liquid-markdown.md).

---

## What HTML-in-Canvas actually is

Living explainer: [WICG/html-in-canvas](https://github.com/WICG/html-in-canvas) · demos: [html-in-canvas.dev](https://html-in-canvas.dev/) · Chrome origin trial writeup: [developer.chrome.com](https://developer.chrome.com/blog/html-in-canvas-origin-trial).

Three primitives (+ helpers):

1. **`layoutsubtree`** on `<canvas>` — direct children participate in layout / hit-test / a11y, but are not painted until drawn.
2. **`drawElementImage()`** (2D) / **`texElementImage2D`** (WebGL) / **`copyElementImageToTexture`** (WebGPU) — paint a child into the canvas / GPU texture.
3. **`paint` event** (+ `requestPaint()`) — redraw when child rendering changes.
4. Sync helper: apply returned `DOMMatrix` to `element.style.transform` so clicks / selection / a11y line up with pixels.

**Not** `html2canvas`. The browser paints styled HTML natively (fonts, ligatures, RTL) into the canvas buffer, with privacy exclusions (cross-origin embeds, system colors, visited links, etc.).

**Availability (mid-2026):** Chromium **147+** behind `chrome://flags/#canvas-draw-element` (Canary / recent Brave). Origin trial targeting Chrome **148–150**. API still early; details may change. Three.js (`HTMLTexture`) and PlayCanvas have experimental hooks.

---

## Fit against Tauri 2.0 Home

Home is Tauri **2** + Svelte **5** + platform webviews ([Tauri webview versions](https://v2.tauri.app/reference/webview-versions/)):

```
┌─────────────────────────────────────────────────────────┐
│  medousa-home (Tauri 2 shell)                           │
│  ┌───────────────────────────────────────────────────┐  │
│  │  Webview: WebView2 | WKWebView | webkit2gtk       │  │
│  │  Liquid = DOM + SVG Svelte mounts                 │  │
│  │  Export = DOM → html2canvas / pdf                 │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
         ▲
         │  HTML-in-Canvas needs Chromium API shipping
         │  in *that* webview build, not Chrome desktop
         ▼
   Windows WebView2 ≈ Edge Chromium (evergreen, but
   experimental flags / origin trials ≠ app-usable)
   Apple + Linux = WebKit → no HiC path without WebKit
```

### Hard constraints for Medousa

1. **Cross-platform parity** — Liquid fences must work the same in chat on phone (WKWebView / Android WebView) and desktop. A Windows-only shader path would fork the product model.
2. **Sanitize + allowlist** — agents emit fences, not raw HTML/CSS. Canvas shaders must stay behind client-owned archetypes, not freeform author CSS.
3. **Chat scroll performance** — HiC paint is JS-driven; Chrome docs warn scrolling/animations inside canvas don’t update independently of the main thread. A long chat transcript full of canvas-hosted cards would be the wrong default.
4. **Export** — HiC could *eventually* replace brittle `html2canvas` for PDF/DOCX snapshots (spec use case: media export). Until the API is everywhere Home runs, export must keep a DOM fallback.
5. **ADR-010** already rejected heavy canvas editors for slides v1; HiC is complementary (DOM→texture), not a reason to revive Konva for authoring.

### Soft opportunities (when gated)

| Opportunity | Why HiC helps | Liquid surface |
|-------------|-----------------|----------------|
| Post-process shaders on cards/charts | CRT, chromatic, glass, paper grain without SVG filter soup | opt-in `effect:` on `card` / `chart` / `report` |
| Immersive slides / present mode | HTML slide bodies as WebGL textures on a stage | `SlidesPlayer` only (not inline chat) |
| Honest PDF/PNG export | Native paint → canvas → bytes, fewer color-mix / filter bugs | `vaultExportPrep` |
| “Living” media embeds | Animated SVG / CSS loops as GPU textures | `media` / atmosphere layers |
| Accessible canvas UIs | Drawn nodes *are* a11y tree (unlike pure canvas) | future whiteboard / board organisms |

---

## Cool components that *would* fit Liquid’s model

Keep the paste-first fence contract. New power should look like more **archetypes / props**, not “drop HTML into canvas.”

### A. Near-term (no HiC) — high ROI

These stay on the current DOM/SVG stack and ship everywhere Tauri runs:

| Idea | Sketch | Notes |
|------|--------|-------|
| **Motion tokens** | `motion: fade \| fade-up \| scale-in \| draw` on more organisms | Extend ADR-010 beyond slides; CSS only |
| **Chart draw-on** | path `stroke-dashoffset` reveal for line/area | Fits LayerCake SVG |
| **Compare / timeline scrub** | CSS scroll-driven or View Transitions on section change | Progressive enhancement |
| **Slides layer motion** | per-layer `enter:` / `parallax:` KV | Still declarative CSS `<img>` layers |
| **Ceremony polish** | pulse / whisper / prose View Transitions | Chat scene graph already exists |
| **Reduced-motion contracts** | document + enforce per archetype | Already partial |

### B. Mid-term (capability detect) — “Liquid FX” organism

Gate on `typeof CanvasRenderingContext2D.prototype.drawElementImage === "function"` (name may change):

````md
```fx
effect: glass | crt | paper | bloom
src: chart   # or inline nested fence body

```chart
type: area
…
```
````

Implementation sketch:

1. Hydrate normal Liquid child **as a canvas direct child** (or clone into canvas subtree).
2. On `paint`, `drawElementImage` + optional WebGL fragment pass.
3. Sync transform; forward pointer events via DOM hit-test.
4. If unsupported → render child only (no FX), same fence payload.

**Do not** put this in the default chat prose path until scroll cost is measured. Prefer: Present mode, vault preview “Focus”, `/dev/liquid` gallery.

### C. Ambition lane (post-stable HiC + WebGL)

| Component | Fence / surface | Pitch |
|-----------|-----------------|-------|
| **Stage** | ` ```stage ` or Present-only | Full-bleed WebGL room; Liquid cards as live textures on surfaces |
| **Deck 3D** | Slides player upgrade | Page-flip / depth with real DOM text (selectable notes) |
| **Board** | kanban / whiteboard organism | Pan-zoom canvas with HTML cards as textures (a11y-preserving) |
| **Export studio** | Settings / vault export | HiC capture path with DOM fallback |

These are product bets, not markdown niceties — schedule only after WebView2 ships the API and WebKit status is known.

---

## Alternatives matrix (what to use instead, for now)

| Approach | Quality | Perf | a11y | Tauri fit | Role vs Liquid |
|----------|---------|------|------|-----------|----------------|
| **CSS / View Transitions / SVG SMIL-ish** | High for UI | Excellent | Native | ✅ all platforms | **Default motion layer** |
| **LayerCake SVG + filters** | High for charts | Good | Good | ✅ | Keep charts here |
| **html2canvas / html2pdf** | Medium (quirks) | Slow | N/A (export) | ✅ already | Keep until HiC export |
| **foreignObject → canvas** | Fragile | Medium | Split | Partial | Avoid new work |
| **Konva / Fabric** | High for freeform | Medium | Weak | ✅ but heavy | Still deferred (ADR-010) |
| **Three.js CSS3D / Html overlays** | Cool demos | Costly | Overlay DOM | ✅ but bundle | Optional Present experiments |
| **HTML-in-Canvas** | Highest ceiling | TBD; JS paint | Strong (design goal) | ❌ production today | Watch + gated spike |

---

## Recommended stance

### Decision (proposed, not yet ADR)

1. **Do not** build production Liquid features on HTML-in-Canvas in 0.x.
2. **Do** invest in a **Liquid motion system** on CSS/SVG (tokens, chart draw-on, slides layer enters, View Transitions in chat ceremony).
3. **Do** keep a short **watchlist**: Chromium stable ship → WebView2 rollout → WebKit signals.
4. **Optional spike** (Windows + Edge Canary / Brave with flag, or a throwaway Vite page — not behind Home release flags): one `LiquidFxHost` prototype in `/dev/liquid` with feature detect + no-op fallback, to learn paint timing vs chat scroll.
5. **Revisit ADR-010** only if Present-mode 3D becomes a real product goal; HiC would then be the preferred texture path over screenshot hacks, still not a freeform editor.

### Suggested spike acceptance criteria (if we greenlight `/dev`)

- [ ] Feature-detect helper: `supportsHtmlInCanvas()`
- [ ] One card drawn via `drawElementImage` with a trivial shader or 2D warp
- [ ] Click / expand still works (transform sync)
- [ ] Fallback path identical to today’s Card
- [ ] Documented as unsupported on WKWebView / webkit2gtk
- [ ] No agent prompt / fence catalog changes until spike graduates

### Non-goals

- Shipping `effect:` fences to agents before cross-platform support
- Replacing the markdown hydrate pipeline with a canvas scene graph
- Bundling Chromium just to get Canary APIs inside Tauri

---

## Code anchors

| Path | Relevance |
|------|-----------|
| `apps/medousa-home/src/lib/markdown/liquidEmbeds.ts` | Fence → placeholder |
| `apps/medousa-home/src/lib/markdown/hydrateLiquidEmbeds.ts` | Mount lifecycle |
| `apps/medousa-home/src/lib/markdown/LiquidMdHost.svelte` | Enter / stagger motion |
| `apps/medousa-home/src/lib/liquid/archetypes/` | Component vocabulary |
| `apps/medousa-home/src/lib/components/vault/SlidesPlayer.svelte` | Present mode (best HiC candidate later) |
| `apps/medousa-home/src/lib/utils/vaultExportPrep.ts` | DOM export capture |
| `docs/architecture/decisions/adr-010-slides-player-and-layers.md` | Canvas editors deferred |
| `docs/cookbook/liquid-markdown.md` | Author-facing fence catalog |
| `apps/medousa-home/src/routes/dev/liquid/+page.svelte` | Gallery / spike home |

---

## References

- Spec overview: https://html-in-canvas.dev/docs/overview/
- API reference: https://html-in-canvas.dev/docs/api-reference/
- Chrome origin trial: https://developer.chrome.com/blog/html-in-canvas-origin-trial
- Tauri webview versions: https://v2.tauri.app/reference/webview-versions/
- WICG repo: https://github.com/WICG/html-in-canvas

---

## One-line verdict

**HTML-in-Canvas is the right long-term escape hatch for shader-y Present/export Liquid — but Medousa’s Tauri webview matrix means we superpower Liquid with CSS/SVG motion first, and treat HiC as a capability-gated experiment until WebView2 (and ideally WebKit) ship it for real.**
