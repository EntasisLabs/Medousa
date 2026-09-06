import type {
  BrowserObservationViewport,
  IsolatedBrowserWorld,
} from "$lib/daemon/browserWorlds";

export type BrowserSurfaceSource =
  | { kind: "device" }
  | { kind: "workshop"; worldId: string; runtimeId: string };

export interface BrowserSourcePreference {
  source: BrowserSurfaceSource;
  selectedAt: number;
  browserDriverId?: string;
}

export function chooseBrowserSurfaceSource(
  worlds: IsolatedBrowserWorld[],
  preference: BrowserSourcePreference | null,
  runtimeId: string,
): BrowserSurfaceSource {
  const preferredSource = preference?.source;
  const preferredRuntimeId =
    preferredSource?.kind === "workshop" ? preferredSource.runtimeId : null;
  const preferredWorld =
    preferredSource?.kind === "workshop"
      ? worlds.find((world) => world.world_id === preferredSource.worldId)
      : null;
  if (preferredWorld && preferredRuntimeId) {
    return {
      kind: "workshop",
      worldId: preferredWorld.world_id,
      runtimeId: preferredRuntimeId,
    };
  }

  const newlyAttached = [...worlds]
    .filter(
      (world) =>
        world.view_attached &&
        world.run_state !== "failed" &&
        world.updated_at_ms > (preference?.selectedAt ?? 0),
    )
    .sort((left, right) => right.updated_at_ms - left.updated_at_ms)[0];
  if (newlyAttached) {
    return { kind: "workshop", worldId: newlyAttached.world_id, runtimeId };
  }

  return { kind: "device" };
}

export function pointInCssViewport(
  clientX: number,
  clientY: number,
  bounds: Pick<DOMRect, "left" | "top" | "width" | "height">,
  viewport: BrowserObservationViewport,
): { x: number; y: number } | null {
  if (bounds.width <= 0 || bounds.height <= 0 || viewport.width <= 0 || viewport.height <= 0) {
    return null;
  }
  const imageRatio = viewport.width / viewport.height;
  const boundsRatio = bounds.width / bounds.height;
  const renderedWidth = boundsRatio > imageRatio ? bounds.height * imageRatio : bounds.width;
  const renderedHeight = boundsRatio > imageRatio ? bounds.height : bounds.width / imageRatio;
  const left = bounds.left + (bounds.width - renderedWidth) / 2;
  const top = bounds.top + (bounds.height - renderedHeight) / 2;
  const relativeX = clientX - left;
  const relativeY = clientY - top;
  if (
    relativeX < 0 ||
    relativeY < 0 ||
    relativeX > renderedWidth ||
    relativeY > renderedHeight
  ) {
    return null;
  }
  return {
    x: (relativeX / renderedWidth) * viewport.width,
    y: (relativeY / renderedHeight) * viewport.height,
  };
}
