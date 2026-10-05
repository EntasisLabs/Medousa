# Medousa Mascot App Assets v0

Generated from the supplied Medousa / Seahorse / Starfish character sheet.

## Included
- 18 clean transparent sprites: 3 bodies × 6 states
- Clean PNGs at 64, 128, 192, and 256 px
- FX PNGs with the detached pixel accents preserved
- Xcode `.xcassets` catalog using 1x / 2x / 3x PNGs
- 6×3 transparent 256 px sprite atlas + JSON frame map
- `manifest.json`
- `preview.png`

## Naming
Bodies: `medousa`, `seahorse`, `starfish`
States: `default`, `happy`, `chill`, `sweet`, `focus`, `sus`

## Pixel-art rendering
Use nearest-neighbor / pixelated rendering in any runtime that rescales the images.
For CSS, `image-rendering: pixelated;` is a good default.

## Note
This is an extraction pass from the current JPEG character sheet. For production-final art,
the ideal next pass is to redraw/export the masters directly on a fixed pixel grid rather
than deriving them from a compressed poster image.
