/**
 * Expand a single accent RGB ("r g b") into a Skeleton-style 50–900 ramp.
 * Flat single-RGB accents made hover/active states feel dead.
 */
export function expandAccentRamp(
  baseRgb: string,
  opts: { towardWhite?: number; towardBlack?: number } = {},
): Record<string, string> {
  const parts = baseRgb.trim().split(/\s+/).map(Number);
  if (parts.length !== 3 || parts.some((n) => Number.isNaN(n))) {
    return flatRamp(baseRgb);
  }
  const [r, g, b] = parts;
  const tw = opts.towardWhite ?? 1;
  const tb = opts.towardBlack ?? 1;

  const mix = (t: number, toward: "white" | "black") => {
    const amount = toward === "white" ? t * tw : t * tb;
    const mixChannel = (c: number) =>
      toward === "white"
        ? Math.round(c + (255 - c) * amount)
        : Math.round(c * (1 - amount));
    return `${mixChannel(r)} ${mixChannel(g)} ${mixChannel(b)}`;
  };

  return {
    "50": mix(0.92, "white"),
    "100": mix(0.78, "white"),
    "200": mix(0.58, "white"),
    "300": mix(0.38, "white"),
    "400": mix(0.18, "white"),
    "500": baseRgb,
    "600": mix(0.18, "black"),
    "700": mix(0.32, "black"),
    "800": mix(0.48, "black"),
    "900": mix(0.62, "black"),
  };
}

function flatRamp(rgb: string): Record<string, string> {
  return Object.fromEntries(
    ["50", "100", "200", "300", "400", "500", "600", "700", "800", "900"].map(
      (step) => [step, rgb],
    ),
  );
}

export function accentRampProperties(
  family: "primary" | "secondary" | "tertiary",
  baseRgb: string,
): Record<string, string> {
  const ramp = expandAccentRamp(baseRgb);
  return Object.fromEntries(
    Object.entries(ramp).map(([step, value]) => [
      `--color-${family}-${step}`,
      value,
    ]),
  );
}
