/**
 * Scene stream decoder — validates opaque wire JSON into typed `SceneOp`s.
 *
 * The daemon forwards model-authored scene operations verbatim (in the same
 * JSON shape as `SceneOp`), so the trust boundary lives here: anything that
 * doesn't validate is dropped rather than rendered. This is the security
 * dividend of a constrained vocabulary — the model emits typed nodes, never
 * markup.
 */

import type { Binding, BindingSource, FillState, Owner, SceneNode } from "$lib/liquid/core";
import type { SceneOp } from "$lib/liquid/core";

const FILL_STATES: FillState[] = ["skeleton", "streaming", "ready", "error", "stale"];
const OWNERS: Owner[] = ["app", "agent", "user"];
const BINDING_SOURCES: BindingSource[] = [
  "vault:path",
  "vault:query",
  "work:board",
  "work:card",
  "work:lineage",
  "automations:flows",
  "automations:runs",
  "automations:script",
  "context:graph",
  "identity:graph",
  "feed:id",
  "artifact:id",
  "tool:ref",
  "inline",
];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function asString(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function coerceBinding(value: unknown): Binding | undefined {
  if (!isRecord(value)) return undefined;
  const source = value.source;
  if (typeof source !== "string" || !BINDING_SOURCES.includes(source as BindingSource)) {
    return undefined;
  }
  const mode = value.mode === "readwrite" ? "readwrite" : "read";
  const binding: Binding = { source: source as BindingSource, mode };
  if (typeof value.ref === "string") binding.ref = value.ref;
  if (typeof value.live === "boolean") binding.live = value.live;
  if (isRecord(value.window)) {
    const { offset, limit } = value.window;
    if (typeof offset === "number" && typeof limit === "number") {
      binding.window = { offset, limit };
    }
  }
  if ("inline" in value) binding.inline = value.inline;
  return binding;
}

/** Validate an untrusted value into a `SceneNode`, or null if malformed. */
export function coerceNode(value: unknown): SceneNode | null {
  if (!isRecord(value)) return null;
  const id = asString(value.id);
  const type = asString(value.type);
  if (!id || !type) return null;

  const node: SceneNode = {
    id,
    type,
    props: isRecord(value.props) ? value.props : {},
    fillState: FILL_STATES.includes(value.fillState as FillState)
      ? (value.fillState as FillState)
      : "skeleton",
    owner: OWNERS.includes(value.owner as Owner) ? (value.owner as Owner) : "agent",
  };

  const binding = coerceBinding(value.binding);
  if (binding) node.binding = binding;

  if (isRecord(value.meta)) {
    node.meta = {};
    if (typeof value.meta.label === "string") node.meta.label = value.meta.label;
    if (typeof value.meta.icon === "string") node.meta.icon = value.meta.icon;
    if (typeof value.meta.error === "string") node.meta.error = value.meta.error;
  }

  if (isRecord(value.slots)) {
    const slots: Record<string, SceneNode[]> = {};
    for (const [name, children] of Object.entries(value.slots)) {
      if (!Array.isArray(children)) continue;
      slots[name] = children
        .map(coerceNode)
        .filter((child): child is SceneNode => child !== null);
    }
    node.slots = slots;
  }

  return node;
}

function optionalRev(value: unknown): number | undefined {
  return typeof value === "number" ? value : undefined;
}

/** Decode a single wire op into a typed `SceneOp`, or null if invalid. */
export function decodeSceneOp(value: unknown, surfaceOverride?: string): SceneOp | null {
  if (!isRecord(value)) return null;
  const op = value.op;

  switch (op) {
    case "plan_layout": {
      const root = coerceNode(value.root);
      if (!root) return null;
      const surfaceId = surfaceOverride ?? asString(value.surfaceId);
      if (!surfaceId) return null;
      const rev = typeof value.rev === "number" ? value.rev : 0;
      return { op: "plan_layout", surfaceId, root, rev };
    }
    case "fill_slot": {
      const nodeId = asString(value.nodeId);
      const slot = asString(value.slot);
      if (!nodeId || !slot || !Array.isArray(value.nodes)) return null;
      const nodes = value.nodes
        .map(coerceNode)
        .filter((node): node is SceneNode => node !== null);
      return { op: "fill_slot", nodeId, slot, nodes, rev: optionalRev(value.rev) };
    }
    case "patch_props": {
      const nodeId = asString(value.nodeId);
      if (!nodeId || !isRecord(value.props)) return null;
      return { op: "patch_props", nodeId, props: value.props, rev: optionalRev(value.rev) };
    }
    case "set_binding": {
      const nodeId = asString(value.nodeId);
      const binding = coerceBinding(value.binding);
      if (!nodeId || !binding) return null;
      return { op: "set_binding", nodeId, binding, rev: optionalRev(value.rev) };
    }
    case "set_fill_state": {
      const nodeId = asString(value.nodeId);
      const state = value.state;
      if (!nodeId || !FILL_STATES.includes(state as FillState)) return null;
      return {
        op: "set_fill_state",
        nodeId,
        state: state as FillState,
        ...(typeof value.error === "string" ? { error: value.error } : {}),
        rev: optionalRev(value.rev),
      };
    }
    case "precompute": {
      const nodeId = asString(value.nodeId);
      const variant = asString(value.variant);
      const root = coerceNode(value.root);
      if (!nodeId || !variant || !root) return null;
      return { op: "precompute", nodeId, variant, root, rev: optionalRev(value.rev) };
    }
    case "remove": {
      const nodeId = asString(value.nodeId);
      if (!nodeId) return null;
      return { op: "remove", nodeId, rev: optionalRev(value.rev) };
    }
    default:
      return null;
  }
}

/**
 * Decode an ordered batch of wire ops, dropping invalid entries. When
 * `surfaceOverride` is set, every `plan_layout` is stamped with it so the scene
 * surface always matches (the client owns the surface id, not the model).
 */
export function decodeSceneOps(ops: unknown[], surfaceOverride?: string): SceneOp[] {
  const result: SceneOp[] = [];
  for (const raw of ops) {
    const decoded = decodeSceneOp(raw, surfaceOverride);
    if (decoded) result.push(decoded);
  }
  return result;
}
