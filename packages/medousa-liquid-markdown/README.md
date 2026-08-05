# @medousa/liquid-markdown

Dependency-free Liquid Markdown grammar, payload types, and inert placeholder
encoding shared by Medousa's first-party rendering surfaces.

The package intentionally does not own a Markdown engine, UI framework, host
navigation, image resolution, or network access. Browser rendering is exposed
separately so hosts can supply those capabilities.

## Entry points

- `@medousa/liquid-markdown` parses Liquid fences and icon shortcodes into inert,
  encoded placeholders. It is safe to use in Node and other non-DOM runtimes.
- `@medousa/liquid-markdown/browser` renders and hydrates those placeholders with
  framework-independent DOM, SVG charts, interactions, and host-theme-aware CSS.

Browser hosts call `hydrateLiquidMarkdown(container, options)` after their normal
Markdown pass. The options provide host-owned behavior such as nested Markdown
rendering, link navigation, action handling, media URL resolution, clipboard
access, and live-feed loading. The returned handle exposes `ready` and `destroy()`
for async rendering and lifecycle cleanup.

Portable hosts should start with `preprocessPortableLiquidEmbeds(markdown)`. It
keeps the shared chart payload while omitting Home's chart-editing toolbar.
