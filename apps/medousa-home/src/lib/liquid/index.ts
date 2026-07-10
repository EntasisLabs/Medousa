/**
 * Liquid UI — semantic scene graph the model authors at runtime.
 *
 * Architecture is isomorphic to the runtime: `core/` is pure domain, `ports/`
 * are the hexagonal seams, and (from PR2) `archetypes/` mirror the vocabulary
 * one module per node type.
 */

export * from "./core";
export * from "./ports";
