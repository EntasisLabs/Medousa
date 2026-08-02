export const DRAW_SCHEMA = "medousa-draw" as const;
export const DRAW_VERSION = 1 as const;
export const DRAW_WIDTH = 1200;
export const DRAW_HEIGHT = 720;

const MAX_DRAW_PAYLOAD_BYTES = 2 * 1024 * 1024;
const PAYLOAD_LINE_WIDTH = 96;

export type DrawPoint = {
  x: number;
  y: number;
  pressure?: number;
};

export type DrawStroke = {
  id: string;
  color: string;
  width: number;
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

export function cloneDrawDocument(document: DrawDocument): DrawDocument {
  return {
    ...document,
    strokes: document.strokes.map((stroke) => ({
      ...stroke,
      points: stroke.points.map((point) => ({ ...point })),
    })),
  };
}

function finite(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function normalizeDocument(value: unknown): DrawDocument {
  if (!value || typeof value !== "object") throw new Error("Invalid drawing payload");
  const input = value as Record<string, unknown>;
  if (input.schema !== DRAW_SCHEMA || input.version !== DRAW_VERSION) {
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
      .filter((point): point is Record<string, unknown> => Boolean(point && typeof point === "object"))
      .map((point) => ({
        x: clamp(finite(point.x, 0), -width, width * 2),
        y: clamp(finite(point.y, 0), -height, height * 2),
        ...(typeof point.pressure === "number"
          ? { pressure: clamp(finite(point.pressure, 0.5), 0, 1) }
          : {}),
      }));
    if (points.length === 0) continue;
    strokes.push({
      id: typeof stroke.id === "string" && stroke.id ? stroke.id.slice(0, 128) : cryptoId(),
      color: typeof stroke.color === "string" ? stroke.color.slice(0, 64) : "#e7e5e4",
      width: clamp(finite(stroke.width, 4), 1, 80),
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

function cryptoId(): string {
  return typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : `stroke-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
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
  if (version !== String(DRAW_VERSION) || encoding !== "base64url" || !marker) {
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

export function drawStrokePath(points: DrawPoint[]): string {
  if (points.length === 0) return "";
  if (points.length === 1) {
    const point = points[0];
    return `M ${point.x} ${point.y} l 0.01 0`;
  }
  let path = `M ${points[0].x} ${points[0].y}`;
  for (let index = 1; index < points.length - 1; index += 1) {
    const point = points[index];
    const next = points[index + 1];
    path += ` Q ${point.x} ${point.y} ${(point.x + next.x) / 2} ${(point.y + next.y) / 2}`;
  }
  const last = points[points.length - 1];
  return `${path} L ${last.x} ${last.y}`;
}
