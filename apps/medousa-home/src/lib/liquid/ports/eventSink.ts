/**
 * Liquid UI — event sink port.
 *
 * Rendered nodes emit `SceneEvent`s here. Implementations route them to the
 * model's context (context path) and/or trigger binding writes (fast path).
 */

import type { SceneEvent } from "../core/events";

export interface EventSink {
  emit(event: SceneEvent): void;
}
