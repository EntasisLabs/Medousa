export type DrawVector = { x: number; y: number };
export type DrawSize = { width: number; height: number };
export type DrawRect = DrawVector & DrawSize;

export type DrawCamera = {
  panX: number;
  panY: number;
  zoom: number;
};

export const MIN_DRAW_ZOOM = 0.25;
export const MAX_DRAW_ZOOM = 8;

export function createDrawCamera(): DrawCamera {
  return { panX: 0, panY: 0, zoom: 1 };
}

export function clampDrawZoom(zoom: number): number {
  return Math.max(MIN_DRAW_ZOOM, Math.min(MAX_DRAW_ZOOM, zoom));
}

export function sceneToView(camera: DrawCamera, point: DrawVector): DrawVector {
  return {
    x: point.x * camera.zoom + camera.panX,
    y: point.y * camera.zoom + camera.panY,
  };
}

export function viewToScene(camera: DrawCamera, point: DrawVector): DrawVector {
  return {
    x: (point.x - camera.panX) / camera.zoom,
    y: (point.y - camera.panY) / camera.zoom,
  };
}

export function panDrawCamera(camera: DrawCamera, delta: DrawVector): DrawCamera {
  return { ...camera, panX: camera.panX + delta.x, panY: camera.panY + delta.y };
}

export function fitDrawCamera(
  bounds: DrawRect | null,
  viewport: DrawSize,
  padding = 32,
): DrawCamera {
  if (!bounds || bounds.width <= 0 || bounds.height <= 0) return createDrawCamera();
  const availableWidth = Math.max(1, viewport.width - padding * 2);
  const availableHeight = Math.max(1, viewport.height - padding * 2);
  const zoom = clampDrawZoom(Math.min(availableWidth / bounds.width, availableHeight / bounds.height));
  return {
    zoom,
    panX: viewport.width / 2 - (bounds.x + bounds.width / 2) * zoom,
    panY: viewport.height / 2 - (bounds.y + bounds.height / 2) * zoom,
  };
}

export function resizeDrawCamera(
  camera: DrawCamera,
  previousViewport: DrawSize,
  nextViewport: DrawSize,
): DrawCamera {
  const sceneCenter = viewToScene(camera, {
    x: previousViewport.width / 2,
    y: previousViewport.height / 2,
  });
  return {
    ...camera,
    panX: nextViewport.width / 2 - sceneCenter.x * camera.zoom,
    panY: nextViewport.height / 2 - sceneCenter.y * camera.zoom,
  };
}

export function zoomDrawCameraAt(
  camera: DrawCamera,
  anchor: DrawVector,
  requestedZoom: number,
): DrawCamera {
  const zoom = clampDrawZoom(requestedZoom);
  const sceneAnchor = viewToScene(camera, anchor);
  return {
    zoom,
    panX: anchor.x - sceneAnchor.x * zoom,
    panY: anchor.y - sceneAnchor.y * zoom,
  };
}
