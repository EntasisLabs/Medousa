/** Liquid UI — renderer-side shared types. */

import type { SceneNode } from "$lib/liquid/core";

/** Every archetype renderer receives exactly its node; context comes via Svelte context. */
export interface ArchetypeProps {
  node: SceneNode;
}
