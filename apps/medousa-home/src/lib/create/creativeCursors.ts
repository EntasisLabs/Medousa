/** A visible rotation affordance, including while pointer capture moves off the handle. */
export const rotationCursor = `url("data:image/svg+xml,${encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32"><g fill="none" stroke-linecap="round" stroke-linejoin="round"><path d="M24 14a9 9 0 1 0-3 9M24 6v8h-8" stroke="white" stroke-width="5"/><path d="M24 14a9 9 0 1 0-3 9M24 6v8h-8" stroke="#242424" stroke-width="2"/></g></svg>')}") 16 16, crosshair`;
export function resizeCursor(handle: string, angle: number): string {
  const direction = { e: 0, w: 0, ne: -45, sw: -45, n: 90, s: 90, nw: 45, se: 45 }[handle] ?? 0;
  return ["ew-resize", "nwse-resize", "ns-resize", "nesw-resize"][((Math.round((angle + direction) / 45) % 4) + 4) % 4];
}
