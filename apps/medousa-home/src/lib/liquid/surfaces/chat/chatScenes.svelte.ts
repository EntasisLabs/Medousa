/**
 * Per-message daemon-authored scene store.
 *
 * When the daemon streams `ui_scene` ops for a turn, we accumulate them into a
 * live `Scene` keyed by the chat message id. `LiquidChatMessage` prefers this
 * daemon scene over the client-side `messageToScene` adapter — so a
 * model-authored structured turn renders bones-first and fills in place, while
 * turns without a scene fall back to the deterministic adapter (PR3).
 *
 * State lives in a rune so consumers re-render as ops arrive.
 */

import { applyOps, createScene, type Scene } from "$lib/liquid/core";
import { decodeSceneOps } from "./sceneStream";

class ChatSceneStore {
  private scenes = $state<Record<string, Scene>>({});

  /** The accumulated scene for a message, or null if the daemon authored none. */
  get(messageId: string): Scene | null {
    return this.scenes[messageId] ?? null;
  }

  /** True when a renderable (non-empty) daemon scene exists for the message. */
  has(messageId: string): boolean {
    return this.scenes[messageId]?.root != null;
  }

  /**
   * Apply a streamed batch of wire ops to a message's scene. The client owns the
   * surface id (derived from the turn), so `plan_layout` ops are stamped with it
   * regardless of what the model emitted.
   */
  applyWire(messageId: string, surfaceId: string, wireOps: unknown[]): void {
    const ops = decodeSceneOps(wireOps, surfaceId);
    if (ops.length === 0) return;
    const current = this.scenes[messageId] ?? createScene(surfaceId);
    const next = applyOps(current, ops);
    if (next === current) return;
    this.scenes = { ...this.scenes, [messageId]: next };
  }

  clear(messageId: string): void {
    if (!(messageId in this.scenes)) return;
    const { [messageId]: _dropped, ...rest } = this.scenes;
    this.scenes = rest;
  }

  reset(): void {
    if (Object.keys(this.scenes).length === 0) return;
    this.scenes = {};
  }
}

export const chatScenes = new ChatSceneStore();
