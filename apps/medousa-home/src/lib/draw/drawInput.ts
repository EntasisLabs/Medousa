import type { DrawInputKind, DrawPoint } from "./drawDocument";

type PointerSample = Pick<
  PointerEvent,
  "clientX" | "clientY" | "pressure" | "pointerType" | "timeStamp" | "tiltX" | "tiltY"
>;

export function drawInputKind(pointerType: string): DrawInputKind {
  if (pointerType === "pen" || pointerType === "touch" || pointerType === "mouse") {
    return pointerType;
  }
  return "unknown";
}

export function expressivePenPressure(pressure: number): number {
  const normalized = Math.max(0, Math.min(1, (pressure - 0.08) / 0.62));
  return normalized * normalized * (3 - 2 * normalized);
}

export function coalescedPointerSamples(event: PointerEvent): PointerEvent[] {
  let samples: PointerEvent[] = [];
  try {
    samples = event.getCoalescedEvents?.() ?? [];
  } catch {
    samples = [];
  }
  const last = samples[samples.length - 1];
  if (!last || last.clientX !== event.clientX || last.clientY !== event.clientY || last.timeStamp !== event.timeStamp) {
    samples.push(event);
  }
  return samples;
}

export function drawPointFromPointer(
  event: PointerSample,
  mapClientPoint: (clientX: number, clientY: number) => { x: number; y: number } | null,
  startedAt: number,
): DrawPoint | null {
  const mapped = mapClientPoint(event.clientX, event.clientY);
  if (!mapped) return null;
  const input = drawInputKind(event.pointerType);
  const pressure = input === "pen" && event.pressure > 0
    ? expressivePenPressure(event.pressure)
    : undefined;
  return {
    x: Math.round(mapped.x * 100) / 100,
    y: Math.round(mapped.y * 100) / 100,
    ...(pressure != null ? { pressure: Math.round(Math.max(0, Math.min(1, pressure)) * 1000) / 1000 } : {}),
    elapsedMs: Math.max(0, Math.round(event.timeStamp - startedAt)),
    ...(input === "pen" && Number.isFinite(event.tiltX)
      ? { tiltX: Math.max(-90, Math.min(90, event.tiltX)) }
      : {}),
    ...(input === "pen" && Number.isFinite(event.tiltY)
      ? { tiltY: Math.max(-90, Math.min(90, event.tiltY)) }
      : {}),
  };
}

export function shouldDrawWithPointer(
  pointerType: string,
  fingerDraw: boolean,
  penActive: boolean,
): boolean {
  if (pointerType === "pen") return true;
  if (pointerType === "touch") return fingerDraw && !penActive;
  return !penActive;
}
