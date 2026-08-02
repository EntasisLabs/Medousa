# Medousa animation sprites

These assets turn the full, detailed Medousa product mark into a small action
library. Each frame is the same logo in a different pose: the bell compresses,
expands, recoils, or floats while the tentacles follow with their own motion.
The simplified atlas remains available only for accessibility and tiny-size
contexts.

## Files

- `medousa-mark-action-sprite.svg` — the primary six-frame-per-action atlas for
  the detailed product mark. Rows are `idle`, `jump`, `hit`, `power-up`, and
  `float`; every frame is `352 × 560` with a 48px horizontal / 40px vertical
  safety border. The float row is center-anchored so it pulses in place instead
  of rolling across the frame.
- `medousa-mark-action-sprite.json` — action names, frame ids, dimensions, and
  the independently animated logo parts for canvas/game-engine integrations.
- `medousa-mark-detail-sprite.svg` — the earlier subtle-breathe detail atlas;
  useful when only a restrained idle motion is wanted.
- `medousa-mark-sprite.svg` — the one-row, eight-frame simplified atlas for
  accessibility and tiny-size usage only. Each frame is `128 × 160`; use
  `steps(7, end)` when animating its eight columns with percentage masks.
- `medousa-mark-sprite.json` and `medousa-mark-detail-sprite.json` — metadata
  for the two legacy/simple atlases.
- `../companion/medousa-companion.json` — semantic companion states mapped to
  the detailed action rows. The companion is the same Medousa mark at a smaller
  scale; it does not introduce a separate character silhouette.
- `../../../src/lib/components/brand/MedousaSprite.svelte` — Svelte wrapper
  that uses the detailed action atlas by default and applies approved Medousa
  colors or gradients through a CSS mask.

## Svelte

```svelte
<script lang="ts">
  import MedousaSprite from "$lib/components/brand/MedousaSprite.svelte";
</script>

<MedousaSprite variant="aurora" action="float" size="8rem" fps={8} />
<MedousaSprite action="jump" loop={false} label="Medousa jumps" />
```

Available detailed actions are `idle`, `jump`, `hit`, `power-up`, and `float`.
Set `simplified={true}` only for accessibility/tiny-size usage. Set
`label={null}` for a decorative sprite, or `paused={true}` when it should hold
on its current frame. The component respects `prefers-reduced-motion`.
