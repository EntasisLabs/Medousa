export type DrawVector = { x: number; y: number };

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
