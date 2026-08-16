/**
 * Liquid UI — component registry (the renderer port, Svelte-backed).
 *
 * Maps archetype id → lazy Svelte component factory. Kept separate from the pure domain
 * `core/registry` (descriptors): the domain describes capabilities, this maps to
 * a body. Descriptor imports never evaluate renderer implementations.
 */

import type { Component } from "svelte";
import type { ArchetypeId } from "$lib/liquid/core";
import type { RendererPort } from "$lib/liquid/ports";
import type { ArchetypeProps } from "./types";

export type ArchetypeComponent = Component<ArchetypeProps>;
export type ArchetypeComponentModule = { default: ArchetypeComponent };
export type ArchetypeComponentLoader = () => Promise<ArchetypeComponentModule>;

const components = new Map<ArchetypeId, ArchetypeComponent>();
const loaders = new Map<ArchetypeId, ArchetypeComponentLoader>();
const inflight = new Map<ArchetypeId, Promise<ArchetypeComponent | null>>();

export function registerComponent(id: ArchetypeId, component: ArchetypeComponent): void {
  components.set(id, component);
}

export function registerComponentLoader(
  id: ArchetypeId,
  loader: ArchetypeComponentLoader,
): void {
  loaders.set(id, loader);
}

export function resolveComponent(id: ArchetypeId): ArchetypeComponent | null {
  return components.get(id) ?? null;
}

export function hasComponent(id: ArchetypeId): boolean {
  return components.has(id) || loaders.has(id);
}

export function loadComponent(id: ArchetypeId): Promise<ArchetypeComponent | null> {
  const component = components.get(id);
  if (component) return Promise.resolve(component);
  const pending = inflight.get(id);
  if (pending) return pending;
  const loader = loaders.get(id);
  if (!loader) return Promise.resolve(null);
  const promise = loader()
    .then((module) => {
      components.set(id, module.default);
      return module.default;
    })
    .finally(() => {
      inflight.delete(id);
    });
  inflight.set(id, promise);
  return promise;
}

/** The renderer port implementation over the Svelte component map. */
export const componentRegistry: RendererPort<ArchetypeComponent> = {
  resolve: resolveComponent,
};
