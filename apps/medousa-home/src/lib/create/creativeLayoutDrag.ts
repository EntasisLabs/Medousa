import type { CreativeScene } from "$lib/types/generated/daemon_api";
import type { DrawVector } from "$lib/draw/drawCamera";
import { moveCreativeSelection } from "./creativeInk";
import { selectionBounds } from "./creativeSelection";
import { layoutSnapTargets, snapLayoutMove, type LayoutGuide, type LayoutPreferences } from "./creativeLayout";

/** Adapter around the Notes movement core; cache target geometry for this gesture. */
export function creativeLayoutDrag(options: () => { settings: LayoutPreferences; zoom: number; bypass: boolean }, onGuides: (guides: LayoutGuide[]) => void) {
  let cached: { nodes: CreativeScene["nodes"]; key: string; bounds: ReturnType<typeof selectionBounds>; targets: ReturnType<typeof layoutSnapTargets> } | null = null;
  return {
    reset() { cached = null; onGuides([]); },
    move(scene: CreativeScene, ids: string[], delta: DrawVector) {
      const key = ids.join("|");
      if (!cached || cached.nodes !== scene.nodes || cached.key !== key) cached = { nodes: scene.nodes, key, bounds: selectionBounds(ids, scene.nodes), targets: layoutSnapTargets(ids, scene.nodes) };
      const { settings, zoom, bypass } = options();
      const result = cached.bounds ? snapLayoutMove(cached.bounds, cached.targets, delta, settings, zoom, bypass) : { delta, guides: [] };
      onGuides(result.guides);
      return moveCreativeSelection(scene, ids, result.delta);
    },
  };
}
