export interface PhysicalPoint {
  x: number;
  y: number;
}

export interface PhysicalExtent {
  width: number;
  height: number;
}

export interface PhysicalRect {
  position: PhysicalPoint;
  size: PhysicalExtent;
}

/**
 * Resize around the window's bottom-right corner, then keep the result inside
 * the monitor work area (excluding menu bars, docks, and taskbars).
 */
export function anchoredCompanionPosition(input: {
  position: PhysicalPoint;
  previousSize: PhysicalExtent;
  targetSize: PhysicalExtent;
  workArea?: PhysicalRect | null;
}): PhysicalPoint {
  let x = input.position.x + input.previousSize.width - input.targetSize.width;
  let y = input.position.y + input.previousSize.height - input.targetSize.height;
  const workArea = input.workArea;
  if (!workArea) return { x, y };

  const minX = workArea.position.x;
  const minY = workArea.position.y;
  const maxX = Math.max(minX, minX + workArea.size.width - input.targetSize.width);
  const maxY = Math.max(minY, minY + workArea.size.height - input.targetSize.height);
  x = Math.min(Math.max(x, minX), maxX);
  y = Math.min(Math.max(y, minY), maxY);
  return { x, y };
}
