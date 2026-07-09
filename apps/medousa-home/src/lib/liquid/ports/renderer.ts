/**
 * Liquid UI — renderer port.
 *
 * Maps an archetype id to a concrete renderer (a Svelte component in this app).
 * Kept framework-agnostic so the domain never imports Svelte. PR2 provides the
 * Svelte-backed implementation.
 */

import type { ArchetypeId } from "../core/scene";

export interface RendererPort<TComponent = unknown> {
  /** Resolve an archetype id to its renderer, or null if unregistered. */
  resolve(type: ArchetypeId): TComponent | null;
}
