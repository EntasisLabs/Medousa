/**
 * Liquid UI — keyed reconciliation.
 *
 * React's insight: match by key (here, `id`) to preserve identity across a
 * re-render. At the domain level that means: when the model resubmits a layout
 * (`plan_layout`), a node that still exists keeps its transient runtime state
 * (fillState, precomputed variants, binding) unless the new op explicitly
 * changes it — so a bones-first resubmit never regresses a `ready` node back to
 * `skeleton`, and never drops speculative work.
 *
 * Structure (props, slots, children, order) always comes from the NEW tree.
 */

import type { SceneNode } from "./scene";

function indexById(root: SceneNode | undefined): Map<string, SceneNode> {
  const map = new Map<string, SceneNode>();
  if (!root) return map;
  const stack: SceneNode[] = [root];
  while (stack.length > 0) {
    const node = stack.pop() as SceneNode;
    map.set(node.id, node);
    if (node.slots) {
      for (const children of Object.values(node.slots)) {
        for (const child of children) stack.push(child);
      }
    }
  }
  return map;
}

function mergeNode(next: SceneNode, prevById: Map<string, SceneNode>): SceneNode {
  const prev = prevById.get(next.id);

  // Recurse into children first (structure follows the new tree).
  let slots = next.slots;
  if (next.slots) {
    const merged: Record<string, SceneNode[]> = {};
    for (const [name, children] of Object.entries(next.slots)) {
      merged[name] = children.map((child) => mergeNode(child, prevById));
    }
    slots = merged;
  }

  // No prior node with this id, or the type changed → new node wins wholesale.
  if (!prev || prev.type !== next.type) {
    return slots === next.slots ? next : { ...next, slots };
  }

  // Matched id + type: carry transient state from prev unless new is explicit.
  const result: SceneNode = { ...next };
  if (slots !== next.slots) result.slots = slots;

  // fillState: a default `skeleton` on the new node does not clobber prior progress.
  if (next.fillState === "skeleton" && prev.fillState !== "skeleton") {
    result.fillState = prev.fillState;
  }

  // binding: keep prior binding when the new node omits one.
  if (!next.binding && prev.binding) {
    result.binding = prev.binding;
  }

  // precomputed: union, new variants win on conflict.
  if (prev.precomputed || next.precomputed) {
    result.precomputed = { ...prev.precomputed, ...next.precomputed };
  }

  return result;
}

/**
 * Merge a freshly authored tree onto the previous one, preserving transient
 * runtime state for nodes whose id and type are unchanged.
 */
export function reconcile(prev: SceneNode | null, next: SceneNode): SceneNode {
  if (!prev) return next;
  return mergeNode(next, indexById(prev));
}
