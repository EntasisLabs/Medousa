/**
 * Liquid UI — render context.
 *
 * Threaded once at the scene root and inherited by every nested renderer via
 * Svelte context (no prop drilling). Carries cross-cutting concerns the domain
 * must not know about: the event sink, markdown link handling, session id.
 */

import { getContext, setContext } from "svelte";
import type { EventSink } from "$lib/liquid/ports";

export interface LiquidRenderContext {
  /** Where node events (select/edit/run/…) are emitted. */
  sink?: EventSink;
  /** Wikilink title resolution for prose markdown. */
  titleByPath?: Map<string, string>;
  /** Route http(s) links through the Web surface instead of a new tab. */
  openLinksInWeb?: boolean;
  /** Session id for media / artifact binding fetches. */
  sessionId?: string;
}

const KEY = Symbol("liquid.render.context");

export function setLiquidContext(context: LiquidRenderContext): void {
  setContext(KEY, context);
}

export function getLiquidContext(): LiquidRenderContext {
  return getContext<LiquidRenderContext | undefined>(KEY) ?? {};
}
