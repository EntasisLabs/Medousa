import type { ContextMapNodeKind } from "$lib/utils/contextMap";

export interface MapKindVisual {
  kind: ContextMapNodeKind;
  label: string;
  shortLabel: string;
  hue: number;
  saturation: number;
  light: number;
  strokeLight: number;
  shape: "circle" | "rounded-square" | "diamond" | "hexagon";
  planned?: boolean;
}

/** Canonical palette — extend when M13b (claim) and M13c (note) land. */
export const MAP_KIND_VISUALS: Record<ContextMapNodeKind, MapKindVisual> = {
  session: {
    kind: "session",
    label: "Session",
    shortLabel: "Session",
    hue: 265,
    saturation: 72,
    light: 58,
    strokeLight: 82,
    shape: "circle",
  },
  thread: {
    kind: "thread",
    label: "Moment",
    shortLabel: "Moment",
    hue: 168,
    saturation: 64,
    light: 48,
    strokeLight: 72,
    shape: "rounded-square",
  },
  claim: {
    kind: "claim",
    label: "Memory",
    shortLabel: "Memory",
    hue: 32,
    saturation: 78,
    light: 56,
    strokeLight: 78,
    shape: "diamond",
    planned: true,
  },
  note: {
    kind: "note",
    label: "Note",
    shortLabel: "Note",
    hue: 210,
    saturation: 58,
    light: 62,
    strokeLight: 80,
    shape: "hexagon",
    planned: true,
  },
};

export const MAP_KIND_LEGEND: MapKindVisual[] = [
  MAP_KIND_VISUALS.session,
  MAP_KIND_VISUALS.thread,
  MAP_KIND_VISUALS.claim,
  MAP_KIND_VISUALS.note,
];

export function mapNodeStyleVars(
  kind: ContextMapNodeKind,
  sessionAccent = 0,
): string {
  if (kind !== "session") return "";
  return `--map-accent:${sessionAccent % 8}`;
}

export function mapKindLabel(kind: ContextMapNodeKind): string {
  return MAP_KIND_VISUALS[kind]?.label ?? kind;
}

export type MapLabelMode = "hidden" | "whisper" | "neighbor" | "full";

export function truncateMapLabel(text: string, max = 14): string {
  const trimmed = text.trim();
  if (trimmed.length <= max) return trimmed;
  return `${trimmed.slice(0, max - 1)}…`;
}

/** Strip trailing moment-count suffix (`Name · 4`) for calm ambient labels. */
export function mapLabelBase(label: string): string {
  const match = label.match(/^(.+?) · (\d+)$/);
  return match ? match[1].trim() : label.trim();
}

export function mapDisplayLabel(
  label: string,
  mode: MapLabelMode,
  kind: ContextMapNodeKind,
): string {
  if (mode === "hidden") return "";
  if (mode === "full") return label;
  const base = mapLabelBase(label);
  if (mode === "neighbor") {
    return truncateMapLabel(base, kind === "session" ? 22 : 16);
  }
  return truncateMapLabel(base, 13);
}

export function resolveMapLabelMode(options: {
  selected: boolean;
  hovered: boolean;
  ghost: boolean;
  inNeighborhood: boolean;
  focusActive: boolean;
  kind: ContextMapNodeKind;
}): MapLabelMode {
  const { selected, hovered, ghost, inNeighborhood, focusActive, kind } = options;
  if (selected || hovered) return "full";
  if (ghost) return "hidden";
  if (focusActive && inNeighborhood) return "neighbor";
  if (kind === "session") return "whisper";
  return "hidden";
}
