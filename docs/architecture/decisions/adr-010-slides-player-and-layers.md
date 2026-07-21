# ADR-010: Slides player + declarative CSS layers

## Status

Accepted

## Context

Vault slides already have a deck grammar and Liquid organism, but no fullscreen player, positioned graphics, or entrance motion. Freeform canvas editors (Konva/Fabric) are heavier than we need for 0.4.0.

## Decision

1. **Fullscreen player** from the deck editor (`Present`) with ←/→/Esc, progress, and speaker `notes:`.
2. **Declarative layers:** nested KV under `layer: id` with `src`, `x`, `y`, `w`, optional `h` in normalized 0–1 stage coords; rendered as absolutely positioned CSS `<img>` children.
3. **Motion v1:** per-slide `motion: none | fade | fade-up` on slide change.
4. **Defer** Konva/Fabric drag-edit and PPTX export.

## Consequences

- Authoring is numeric KV (or Write mode), not a canvas.
- Layers resolve vault-relative `src` like slide backgrounds.
- Export path should eventually include layers (follow-up if print CSS omits them).

## Code anchors

- `apps/medousa-home/src/lib/utils/markdownSlides.ts`
- `apps/medousa-home/src/lib/components/vault/SlidesPlayer.svelte`
- `apps/medousa-home/src/lib/liquid/archetypes/organisms/slides/Slides.svelte`
