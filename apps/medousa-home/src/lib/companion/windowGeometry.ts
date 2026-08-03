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
 * Pick the display containing most of the draggable pet. Borderless windows can
 * make the platform's `currentMonitor()` ambiguous when they straddle an edge.
 */
export function companionWorkAreaForWindow(input: {
  position: PhysicalPoint;
  size: PhysicalExtent;
  workAreas: PhysicalRect[];
}): PhysicalRect | null {
  if (input.workAreas.length === 0) return null;

  const windowCenter = {
    x: input.position.x + input.size.width / 2,
    y: input.position.y + input.size.height / 2,
  };
  let best = input.workAreas[0]!;
  let bestOverlap = -1;
  let bestDistance = Number.POSITIVE_INFINITY;

  for (const workArea of input.workAreas) {
    const overlapWidth = Math.max(
      0,
      Math.min(input.position.x + input.size.width, workArea.position.x + workArea.size.width) -
        Math.max(input.position.x, workArea.position.x),
    );
    const overlapHeight = Math.max(
      0,
      Math.min(input.position.y + input.size.height, workArea.position.y + workArea.size.height) -
        Math.max(input.position.y, workArea.position.y),
    );
    const overlap = overlapWidth * overlapHeight;
    const nearestX = Math.min(
      Math.max(windowCenter.x, workArea.position.x),
      workArea.position.x + workArea.size.width,
    );
    const nearestY = Math.min(
      Math.max(windowCenter.y, workArea.position.y),
      workArea.position.y + workArea.size.height,
    );
    const distance = (windowCenter.x - nearestX) ** 2 + (windowCenter.y - nearestY) ** 2;
    if (overlap > bestOverlap || (overlap === bestOverlap && distance < bestDistance)) {
      best = workArea;
      bestOverlap = overlap;
      bestDistance = distance;
    }
  }

  return best;
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
