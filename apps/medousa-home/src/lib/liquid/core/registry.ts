/**
 * Liquid UI — archetype registry.
 *
 * Each archetype module exports a descriptor and self-registers. The registry is
 * the seam shared by the model (what a node may do), the renderer (how to draw
 * it), and validation. This module is pure domain — renderers attach separately.
 */

import type { ArchetypeId, BindingSource, Owner, SceneNode } from "./scene";
import type { SceneEventType } from "./events";

export type ArchetypeTier = "atom" | "molecule" | "layout" | "shell" | "organism";

export type PropType = "string" | "number" | "boolean" | "object" | "array";

export interface PropSpec {
  type: PropType;
  required?: boolean;
}

export interface ArchetypeDescriptor {
  id: ArchetypeId;
  tier: ArchetypeTier;
  /** Prop name → spec. Permissive: unknown props are allowed unless strict. */
  props?: Record<string, PropSpec>;
  acceptsBindings: BindingSource[];
  writeCapable: boolean;
  /** Named child regions this archetype hosts. */
  slots: string[];
  emits: SceneEventType[];
  virtualization: "none" | "window";
  defaultOwner: Owner;
}

export interface ValidationIssue {
  nodeId: string;
  code:
    | "unknown_type"
    | "binding_not_accepted"
    | "unknown_slot"
    | "missing_required_prop";
  message: string;
}

export class ArchetypeRegistry {
  private readonly map = new Map<ArchetypeId, ArchetypeDescriptor>();

  register(descriptor: ArchetypeDescriptor): void {
    this.map.set(descriptor.id, descriptor);
  }

  get(id: ArchetypeId): ArchetypeDescriptor | undefined {
    return this.map.get(id);
  }

  has(id: ArchetypeId): boolean {
    return this.map.has(id);
  }

  all(): ArchetypeDescriptor[] {
    return [...this.map.values()];
  }

  clear(): void {
    this.map.clear();
  }
}

/** Shared process-wide registry; archetype modules register into this. */
export const registry = new ArchetypeRegistry();

/** Convenience for archetype modules: register and return the descriptor. */
export function defineArchetype(
  descriptor: ArchetypeDescriptor,
  target: ArchetypeRegistry = registry,
): ArchetypeDescriptor {
  target.register(descriptor);
  return descriptor;
}

/**
 * Validate a single node against a registry. Does not recurse — callers walk the
 * tree. Returns an empty array when the node is well-formed.
 */
export function validateNode(
  node: SceneNode,
  target: ArchetypeRegistry = registry,
): ValidationIssue[] {
  const issues: ValidationIssue[] = [];
  const descriptor = target.get(node.type);

  if (!descriptor) {
    issues.push({
      nodeId: node.id,
      code: "unknown_type",
      message: `Unknown archetype "${node.type}"`,
    });
    return issues;
  }

  if (node.binding && !descriptor.acceptsBindings.includes(node.binding.source)) {
    issues.push({
      nodeId: node.id,
      code: "binding_not_accepted",
      message: `Archetype "${node.type}" does not accept binding source "${node.binding.source}"`,
    });
  }

  if (node.slots) {
    for (const slot of Object.keys(node.slots)) {
      if (!descriptor.slots.includes(slot)) {
        issues.push({
          nodeId: node.id,
          code: "unknown_slot",
          message: `Archetype "${node.type}" has no slot "${slot}"`,
        });
      }
    }
  }

  if (descriptor.props) {
    for (const [name, spec] of Object.entries(descriptor.props)) {
      if (spec.required && !(name in node.props)) {
        issues.push({
          nodeId: node.id,
          code: "missing_required_prop",
          message: `Archetype "${node.type}" requires prop "${name}"`,
        });
      }
    }
  }

  return issues;
}

/** Validate an entire tree against a registry. */
export function validateTree(
  root: SceneNode,
  target: ArchetypeRegistry = registry,
): ValidationIssue[] {
  const issues: ValidationIssue[] = [];
  const stack: SceneNode[] = [root];
  while (stack.length > 0) {
    const node = stack.pop() as SceneNode;
    issues.push(...validateNode(node, target));
    if (node.slots) {
      for (const children of Object.values(node.slots)) {
        for (const child of children) stack.push(child);
      }
    }
  }
  return issues;
}
