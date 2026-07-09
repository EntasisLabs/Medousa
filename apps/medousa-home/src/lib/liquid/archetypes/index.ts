/**
 * Liquid UI archetype barrel — importing this registers the vocabulary into both
 * the domain descriptor registry and the Svelte component registry (side effects
 * at import). Consumers import this once before rendering a scene.
 */

export { prose } from "./atoms/prose/prose";
export { statusPill } from "./atoms/status_pill/statusPill";
export { media } from "./atoms/media/media";
export { stack } from "./layout/stack/stack";
