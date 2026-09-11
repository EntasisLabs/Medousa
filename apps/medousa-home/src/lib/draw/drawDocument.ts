import { randomUuid } from "$lib/utils/randomUuid";

export const DRAW_SCHEMA = "medousa-draw" as const;
export const DRAW_VERSION = 2 as const;
export const DRAW_WIDTH = 1200;
export const DRAW_HEIGHT = 720;

export const MAX_DRAW_PAYLOAD_BYTES = 2 * 1024 * 1024;
const PAYLOAD_LINE_WIDTH = 96;

export type DrawInputKind = "pen" | "touch" | "mouse" | "unknown";
export type DrawBrushKind = "pen" | "pencil" | "marker" | "highlighter";

export type DrawPoint = {
  x: number;
  y: number;
  pressure?: number;
  elapsedMs?: number;
  tiltX?: number;
  tiltY?: number;
};

export type DrawBrush = {
  kind: DrawBrushKind;
  size: number;
  thinning: number;
  smoothing: number;
  streamline: number;
  opacity: number;
};

export type DrawStroke = {
  id: string;
  color: string;
  input: DrawInputKind;
  brush: DrawBrush;
  points: DrawPoint[];
};

export type DrawDocument = {
  schema: typeof DRAW_SCHEMA;
  version: typeof DRAW_VERSION;
  width: number;
  height: number;
  background: "transparent" | string;
  strokes: DrawStroke[];
};

export type ParsedDrawFence = {
  start: number;
  end: number;
  raw: string;
  body: string;
  document: DrawDocument;
};

const BRUSH_DEFAULTS: Record<DrawBrushKind, Omit<DrawBrush, "kind" | "size"> & { size: number }> = {
  pen: { size: 6, thinning: 0.62, smoothing: 0.55, streamline: 0.5, opacity: 1 },
  pencil: { size: 5, thinning: 0.48, smoothing: 0.42, streamline: 0.38, opacity: 0.82 },
  marker: { size: 14, thinning: 0.24, smoothing: 0.62, streamline: 0.55, opacity: 0.92 },
  highlighter: { size: 24, thinning: 0.08, smoothing: 0.7, streamline: 0.58, opacity: 0.3 },
};

export function createDrawBrush(kind: DrawBrushKind = "pen", size?: number): DrawBrush {
  const defaults = BRUSH_DEFAULTS[kind];
  return { kind, ...defaults, size: size ?? defaults.size };
}

export function createEmptyDrawDocument(): DrawDocument {
  return {
    schema: DRAW_SCHEMA,
    version: DRAW_VERSION,
    width: DRAW_WIDTH,
    height: DRAW_HEIGHT,
    background: "transparent",
    strokes: [],
  };
}

export function cloneDrawStroke(stroke: DrawStroke): DrawStroke {
  return {
    ...stroke,
    brush: { ...stroke.brush },
    points: stroke.points.map((point) => ({ ...point })),
  };
}

export function cloneDrawDocument(document: DrawDocument): DrawDocument {
  return {
    ...document,
    strokes: document.strokes.map(cloneDrawStroke),
  };
}

function finite(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function normalizeBrush(value: unknown, legacyWidth?: unknown, migratingLegacy = false): DrawBrush {
  if (migratingLegacy) {
    return {
      kind: "pen",
      size: clamp(finite(legacyWidth, 4), 1, 80),
      thinning: 0,
      smoothing: 0,
      streamline: 0,
      opacity: 1,
    };
  }
  const input = value && typeof value === "object" ? (value as Record<string, unknown>) : {};
  const kind: DrawBrushKind =
    input.kind === "pencil" || input.kind === "marker" || input.kind === "highlighter"
      ? input.kind
      : "pen";
  const defaults = createDrawBrush(
    kind,
    clamp(finite(input.size ?? legacyWidth, BRUSH_DEFAULTS[kind].size), 0.25, 160),
  );
  return {
    kind,
    size: defaults.size,
    thinning: clamp(finite(input.thinning, defaults.thinning), 0, 1),
    smoothing: clamp(finite(input.smoothing, defaults.smoothing), 0, 1),
    streamline: clamp(finite(input.streamline, defaults.streamline), 0, 1),
    opacity: clamp(finite(input.opacity, defaults.opacity), 0.05, 1),
  };
}

function normalizePoint(value: unknown, width: number, height: number): DrawPoint | null {
  if (!value || typeof value !== "object") return null;
  const point = value as Record<string, unknown>;
  return {
    x: Math.round(clamp(finite(point.x, 0), -width, width * 2) * 100) / 100,
    y: Math.round(clamp(finite(point.y, 0), -height, height * 2) * 100) / 100,
    ...(typeof point.pressure === "number"
      ? { pressure: Math.round(clamp(finite(point.pressure, 0.5), 0, 1) * 1000) / 1000 }
      : {}),
    ...(typeof point.elapsedMs === "number"
      ? { elapsedMs: Math.round(clamp(finite(point.elapsedMs, 0), 0, 86_400_000)) }
      : {}),
    ...(typeof point.tiltX === "number"
      ? { tiltX: Math.round(clamp(finite(point.tiltX, 0), -90, 90) * 10) / 10 }
      : {}),
    ...(typeof point.tiltY === "number"
      ? { tiltY: Math.round(clamp(finite(point.tiltY, 0), -90, 90) * 10) / 10 }
      : {}),
  };
}

function normalizeDocument(value: unknown): DrawDocument {
  if (!value || typeof value !== "object") throw new Error("Invalid drawing payload");
  const input = value as Record<string, unknown>;
  const version = finite(input.version, 0);
  if (input.schema !== DRAW_SCHEMA || (version !== 1 && version !== DRAW_VERSION)) {
    throw new Error("Unsupported drawing version");
  }

  const width = clamp(finite(input.width, DRAW_WIDTH), 100, 10_000);
  const height = clamp(finite(input.height, DRAW_HEIGHT), 100, 10_000);
  const rawStrokes = Array.isArray(input.strokes) ? input.strokes : [];
  const strokes: DrawStroke[] = [];

  for (const raw of rawStrokes) {
    if (!raw || typeof raw !== "object") continue;
    const stroke = raw as Record<string, unknown>;
    const rawPoints = Array.isArray(stroke.points) ? stroke.points : [];
    const points = rawPoints
      .map((point) => normalizePoint(point, width, height))
      .filter((point): point is DrawPoint => point != null);
    if (points.length === 0) continue;
    const inputKind: DrawInputKind =
      stroke.input === "pen" || stroke.input === "touch" || stroke.input === "mouse"
        ? stroke.input
        : "unknown";
    strokes.push({
      id: typeof stroke.id === "string" && stroke.id ? stroke.id.slice(0, 128) : randomUuid(),
      color: typeof stroke.color === "string" ? stroke.color.slice(0, 64) : "#e7e5e4",
      input: inputKind,
      brush: normalizeBrush(stroke.brush, stroke.width, version === 1),
      points,
    });
  }

  return {
    schema: DRAW_SCHEMA,
    version: DRAW_VERSION,
    width,
    height,
    background:
      typeof input.background === "string" ? input.background.slice(0, 64) : "transparent",
    strokes,
  };
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary);
}

function base64ToBytes(value: string): Uint8Array {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, "=");
  const binary = atob(padded);
  if (binary.length > MAX_DRAW_PAYLOAD_BYTES) throw new Error("Drawing is too large");
  return Uint8Array.from(binary, (char) => char.charCodeAt(0));
}

export function encodeDrawDocument(document: DrawDocument): string {
  const normalized = normalizeDocument(document);
  const bytes = new TextEncoder().encode(JSON.stringify(normalized));
  if (bytes.length > MAX_DRAW_PAYLOAD_BYTES) throw new Error("Drawing is too large");
  return bytesToBase64(bytes).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function decodeDrawDocument(payload: string): DrawDocument {
  const clean = payload.replace(/\s+/g, "");
  if (!clean || clean.length > Math.ceil((MAX_DRAW_PAYLOAD_BYTES * 4) / 3) + 8) {
    throw new Error(clean ? "Drawing is too large" : "Drawing payload is empty");
  }
  const json = new TextDecoder().decode(base64ToBytes(clean));
  return normalizeDocument(JSON.parse(json));
}

export function serializeDrawFenceBody(document: DrawDocument): string {
  const payload = encodeDrawDocument(document);
  const lines = payload.match(new RegExp(`.{1,${PAYLOAD_LINE_WIDTH}}`, "g")) ?? [];
  return `version: ${DRAW_VERSION}\nencoding: base64url\npayload:\n${lines
    .map((line) => `  ${line}`)
    .join("\n")}`;
}

export function serializeDrawFence(document: DrawDocument): string {
  return `\`\`\`draw\n${serializeDrawFenceBody(document)}\n\`\`\``;
}

export function parseDrawFenceBody(body: string): DrawDocument {
  const version = /^version:\s*(\d+)\s*$/im.exec(body)?.[1];
  const encoding = /^encoding:\s*([^\s]+)\s*$/im.exec(body)?.[1]?.toLowerCase();
  const marker = /^payload:\s*$/im.exec(body);
  if ((version !== "1" && version !== String(DRAW_VERSION)) || encoding !== "base64url" || !marker) {
    throw new Error("Unsupported drawing fence");
  }
  const payload = body.slice((marker.index ?? 0) + marker[0].length).replace(/\s+/g, "");
  return decodeDrawDocument(payload);
}

const DRAW_FENCE = /```draw(?:[ \t]+[^\r\n`]*)?\r?\n([\s\S]*?)\r?\n```/i;

export function findDrawFence(content: string): ParsedDrawFence | null {
  const match = DRAW_FENCE.exec(content);
  if (!match || match.index == null) return null;
  return {
    start: match.index,
    end: match.index + match[0].length,
    raw: match[0],
    body: match[1],
    document: parseDrawFenceBody(match[1]),
  };
}

export function noteHasDraw(content: string): boolean {
  try {
    return findDrawFence(content) != null;
  } catch {
    return false;
  }
}

export function noteHasDrawFence(content: string): boolean {
  return DRAW_FENCE.test(content);
}

export function drawDocumentFromContent(content: string): DrawDocument {
  try {
    return findDrawFence(content)?.document ?? createEmptyDrawDocument();
  } catch {
    return createEmptyDrawDocument();
  }
}

export function replaceDrawFence(content: string, document: DrawDocument): string {
  const fence = serializeDrawFence(document);
  const match = DRAW_FENCE.exec(content);
  if (match?.index != null) {
    return `${content.slice(0, match.index)}${fence}${content.slice(match.index + match[0].length)}`;
  }
  const trimmed = content.replace(/\s*$/, "");
  return trimmed ? `${trimmed}\n\n${fence}\n` : `${fence}\n`;
}
