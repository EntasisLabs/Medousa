import type { CustomThemeConfig } from "@skeletonlabs/tw-plugin";
import { expandAccentRamp } from "./accent-ramps";

type ThemeProperties = CustomThemeConfig["properties"];

export type ThemeRgb = string;

export interface ThemePersonality {
  roles?: Partial<{
    canvas: ThemeRgb;
    chrome: ThemeRgb;
    header: ThemeRgb;
    pane: ThemeRgb;
    paneMuted: ThemeRgb;
    card: ThemeRgb;
    cardHover: ThemeRgb;
    border: ThemeRgb;
    text: ThemeRgb;
    textSecondary: ThemeRgb;
    textTertiary: ThemeRgb;
    textQuiet: ThemeRgb;
    textFaint: ThemeRgb;
    /** @deprecated Use `textTertiary`. */
    textMuted: ThemeRgb;
    placeholder: ThemeRgb;
    disabled: ThemeRgb;
    action: ThemeRgb;
    actionHover: ThemeRgb;
    focus: ThemeRgb;
    selection: ThemeRgb;
    selectionText: ThemeRgb;
    decorative: ThemeRgb;
    link: ThemeRgb;
    error: ThemeRgb;
    success: ThemeRgb;
    warning: ThemeRgb;
  }>;
  syntax?: Partial<{
    background: ThemeRgb;
    border: ThemeRgb;
    foreground: ThemeRgb;
    comment: ThemeRgb;
    keyword: ThemeRgb;
    string: ThemeRgb;
    number: ThemeRgb;
    function: ThemeRgb;
    type: ThemeRgb;
    attribute: ThemeRgb;
    operator: ThemeRgb;
  }>;
  charts?: Partial<{
    one: ThemeRgb;
    two: ThemeRgb;
    three: ThemeRgb;
    four: ThemeRgb;
    five: ThemeRgb;
  }>;
  effects?: Partial<{
    shadow: ThemeRgb;
    glow: ThemeRgb;
    gradientA: ThemeRgb;
    gradientB: ThemeRgb;
    gradientC: ThemeRgb;
    glowStrength: string;
    chromeAlpha: string;
    paneAlpha: string;
  }>;
  shape?: Partial<{
    controlRadius: string;
    containerRadius: string;
  }>;
}

const STEPS = ["50", "100", "200", "300", "400", "500", "600", "700", "800", "900"] as const;

function value(properties: ThemeProperties, key: string, fallback: ThemeRgb): ThemeRgb {
  return String((properties as Record<string, string>)[key] ?? fallback);
}

function missingRamp(family: "error" | "success" | "warning", base: ThemeRgb) {
  const ramp = expandAccentRamp(base);
  return Object.fromEntries(
    STEPS.map((step) => [`--color-${family}-${step}`, ramp[step]]),
  ) as Record<string, string>;
}

function colorRamp(family: string, base: ThemeRgb) {
  const ramp = expandAccentRamp(base);
  return Object.fromEntries(
    STEPS.map((step) => [`--color-${family}-${step}`, ramp[step]]),
  ) as Record<string, string>;
}

function relativeLuminance(color: ThemeRgb): number {
  const channels = color.split(/\s+/).map(Number);
  if (channels.length !== 3 || channels.some((channel) => !Number.isFinite(channel))) return 0;
  const linear = channels.map((channel) => {
    const normalized = channel / 255;
    return normalized <= 0.04045
      ? normalized / 12.92
      : ((normalized + 0.055) / 1.055) ** 2.4;
  });
  return linear[0]! * 0.2126 + linear[1]! * 0.7152 + linear[2]! * 0.0722;
}

function contrastRatio(a: ThemeRgb, b: ThemeRgb): number {
  const lighter = Math.max(relativeLuminance(a), relativeLuminance(b));
  const darker = Math.min(relativeLuminance(a), relativeLuminance(b));
  return (lighter + 0.05) / (darker + 0.05);
}

function accessibleForeground(background: ThemeRgb, preferred: ThemeRgb): ThemeRgb {
  if (contrastRatio(background, preferred) >= 4.5) return preferred;
  const ink = "0 0 0";
  const paper = "255 255 255";
  return contrastRatio(background, ink) >= contrastRatio(background, paper) ? ink : paper;
}

function minimumContrast(foreground: ThemeRgb, backgrounds: ThemeRgb[]): number {
  return Math.min(...backgrounds.map((background) => contrastRatio(foreground, background)));
}

function mixColor(from: ThemeRgb, toward: ThemeRgb, amount: number): ThemeRgb {
  const start = from.split(/\s+/).map(Number);
  const end = toward.split(/\s+/).map(Number);
  const finite = (channels: number[]) =>
    channels.length === 3 && channels.every((channel) => Number.isFinite(channel));
  if (!finite(start) || !finite(end)) return from;
  return start
    .map((channel, index) => Math.round(channel + (end[index]! - channel) * amount))
    .join(" ");
}

/**
 * Contrast the subdued tiers aim for, measured against the least forgiving
 * shell surface. Light ramps mirror their dark counterparts, which works for
 * backgrounds but not for text: sRGB luminance is not symmetric about
 * mid-gray, so a mirrored step lands far closer to paper than to ink. Deriving
 * against these targets restores the runway the dark ramps get for free.
 */
const TEXT_TIER_TARGETS = {
  secondary: 7.5,
  tertiary: 5.5,
  quiet: 4.5,
  faint: 3.2,
} as const;

/**
 * Fade `ink` toward `toward` until the result still clears `target` against
 * every background. Mixing from the theme's own ink keeps each palette's cast
 * intact — Solarized stays teal, Ember stays warm, Black Lily stays purple.
 */
function deriveTextTier(
  ink: ThemeRgb,
  toward: ThemeRgb,
  backgrounds: ThemeRgb[],
  target: number,
): ThemeRgb {
  if (minimumContrast(ink, backgrounds) < target) return ink;
  let low = 0;
  let high = 1;
  let best = ink;
  for (let step = 0; step < 24; step += 1) {
    const middle = (low + high) / 2;
    const candidate = mixColor(ink, toward, middle);
    if (minimumContrast(candidate, backgrounds) >= target) {
      best = candidate;
      low = middle;
    } else {
      high = middle;
    }
  }
  return best;
}

/**
 * Build the four subdued tiers for a light theme. When the palette's ink
 * cannot reach the top target — Catppuccin Latte only manages 6.9:1 — every
 * target is scaled by the same factor so the ladder compresses instead of
 * collapsing onto the body text colour.
 */
function deriveTextLadder(ink: ThemeRgb, toward: ThemeRgb, backgrounds: ThemeRgb[]) {
  const available = minimumContrast(ink, backgrounds);
  const scale = available < TEXT_TIER_TARGETS.secondary
    ? available / TEXT_TIER_TARGETS.secondary
    : 1;
  const tier = (target: number) => deriveTextTier(ink, toward, backgrounds, target * scale);
  return {
    secondary: tier(TEXT_TIER_TARGETS.secondary),
    tertiary: tier(TEXT_TIER_TARGETS.tertiary),
    quiet: tier(TEXT_TIER_TARGETS.quiet),
    faint: tier(TEXT_TIER_TARGETS.faint),
  };
}

/**
 * Resolve a foreground that stays readable against the surfaces it is checked
 * on. Palette stops are tried in visual-preference order.
 *
 * Only roles that carry meaning are checked: primary and secondary body text
 * against every shell surface, status/link/focus against the canvas. On dark
 * ramps the subdued tiers pass through unchanged — forcing them to a body-text
 * ratio collapses the hierarchy they exist to express. Light ramps have no
 * comparable runway, so `deriveTextLadder` builds those tiers instead.
 */
function readableForeground(
  backgrounds: ThemeRgb[],
  candidates: Array<ThemeRgb | undefined>,
  minimum = 4.5,
): ThemeRgb {
  const unique = candidates.filter(
    (candidate, index, values): candidate is ThemeRgb =>
      Boolean(candidate) && values.indexOf(candidate) === index,
  );
  const readable = unique.find(
    (candidate) => minimumContrast(candidate, backgrounds) >= minimum,
  );
  if (readable) return readable;

  const universal = ["0 0 0", "255 255 255"];
  return universal.sort(
    (a, b) => minimumContrast(b, backgrounds) - minimumContrast(a, backgrounds),
  )[0]!;
}

/**
 * Complete a Skeleton theme with Medousa's semantic contract.
 *
 * Skeleton's color ramps remain the compatibility layer. Product components
 * consume the `--theme-*`, `--syn-*`, and `--chart-*` roles emitted here so a
 * theme can grow without component-specific selectors.
 */
export function completeThemeConfig(
  theme: CustomThemeConfig,
  personality: ThemePersonality = {},
): CustomThemeConfig {
  const original = theme.properties;
  const accentDefaults = {
    ...colorRamp("primary", value(original, "--color-primary-500", "124 58 237")),
    ...colorRamp("secondary", value(original, "--color-secondary-500", "99 102 241")),
    ...colorRamp("tertiary", value(original, "--color-tertiary-500", "167 139 250")),
    ...original,
  } as ThemeProperties;
  const primary = (step: string) =>
    value(accentDefaults, `--color-primary-${step}`, "124 58 237");
  const secondary = (step: string) =>
    value(accentDefaults, `--color-secondary-${step}`, primary(step));
  const tertiary = (step: string) =>
    value(accentDefaults, `--color-tertiary-${step}`, secondary(step));
  const surface = (step: string) => value(accentDefaults, `--color-surface-${step}`, "0 0 0");
  const roles = personality.roles ?? {};
  const syntax = personality.syntax ?? {};
  const charts = personality.charts ?? {};
  const effects = personality.effects ?? {};
  const shape = personality.shape ?? {};

  const errorRamp = missingRamp(
    "error",
    value(accentDefaults, "--color-error-500", "255 59 48"),
  );
  const successRamp = missingRamp(
    "success",
    value(accentDefaults, "--color-success-500", "52 199 89"),
  );
  const warningRamp = missingRamp(
    "warning",
    value(accentDefaults, "--color-warning-500", "255 149 0"),
  );
  const completed = {
    ...errorRamp,
    ...successRamp,
    ...warningRamp,
    ...accentDefaults,
  } as ThemeProperties;
  const status = (family: "error" | "success" | "warning", step: string) =>
    value(completed, `--color-${family}-${step}`, "255 255 255");
  const action = roles.action ?? primary("500");
  const canvas = roles.canvas ?? surface("950");
  const chrome = roles.chrome ?? surface("900");
  const header = roles.header ?? surface("800");
  const pane = roles.pane ?? surface("900");
  const paneMuted = roles.paneMuted ?? surface("800");
  const card = roles.card ?? surface("900");
  const cardHover = roles.cardHover ?? surface("800");
  const textBackgrounds = [canvas, chrome, header, pane, paneMuted, card, cardHover];
  const text = readableForeground(textBackgrounds, [
    roles.text,
    surface("50"),
    surface("100"),
    surface("200"),
  ]);
  const isLight = relativeLuminance(canvas) > 0.5;
  const darkestBackground = textBackgrounds.reduce((darkest, background) =>
    relativeLuminance(background) < relativeLuminance(darkest) ? background : darkest,
  );
  /* Dark ramps already fan out across surface 300–600; light ramps do not. */
  const ladder = isLight ? deriveTextLadder(text, darkestBackground, textBackgrounds) : null;
  const textSecondary = ladder
    ? roles.textSecondary ?? ladder.secondary
    : readableForeground(textBackgrounds, [
      roles.textSecondary,
      surface("300"),
      surface("200"),
      surface("100"),
      surface("50"),
      text,
    ]);
  const textTertiary = roles.textTertiary ?? roles.textMuted ?? ladder?.tertiary ?? surface("400");
  const textQuiet = roles.textQuiet ?? ladder?.quiet ?? surface("500");
  const textFaint = roles.textFaint ?? ladder?.faint ?? surface("600");
  const placeholder = roles.placeholder ?? textTertiary;
  const disabled = readableForeground(
    [canvas],
    [roles.disabled, surface("400"), surface("300"), surface("200"), textTertiary],
    3,
  );
  const link = readableForeground([canvas], [
    roles.link,
    primary("300"),
    primary("400"),
    primary("500"),
    primary("600"),
    primary("700"),
    primary("200"),
    primary("800"),
    text,
  ]);
  const focus = readableForeground(
    [canvas],
    [
      roles.focus,
      secondary("400"),
      secondary("500"),
      secondary("600"),
      secondary("300"),
      secondary("700"),
      text,
    ],
    3,
  );
  const selection = roles.selection ?? primary("500");
  const statusForeground = (
    family: "error" | "success" | "warning",
    preferred: ThemeRgb | undefined,
  ) =>
    readableForeground([canvas], [
      preferred,
      status(family, "400"),
      status(family, "500"),
      status(family, "600"),
      status(family, "700"),
      status(family, "300"),
      status(family, "800"),
      status(family, "200"),
      status(family, "900"),
      text,
    ]);

  const synBg = syntax.background ?? surface("900");
  /*
   * Accent step 300 is the seed lightened 38% toward white — vivid on dark
   * paper, washed out on light. Which darker step first clears 4.5:1 depends
   * on hue (violet and rose at 500, amber and teal at 600, green and orange
   * only at 700), so walk the ramp instead of swapping to a fixed step.
   */
  const synToken = (
    preferred: ThemeRgb | undefined,
    ramp: (step: string) => ThemeRgb,
    darkStep: string,
  ): ThemeRgb =>
    isLight
      ? readableForeground(
        [synBg],
        [preferred, ramp("500"), ramp("600"), ramp("700"), ramp("800")],
        4.5,
      )
      : preferred ?? ramp(darkStep);
  const warningRampStep = (step: string) => status("warning", step);
  const successRampStep = (step: string) => status("success", step);
  const errorRampStep = (step: string) => status("error", step);

  const semantic = {
    "--theme-canvas": canvas,
    "--theme-chrome": chrome,
    "--theme-header": header,
    "--theme-pane": pane,
    "--theme-pane-muted": paneMuted,
    "--theme-card": card,
    "--theme-card-hover": cardHover,
    "--theme-border": roles.border ?? surface("500"),
    "--theme-text": text,
    "--theme-text-secondary": textSecondary,
    "--theme-text-tertiary": textTertiary,
    "--theme-text-quiet": textQuiet,
    "--theme-text-faint": textFaint,
    /* Deprecated alias: resolved to the secondary stop before the tier split. */
    "--theme-text-muted": textSecondary,
    "--theme-placeholder": placeholder,
    "--theme-text-disabled": disabled,
    "--theme-action": action,
    "--theme-action-hover": roles.actionHover ?? primary("400"),
    "--theme-focus": focus,
    "--theme-selection": selection,
    "--theme-selection-text": accessibleForeground(
      selection,
      roles.selectionText ?? surface("50"),
    ),
    "--theme-decorative": roles.decorative ?? tertiary("400"),
    "--theme-link": link,
    "--theme-error": statusForeground("error", roles.error),
    "--theme-success": statusForeground("success", roles.success),
    "--theme-warning": statusForeground("warning", roles.warning),
    "--on-primary": accessibleForeground(
      action,
      value(completed, "--on-primary", "255 255 255"),
    ),
    "--on-secondary": accessibleForeground(
      secondary("500"),
      value(completed, "--on-secondary", "255 255 255"),
    ),
    "--on-tertiary": accessibleForeground(
      tertiary("500"),
      value(completed, "--on-tertiary", surface("950")),
    ),
    "--on-error": accessibleForeground(
      status("error", "500"),
      value(completed, "--on-error", "255 255 255"),
    ),
    "--on-success": accessibleForeground(
      status("success", "500"),
      value(completed, "--on-success", "255 255 255"),
    ),
    "--on-warning": accessibleForeground(
      status("warning", "500"),
      value(completed, "--on-warning", "0 0 0"),
    ),
    "--theme-shadow": effects.shadow ?? surface("950"),
    "--theme-glow": effects.glow ?? primary("500"),
    "--theme-gradient-a": effects.gradientA ?? primary("500"),
    "--theme-gradient-b": effects.gradientB ?? secondary("600"),
    "--theme-gradient-c": effects.gradientC ?? tertiary("500"),
    "--theme-glow-strength": effects.glowStrength ?? "0.14",
    "--theme-chrome-alpha": effects.chromeAlpha ?? "0.96",
    "--theme-pane-alpha": effects.paneAlpha ?? "0.82",
    "--theme-control-radius": shape.controlRadius ?? "0.6rem",
    "--theme-container-radius": shape.containerRadius ?? "0.75rem",

    "--syn-bg": synBg,
    "--syn-border": syntax.border ?? surface("600"),
    "--syn-header-bg": syntax.background ?? surface("800"),
    "--syn-fg": syntax.foreground ?? surface("100"),
    "--syn-comment": syntax.comment ?? (isLight ? textQuiet : surface("400")),
    "--syn-keyword": synToken(syntax.keyword, secondary, "300"),
    "--syn-string": synToken(syntax.string, tertiary, "300"),
    "--syn-number": synToken(syntax.number, warningRampStep, "300"),
    "--syn-function": synToken(syntax.function, primary, "300"),
    "--syn-type": synToken(syntax.type, tertiary, "200"),
    "--syn-attr": synToken(syntax.attribute, secondary, "200"),
    "--syn-operator": syntax.operator ?? (isLight ? textTertiary : surface("300")),
    "--syn-meta": syntax.comment ?? (isLight ? textQuiet : surface("400")),
    "--syn-punctuation": syntax.operator ?? (isLight ? textTertiary : surface("300")),
    "--syn-title": synToken(syntax.type, tertiary, "200"),
    "--syn-addition-fg": synToken(undefined, successRampStep, "300"),
    "--syn-addition-bg": status("success", "500"),
    "--syn-deletion-fg": synToken(undefined, errorRampStep, "300"),
    "--syn-deletion-bg": status("error", "500"),
    "--md-code-bg": "var(--syn-bg)",
    "--md-code-border": "var(--syn-border)",
    "--md-code-header-bg": "var(--syn-header-bg)",
    "--md-code-fg": "var(--syn-fg)",
    "--md-code-comment": "var(--syn-comment)",
    "--md-code-keyword": "var(--syn-keyword)",
    "--md-code-string": "var(--syn-string)",
    "--md-code-number": "var(--syn-number)",
    "--md-code-title": "var(--syn-title)",
    "--md-code-attr": "var(--syn-attr)",
    "--md-code-function": "var(--syn-function)",
    "--md-code-meta": "var(--syn-meta)",
    "--md-code-addition-fg": "var(--syn-addition-fg)",
    "--md-code-addition-bg": "var(--syn-addition-bg)",
    "--md-code-deletion-fg": "var(--syn-deletion-fg)",
    "--md-code-deletion-bg": "var(--syn-deletion-bg)",

    "--chart-1": charts.one ?? primary("400"),
    "--chart-2": charts.two ?? secondary("400"),
    "--chart-3": charts.three ?? tertiary("400"),
    "--chart-4": charts.four ?? status("warning", "400"),
    "--chart-5": charts.five ?? status("error", "400"),
    "--chart-fg": surface("50"),
    "--chart-fg-secondary": surface("200"),
    "--chart-fg-muted": surface("400"),
    "--chart-fg-subtle": surface("500"),
    "--chart-plot-ink": surface("50"),
    "--chart-grid-rgb": surface("500"),
    "--chart-plot": `color-mix(in srgb, rgb(${surface("50")}) 7%, transparent)`,
    "--chart-plot-muted": `color-mix(in srgb, rgb(${surface("50")}) 14%, transparent)`,
    "--chart-grid": `color-mix(in srgb, rgb(${surface("500")}) 28%, transparent)`,
    "--markdown-chart-red": status("error", "400"),
    "--markdown-chart-orange": status("warning", "400"),
    "--markdown-chart-yellow": status("warning", "300"),
    "--markdown-chart-green": charts.three ?? tertiary("400"),
    "--markdown-chart-blue": charts.one ?? primary("400"),
    "--markdown-chart-purple": charts.two ?? secondary("400"),
    "--markdown-chart-pink": charts.five ?? status("error", "400"),
    "--markdown-chart-gray": surface("400"),
  } as Record<string, string>;

  return {
    ...theme,
    properties: {
      ...completed,
      ...semantic,
    } as ThemeProperties,
  };
}

export const REQUIRED_THEME_PROPERTIES = [
  ...["primary", "secondary", "tertiary"].flatMap((family) =>
    STEPS.map((step) => `--color-${family}-${step}`),
  ),
  ...["error", "success", "warning"].flatMap((family) =>
    STEPS.map((step) => `--color-${family}-${step}`),
  ),
  ...["50", "100", "200", "300", "400", "500", "600", "700", "800", "900", "950"].map(
    (step) => `--color-surface-${step}`,
  ),
  "--theme-canvas",
  "--theme-chrome",
  "--theme-pane",
  "--theme-card",
  "--theme-border",
  "--theme-text",
  "--theme-text-secondary",
  "--theme-text-tertiary",
  "--theme-text-quiet",
  "--theme-text-faint",
  "--theme-placeholder",
  "--theme-text-disabled",
  "--theme-link",
  "--theme-error",
  "--theme-success",
  "--theme-warning",
  "--theme-action",
  "--theme-focus",
  "--theme-selection",
  "--theme-decorative",
  "--theme-gradient-a",
  "--theme-gradient-b",
  "--theme-gradient-c",
  "--syn-bg",
  "--syn-keyword",
  "--syn-string",
  "--chart-1",
  "--chart-5",
] as const;

export function validateThemeConfig(theme: CustomThemeConfig): string[] {
  const properties = theme.properties as Record<string, string>;
  return REQUIRED_THEME_PROPERTIES.filter((key) => !properties[key]);
}
