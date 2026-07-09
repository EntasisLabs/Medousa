/**
 * Liquid UI — scene domain.
 *
 * The scene graph is a semantic, addressable tree the model authors at runtime.
 * This module is pure domain: no daemon, no Svelte, no I/O. It defines the node
 * shape and immutable traversal helpers. Rendering, bindings, and transport live
 * behind ports (see `../ports`).
 */

/** Lifecycle of a node's content. `skeleton` = bones painted, awaiting fill. */
export type FillState = "skeleton" | "streaming" | "ready" | "error" | "stale";

/** Who may mutate a node. The model may only touch `agent`/`user` nodes. */
export type Owner = "app" | "agent" | "user";

/**
 * Archetype identifier — the vocabulary word shared across the model's ops, the
 * runtime node, and the source module. Validated against the registry rather
 * than a closed union so new archetypes are drop-in.
 */
export type ArchetypeId = string;

/** Where a node's live data comes from. Each source is an adapter over the daemon. */
export type BindingSource =
  | "vault:path"
  | "vault:query"
  | "work:board"
  | "work:card"
  | "work:lineage"
  | "automations:flows"
  | "automations:runs"
  | "automations:script"
  | "context:graph"
  | "identity:graph"
  | "feed:id"
  | "artifact:id"
  | "tool:ref"
  | "inline";

export type BindingMode = "read" | "readwrite";

export interface BindingWindow {
  offset: number;
  limit: number;
}

export interface Binding {
  source: BindingSource;
  /** Path / id / query / feed id, depending on `source`. */
  ref?: string;
  mode: BindingMode;
  /** Subscribe to feed/SSE updates. */
  live?: boolean;
  /** Virtualization window (perf in the contract). */
  window?: BindingWindow;
  /** Data payload when `source === "inline"`. */
  inline?: unknown;
}

export interface SceneNodeMeta {
  label?: string;
  icon?: string;
  error?: string;
}

/** A single addressable node. `id` doubles as the reconciliation key. */
export interface SceneNode {
  id: string;
  type: ArchetypeId;
  props: Record<string, unknown>;
  binding?: Binding;
  /** Named child regions. */
  slots?: Record<string, SceneNode[]>;
  fillState: FillState;
  owner: Owner;
  meta?: SceneNodeMeta;
  /** Speculative offscreen variants (generate-more-than-show). Keyed by variant name. */
  precomputed?: Record<string, SceneNode>;
}

/** A surface's live scene. `rev` is the current layout revision. */
export interface Scene {
  surfaceId: string;
  root: SceneNode | null;
  rev: number;
}

export interface CreateNodeInput {
  id: string;
  type: ArchetypeId;
  props?: Record<string, unknown>;
  binding?: Binding;
  slots?: Record<string, SceneNode[]>;
  fillState?: FillState;
  owner?: Owner;
  meta?: SceneNodeMeta;
  precomputed?: Record<string, SceneNode>;
}

/** Build a node with sensible defaults (skeleton, agent-owned). */
export function createNode(input: CreateNodeInput): SceneNode {
  const node: SceneNode = {
    id: input.id,
    type: input.type,
    props: input.props ?? {},
    fillState: input.fillState ?? "skeleton",
    owner: input.owner ?? "agent",
  };
  if (input.binding) node.binding = input.binding;
  if (input.slots) node.slots = input.slots;
  if (input.meta) node.meta = input.meta;
  if (input.precomputed) node.precomputed = input.precomputed;
  return node;
}

/** Create an empty scene for a surface. `plan_layout` sets the root. */
export function createScene(surfaceId: string): Scene {
  return { surfaceId, root: null, rev: 0 };
}

/** Deep clone a node (structural; drops nothing). */
export function cloneNode(node: SceneNode): SceneNode {
  const clone: SceneNode = {
    id: node.id,
    type: node.type,
    props: { ...node.props },
    fillState: node.fillState,
    owner: node.owner,
  };
  if (node.binding) clone.binding = { ...node.binding };
  if (node.meta) clone.meta = { ...node.meta };
  if (node.slots) {
    const slots: Record<string, SceneNode[]> = {};
    for (const [name, children] of Object.entries(node.slots)) {
      slots[name] = children.map(cloneNode);
    }
    clone.slots = slots;
  }
  if (node.precomputed) {
    const pre: Record<string, SceneNode> = {};
    for (const [variant, sub] of Object.entries(node.precomputed)) {
      pre[variant] = cloneNode(sub);
    }
    clone.precomputed = pre;
  }
  return clone;
}

/** Depth-first visit. Return `false` from the visitor to stop descending. */
export function walk(node: SceneNode, visitor: (node: SceneNode) => boolean | void): void {
  const proceed = visitor(node);
  if (proceed === false) return;
  if (!node.slots) return;
  for (const children of Object.values(node.slots)) {
    for (const child of children) {
      walk(child, visitor);
    }
  }
}

/** Find a node by id anywhere in the tree. */
export function findNode(root: SceneNode | null, id: string): SceneNode | null {
  if (!root) return null;
  let found: SceneNode | null = null;
  walk(root, (node) => {
    if (node.id === id) {
      found = node;
      return false;
    }
  });
  return found;
}

/**
 * Return a new tree with the node matching `id` replaced by `fn(node)`.
 * Referential equality is preserved for untouched subtrees. If `fn` returns the
 * same reference or the id is absent, the original tree reference is returned.
 */
export function mapNode(
  root: SceneNode,
  id: string,
  fn: (node: SceneNode) => SceneNode,
): SceneNode {
  if (root.id === id) {
    return fn(root);
  }
  if (!root.slots) return root;
  let changed = false;
  const slots: Record<string, SceneNode[]> = {};
  for (const [name, children] of Object.entries(root.slots)) {
    let childChanged = false;
    const mapped = children.map((child) => {
      const next = mapNode(child, id, fn);
      if (next !== child) childChanged = true;
      return next;
    });
    slots[name] = childChanged ? mapped : children;
    if (childChanged) changed = true;
  }
  if (!changed) return root;
  return { ...root, slots };
}

/**
 * Return a new tree with the node matching `id` removed from its parent slot.
 * The root itself cannot be removed (use `plan_layout` to replace it); the
 * original reference is returned if the id is the root or absent.
 */
export function removeNode(root: SceneNode, id: string): SceneNode {
  if (root.id === id) return root;
  if (!root.slots) return root;
  let changed = false;
  const slots: Record<string, SceneNode[]> = {};
  for (const [name, children] of Object.entries(root.slots)) {
    const filtered: SceneNode[] = [];
    let slotChanged = false;
    for (const child of children) {
      if (child.id === id) {
        slotChanged = true;
        changed = true;
        continue;
      }
      const next = removeNode(child, id);
      if (next !== child) {
        slotChanged = true;
        changed = true;
      }
      filtered.push(next);
    }
    slots[name] = slotChanged ? filtered : children;
  }
  if (!changed) return root;
  return { ...root, slots };
}

/** Collect every node id in the tree (depth-first order). */
export function collectNodeIds(root: SceneNode | null, ids: string[] = []): string[] {
  if (!root) return ids;
  walk(root, (node) => {
    ids.push(node.id);
  });
  return ids;
}
