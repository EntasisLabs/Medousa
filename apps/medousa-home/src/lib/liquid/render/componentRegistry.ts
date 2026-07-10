/**
 * Liquid UI — component registry (the renderer port, Svelte-backed).
 *
 * Maps archetype id → Svelte component. Kept separate from the pure domain
 * `core/registry` (descriptors): the domain describes capabilities, this maps to
 * a body. Archetype modules self-register into both at import time.
 */

import type { Component } from "svelte";
import type { ArchetypeId } from "$lib/liquid/core";
import type { RendererPort } from "$lib/liquid/ports";
import type { ArchetypeProps } from "./types";

export type ArchetypeComponent = Component<ArchetypeProps>;

const components = new Map<ArchetypeId, ArchetypeComponent>();

export function registerComponent(id: ArchetypeId, component: ArchetypeComponent): void {
  components.set(id, component);
}

export function resolveComponent(id: ArchetypeId): ArchetypeComponent | null {
  return components.get(id) ?? null;
}

export function hasComponent(id: ArchetypeId): boolean {
  return components.has(id);
}

/** The renderer port implementation over the Svelte component map. */
export const componentRegistry: RendererPort<ArchetypeComponent> = {
  resolve: resolveComponent,
};
