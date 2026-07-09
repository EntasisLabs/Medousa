/**
 * Liquid UI — operations (the model's verbs).
 *
 * A `SceneOp` is an idempotent, id-targeted mutation. `applyOp` is a pure
 * reducer: it never mutates its input and returns the SAME scene reference when
 * an op is a no-op or dropped (cheap change detection + Svelte-friendly).
 *
 * Ordering under parallel fill: ops may carry the owning layout `rev`. A fill
 * targeting a superseded rev (`rev < scene.rev`) is dropped. `plan_layout` sets
 * the scene rev; other ops never bump it.
 */

import {
  cloneNode,
  findNode,
  mapNode,
  removeNode,
  type Binding,
  type FillState,
  type Scene,
  type SceneNode,
} from "./scene";
import { reconcile } from "./reconcile";

export type SceneOp =
  | { op: "plan_layout"; surfaceId: string; root: SceneNode; rev: number }
  | { op: "fill_slot"; nodeId: string; slot: string; nodes: SceneNode[]; rev?: number }
  | { op: "patch_props"; nodeId: string; props: Record<string, unknown>; rev?: number }
  | { op: "set_binding"; nodeId: string; binding: Binding; rev?: number }
  | { op: "set_fill_state"; nodeId: string; state: FillState; error?: string; rev?: number }
  | { op: "precompute"; nodeId: string; variant: string; root: SceneNode; rev?: number }
  | { op: "remove"; nodeId: string; rev?: number };

/** True when a non-plan op is stale relative to the current scene rev. */
function isStale(scene: Scene, rev: number | undefined): boolean {
  return typeof rev === "number" && rev < scene.rev;
}

/** Apply one operation, returning a new scene (or the same reference if unchanged). */
export function applyOp(scene: Scene, op: SceneOp): Scene {
  switch (op.op) {
    case "plan_layout": {
      if (op.surfaceId !== scene.surfaceId) return scene;
      if (op.rev < scene.rev) return scene;
      const root = reconcile(scene.root, op.root);
      return { surfaceId: scene.surfaceId, root, rev: op.rev };
    }

    case "fill_slot": {
      if (isStale(scene, op.rev)) return scene;
      if (!scene.root) return scene;
      if (!findNode(scene.root, op.nodeId)) return scene;
      const root = mapNode(scene.root, op.nodeId, (node) => ({
        ...node,
        slots: { ...node.slots, [op.slot]: op.nodes },
      }));
      if (root === scene.root) return scene;
      return { ...scene, root };
    }

    case "patch_props": {
      if (isStale(scene, op.rev)) return scene;
      if (!scene.root) return scene;
      if (!findNode(scene.root, op.nodeId)) return scene;
      const root = mapNode(scene.root, op.nodeId, (node) => ({
        ...node,
        props: { ...node.props, ...op.props },
      }));
      if (root === scene.root) return scene;
      return { ...scene, root };
    }

    case "set_binding": {
      if (isStale(scene, op.rev)) return scene;
      if (!scene.root) return scene;
      if (!findNode(scene.root, op.nodeId)) return scene;
      const root = mapNode(scene.root, op.nodeId, (node) => ({
        ...node,
        binding: op.binding,
      }));
      if (root === scene.root) return scene;
      return { ...scene, root };
    }

    case "set_fill_state": {
      if (isStale(scene, op.rev)) return scene;
      if (!scene.root) return scene;
      if (!findNode(scene.root, op.nodeId)) return scene;
      const root = mapNode(scene.root, op.nodeId, (node) => {
        const meta = op.error
          ? { ...node.meta, error: op.error }
          : node.meta;
        return { ...node, fillState: op.state, ...(meta ? { meta } : {}) };
      });
      if (root === scene.root) return scene;
      return { ...scene, root };
    }

    case "precompute": {
      if (isStale(scene, op.rev)) return scene;
      if (!scene.root) return scene;
      if (!findNode(scene.root, op.nodeId)) return scene;
      const root = mapNode(scene.root, op.nodeId, (node) => ({
        ...node,
        precomputed: { ...node.precomputed, [op.variant]: cloneNode(op.root) },
      }));
      if (root === scene.root) return scene;
      return { ...scene, root };
    }

    case "remove": {
      if (isStale(scene, op.rev)) return scene;
      if (!scene.root) return scene;
      if (!findNode(scene.root, op.nodeId)) return scene;
      // Removing the root clears the scene.
      if (scene.root.id === op.nodeId) {
        return { ...scene, root: null };
      }
      const root = removeNode(scene.root, op.nodeId);
      if (root === scene.root) return scene;
      return { ...scene, root };
    }

    default: {
      // Exhaustiveness guard.
      const _never: never = op;
      return _never;
    }
  }
}

/** Apply a batch of ops in order. */
export function applyOps(scene: Scene, ops: SceneOp[]): Scene {
  return ops.reduce(applyOp, scene);
}
