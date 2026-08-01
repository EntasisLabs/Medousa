# Medousa theme system

Themes are complete product environments, not accent swaps. Every shipped or
future user-authored theme enters through the same contract in
`theme-contract.ts`.

## Layers

1. **Skeleton compatibility** — complete surface, primary, secondary,
   tertiary, error, success, and warning ramps.
2. **Product roles** — canvas, chrome, panes, cards, borders, text, actions,
   focus, selection, links, and decoration.
3. **Content roles** — syntax highlighting, markdown code, charts, and data
   visualization.
4. **Material and shape** — shadows, glow, gradients, translucency, and radii.

Components consume semantic `--theme-*`, `--syn-*`, and `--chart-*` variables.
They must not add selectors for an individual palette.

## Adding a theme

Use `buildTheme` or `buildDarkTheme` from `theme-utils.ts`, then register the
result in `theme-catalog.ts`. A minimal theme provides:

- an ordered 50–950 surface ramp;
- primary and secondary RGB anchors (tertiary may fall back to secondary);
- readable surface text colors;
- optional `ThemePersonality` role, syntax, chart, effect, and shape overrides.

The builder generates complete accent and status ramps, accessible foregrounds,
semantic roles, syntax colors, and chart colors. Authors can use
`tintSurfaceScale` when starting from a brand color, or provide a hand-tuned
surface ramp for full art direction.

Run `npx vitest run themes/theme-contract.test.ts` after adding a theme. The
contract test rejects missing variables, duplicate names, unreadable text or
actions, and duplicate Medousa foundations.

## Rules

- Give secondary and tertiary colors explicit semantic jobs; do not merely swap
  them and call the result a new theme.
- Keep semantic status meaning stable even when the surrounding palette changes.
- Prefer role overrides in theme data over component-specific CSS.
- Validate dark and light variants independently.
- Preview shell, controls, prose, code, and charts—not only color swatches.
